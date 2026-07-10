//! Think-mode prompt assembly and response extraction.
//!
//! This module implements an opt-in "think first" mode for [`ConversationRuntime`].
//! When think-mode is enabled, [`assemble_thinking_scaffold`] augments the system
//! prompt with a directive that instructs the model to reason step-by-step
//! *before* producing visible output, and to wrap that reasoning in a
//! `<thinking>...</thinking>` block followed by the visible reply.
//!
//! The matching response parser [`extract_thinking`] splits a model reply into
//! the structured [`ThinkingExtraction`] containing both the captured thinking
//! and the visible portion. This pairs with the existing
//! `ContentBlock::Thinking` channel which the runtime already exposes via
//! `AssistantEvent::Thinking`; this module is the text-level fallback used
//! when the provider returns the thinking inline with the text block.
//!
//! Errors are returned via [`ThinkingError`] so callers can distinguish
//! malformed model output from "no thinking was emitted".

#![allow(clippy::result_large_err)]

use std::fmt::{Display, Formatter};

const THINK_DIRECTIVE: &str = "\
You operate in THINK MODE for this conversation.

Before producing any visible reply, you MUST reason step-by-step about what \
the user is asking, what information is needed, and what the correct next \
action is (e.g. which tool to call, what facts to verify, what assumptions to \
challenge). Do not skip this step.

Format every assistant turn EXACTLY as:

  <thinking>
  ...your private reasoning, step by step...
  </thinking>
  ...the visible reply to the user (or your tool-use block)...

Rules:
- The `<thinking>...</thinking>` block MUST be well-formed: a single opening \
tag, a single closing tag, in that order, with no nesting.
- The visible reply MUST come AFTER the closing tag. Do not interleave \
visible text and thinking blocks.
- If you cannot solve the problem, say so in the visible reply rather than \
emitting malformed thinking blocks.
- Do not include additional `<thinking>` blocks later in the reply; one block \
per assistant turn, placed at the start.";

const THINK_TAG_OPEN: &str = "<thinking>";
const THINK_TAG_CLOSE: &str = "</thinking>";

/// Errors returned by [`extract_thinking`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThinkingError {
    /// The model emitted more than one `<thinking>` block. We require exactly
    /// one per turn.
    MultipleBlocks,
    /// A `<thinking>` opening tag was not matched by a corresponding closing
    /// tag (i.e. an unclosed block).
    UnclosedBlock,
}

impl Display for ThinkingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MultipleBlocks => {
                write!(f, "model emitted multiple <thinking> blocks in one reply")
            }
            Self::UnclosedBlock => {
                write!(
                    f,
                    "model emitted an unclosed <thinking> block (missing </thinking>)"
                )
            }
        }
    }
}

impl std::error::Error for ThinkingError {}

/// Structured result of splitting a model reply into hidden thinking and the
/// user-visible portion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingExtraction {
    /// The captured reasoning. Empty when the model produced no thinking block.
    pub thinking: String,
    /// The visible reply — anything outside the thinking block. Equivalent to
    /// the entire input when no thinking block is present.
    pub visible: String,
}

/// Prepend a think-first directive to the existing system prompt.
///
/// `user_msg` is returned unchanged; only the system prompt is augmented so
/// downstream callers don't have to mutate the user transcript just to enable
/// think-mode.
///
/// # Arguments
///
/// * `system` — the existing system prompt (may be empty).
/// * `user_msg` — the unmodified user message to pass through.
///
/// # Returns
///
/// A `(augmented_system, user_msg)` tuple. `user_msg` is returned verbatim.
#[must_use]
pub fn assemble_thinking_scaffold(system: &str, user_msg: &str) -> (String, String) {
    let mut augmented = String::with_capacity(THINK_DIRECTIVE.len() + system.len() + 8);
    augmented.push_str(THINK_DIRECTIVE);
    if !system.is_empty() {
        augmented.push_str("\n\n---\n\n");
        augmented.push_str(system);
    }
    (augmented, user_msg.to_string())
}

/// Split a model reply into captured thinking and visible text.
///
/// Behavior:
/// - **No `<thinking>` block** → `Ok` with `thinking=""` and
///   `visible=response_text` (passthrough).
/// - **One well-formed `<thinking>...</thinking>` block** → `Ok` with the
///   block content as `thinking` and everything else joined as `visible`.
/// - **Multiple `<thinking>` blocks** → `Err(ThinkingError::MultipleBlocks)`.
/// - **An opening tag with no matching close tag** →
///   `Err(ThinkingError::UnclosedBlock)`.
///
/// Pre-existing visible content that appears before the thinking block is
/// preserved as part of `visible`. Whitespace adjacent to the tags is trimmed
/// from the captured thinking but kept intact in the visible text.
pub fn extract_thinking(response_text: &str) -> Result<ThinkingExtraction, ThinkingError> {
    // Quick scan: count occurrences. Anything other than exactly one open /
    // one close pair is malformed — but distinguish "no close" (unclosed)
    // from "multiple" (more than one of either).
    let open_positions: Vec<usize> = find_all(response_text, THINK_TAG_OPEN);
    let close_positions: Vec<usize> = find_all(response_text, THINK_TAG_CLOSE);

    if open_positions.is_empty() && close_positions.is_empty() {
        return Ok(ThinkingExtraction {
            thinking: String::new(),
            visible: response_text.to_string(),
        });
    }

    // Distinguish: exactly one open + zero close = unclosed.
    // Multiple of either = multiple blocks.
    let has_multiple = open_positions.len() > 1 || close_positions.len() > 1;
    let has_unclosed = open_positions.len() == 1 && close_positions.is_empty();
    if has_multiple {
        return Err(ThinkingError::MultipleBlocks);
    }
    if has_unclosed {
        return Err(ThinkingError::UnclosedBlock);
    }
    if open_positions.len() != 1 || close_positions.len() != 1 {
        // Other malformed combinations (e.g. zero open + one close) are
        // structurally invalid; surface as unclosed.
        return Err(ThinkingError::UnclosedBlock);
    }

    let open_pos = open_positions[0];
    let close_pos = close_positions[0];

    if close_pos < open_pos {
        // The close tag arrived before the open tag in linear order, which is
        // structurally impossible — treat as malformed rather than empty
        // extraction so the caller notices.
        return Err(ThinkingError::UnclosedBlock);
    }

    let inside_start = open_pos + THINK_TAG_OPEN.len();
    // Empty block (`<thinking></thinking>`): we strip the tags and treat
    // the block as no thinking emitted. This is what callers expect.
    let thinking_raw = &response_text[inside_start..close_pos];
    let thinking = thinking_raw.trim().to_string();

    let before = &response_text[..open_pos];
    let after = &response_text[close_pos + THINK_TAG_CLOSE.len()..];
    let mut visible = String::with_capacity(before.len() + after.len());
    visible.push_str(before);
    visible.push_str(after);
    // Trim only the leading/trailing whitespace introduced by tag adjacency.
    let visible = visible.trim().to_string();

    Ok(ThinkingExtraction { thinking, visible })
}

fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut positions = Vec::new();
    let bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.len() > bytes.len() {
        return positions;
    }
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        if &bytes[i..i + needle_bytes.len()] == needle_bytes {
            positions.push(i);
            i += needle_bytes.len();
        } else {
            i += 1;
        }
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_includes_think_directive() {
        let (system, user) = assemble_thinking_scaffold("You are helpful.", "hello");
        assert!(
            system.contains("THINK MODE"),
            "system prompt must contain think-first directive header; got: {system}"
        );
        assert!(
            system.contains("step-by-step"),
            "system prompt must instruct step-by-step reasoning; got: {system}"
        );
        assert!(
            system.contains("<thinking>") && system.contains("</thinking>"),
            "system prompt must describe the <thinking>...</thinking> format; got: {system}"
        );
        assert!(
            system.contains("You are helpful."),
            "original system prompt must be preserved; got: {system}"
        );
        assert_eq!(user, "hello", "user message must be returned unchanged");
    }

    #[test]
    fn scaffold_preserves_empty_system() {
        let (system, _) = assemble_thinking_scaffold("", "hi");
        assert!(system.starts_with(THINK_DIRECTIVE));
        // No `\n\n---\n\n` separator when the original is empty.
        assert!(!system.contains("\n---\n"));
    }

    #[test]
    fn extract_well_formed() {
        let response = "<thinking>\nLet me think about 2+2.\n</thinking>\nThe answer is 4.";
        let got = extract_thinking(response).expect("single block should parse");
        assert_eq!(got.thinking, "Let me think about 2+2.");
        assert_eq!(got.visible, "The answer is 4.");
    }

    #[test]
    fn extract_well_formed_with_visible_prefix() {
        let response = "Sure! <thinking>reasoning</thinking> done.";
        let got = extract_thinking(response).expect("single block with prefix should parse");
        assert_eq!(got.thinking, "reasoning");
        assert_eq!(got.visible, "Sure!  done.");
    }

    #[test]
    fn extract_no_block() {
        let response = "Just a plain reply without any thinking.";
        let got = extract_thinking(response).expect("no block should passthrough");
        assert_eq!(got.thinking, "");
        assert_eq!(got.visible, response);
    }

    #[test]
    fn extract_multiple_blocks_errors() {
        let response = "<thinking>first</thinking> middle <thinking>second</thinking> trailing";
        let err = extract_thinking(response).expect_err("two blocks must error");
        assert_eq!(err, ThinkingError::MultipleBlocks);
    }

    #[test]
    fn extract_unclosed_errors() {
        let response = "<thinking>no close";
        let err = extract_thinking(response).expect_err("unclosed block must error");
        assert_eq!(err, ThinkingError::UnclosedBlock);
    }

    #[test]
    fn extract_empty_block_strips_tags() {
        // `<thinking></thinking>` with no inner content is degenerate but
        // well-formed; we strip the empty tags so callers see clean visible
        // text and an empty thinking field.
        let response = "<thinking></thinking>visible";
        let got = extract_thinking(response).expect("empty block should not error");
        assert_eq!(got.thinking, "");
        assert_eq!(got.visible, "visible");
    }

    #[test]
    fn extract_close_before_open_errors() {
        // Out-of-order tags — treat as malformed so the caller doesn't silently
        // get empty thinking.
        let response = "</thinking>orphan close<thinking>";
        let err = extract_thinking(response).expect_err("misordered tags must error");
        assert_eq!(err, ThinkingError::UnclosedBlock);
    }
}
