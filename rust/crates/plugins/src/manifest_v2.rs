//! Plugin Manifest v2 — extends the v1 plugin manifest with provider registration
//!
//! so any LLM (including non-Anthropic models such as Mavis) can be plugged in as a
//! backend by writing a manifest, with no Rust change required.
//!
//! Backwards-compatible: v1-shaped JSON (no `providers`, `tools`, or `hooks` arrays)
//! still parses — the new fields default to empty `Vec`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::PluginKind;

/// Current schema version. Bump when the manifest shape changes incompatibly.
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDecl {
    /// Stable identifier — e.g. `"mavis"`, `"anthropic"`, `"ollama"`.
    pub id: String,
    /// Human-facing name for UIs.
    pub display_name: String,
    /// Base URL of an OpenAI-compatible or Anthropic-compatible API.
    pub endpoint: String,
    /// Optional env var name holding the API key/token. None = no auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env: Option<String>,
    /// Default model ID for this provider.
    pub default_model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDecl {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input. Stored as raw JSON for forward-compat.
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookDecl {
    pub event: HookEvent,
    /// Optional shell command to run for this hook event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifestV2 {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    /// Free-text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub providers: Vec<ProviderDecl>,
    #[serde(default)]
    pub tools: Vec<ToolDecl>,
    #[serde(default)]
    pub hooks: Vec<HookDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// A required field is missing or empty.
    MissingField(&'static str),
    /// The JSON could not be parsed. The message is captured as `String` because
    /// `serde_json::Error` itself doesn't implement `Clone` / `PartialEq`.
    InvalidJson(String),
    /// The schema_version is not the version this parser understands.
    UnsupportedVersion(u32),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(name) => write!(f, "missing required field: {name}"),
            Self::InvalidJson(e) => write!(f, "invalid JSON: {e}"),
            Self::UnsupportedVersion(v) => {
                write!(
                    f,
                    "unsupported schema version: {v} (this build understands v{SCHEMA_VERSION})"
                )
            }
        }
    }
}

impl std::error::Error for ManifestError {}

impl From<serde_json::Error> for ManifestError {
    fn from(e: serde_json::Error) -> Self {
        Self::InvalidJson(e.to_string())
    }
}

/// Parse a v2 manifest. Backwards-compatible: a v1-shaped JSON (no `providers`,
/// `tools`, `hooks` arrays) still parses with those fields defaulting to empty.
pub fn parse(json_str: &str) -> Result<PluginManifestV2, ManifestError> {
    let manifest: PluginManifestV2 = serde_json::from_str(json_str)?;
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(ManifestError::UnsupportedVersion(manifest.schema_version));
    }
    if manifest.name.is_empty() {
        return Err(ManifestError::MissingField("name"));
    }
    if manifest.version.is_empty() {
        return Err(ManifestError::MissingField("version"));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2_FULL: &str = r#"{
        "schema_version": 2,
        "name": "mavis-bridge",
        "version": "0.1.0",
        "kind": "external",
        "description": "Bridge that registers Mavis (or compatible) as an LLM backend",
        "providers": [
            {
                "id": "mavis",
                "display_name": "Mavis",
                "endpoint": "https://api.example.com/v1",
                "auth_env": "MAVIS_TOKEN",
                "default_model": "MiniMax-M3"
            }
        ],
        "tools": [],
        "hooks": []
    }"#;

    /// A v1-shaped manifest (no providers/tools/hooks arrays at all) — must still parse
    /// because the new fields default to empty `Vec`.
    const V1_SHAPED: &str = r#"{
        "schema_version": 2,
        "name": "legacy-plugin",
        "version": "0.0.1",
        "kind": "bundled"
    }"#;

    #[test]
    fn parse_v2_full_manifest_ok() {
        let m = parse(V2_FULL).expect("v2 full must parse");
        assert_eq!(m.schema_version, 2);
        assert_eq!(m.name, "mavis-bridge");
        assert_eq!(m.kind, PluginKind::External);
        assert_eq!(m.providers.len(), 1);
        assert_eq!(m.providers[0].id, "mavis");
        assert_eq!(m.providers[0].default_model, "MiniMax-M3");
        assert_eq!(m.providers[0].auth_env.as_deref(), Some("MAVIS_TOKEN"));
        assert_eq!(m.tools.len(), 0);
        assert_eq!(m.hooks.len(), 0);
        assert_eq!(
            m.description.as_deref(),
            Some("Bridge that registers Mavis (or compatible) as an LLM backend")
        );
    }

    #[test]
    fn parse_v1_minimal_manifest_ok() {
        // Backwards-compat: no providers/tools/hooks arrays present → defaults to empty.
        let m = parse(V1_SHAPED).expect("v1-shaped manifest must still parse");
        assert_eq!(m.name, "legacy-plugin");
        assert_eq!(m.kind, PluginKind::Bundled);
        assert!(m.providers.is_empty());
        assert!(m.tools.is_empty());
        assert!(m.hooks.is_empty());
        assert!(m.description.is_none());
    }

    #[test]
    fn parse_missing_schema_version_err() {
        let json = r#"{"name":"x","version":"1","kind":"external"}"#;
        // serde will fail to deserialize because schema_version is required.
        let err = parse(json).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidJson(_)));
    }

    #[test]
    fn parse_unsupported_version_err() {
        let json = r#"{"schema_version":3,"name":"x","version":"1","kind":"external"}"#;
        let err = parse(json).unwrap_err();
        assert_eq!(err, ManifestError::UnsupportedVersion(3));
    }

    #[test]
    fn parse_invalid_json_err() {
        let json = "{not valid json}";
        let err = parse(json).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidJson(_)));
    }

    #[test]
    fn parse_empty_name_err() {
        let json = r#"{"schema_version":2,"name":"","version":"1.0","kind":"external"}"#;
        let err = parse(json).unwrap_err();
        assert_eq!(err, ManifestError::MissingField("name"));
    }

    #[test]
    fn parse_round_trip() {
        let m = parse(V2_FULL).expect("parse ok");
        let serialized = serde_json::to_string(&m).expect("serialize ok");
        let m2 = parse(&serialized).expect("re-parse ok");
        assert_eq!(m, m2);
    }

    #[test]
    fn provider_decl_defaults_work() {
        // auth_env is optional — must serialize/deserialize without it.
        let json = r#"{
            "schema_version": 2,
            "name": "p",
            "version": "0.1.0",
            "kind": "external",
            "providers": [
                {"id":"a","display_name":"A","endpoint":"http://x","default_model":"m"}
            ]
        }"#;
        let m = parse(json).expect("parse ok");
        assert!(m.providers[0].auth_env.is_none());
    }

    #[test]
    fn hook_decl_with_and_without_command() {
        let json = r#"{
            "schema_version": 2,
            "name": "p",
            "version": "0.1.0",
            "kind": "builtin",
            "hooks": [
                {"event": "PreToolUse", "command": "echo hi"},
                {"event": "PostToolUse"}
            ]
        }"#;
        let m = parse(json).expect("parse ok");
        assert_eq!(m.hooks.len(), 2);
        assert_eq!(m.hooks[0].event, HookEvent::PreToolUse);
        assert_eq!(m.hooks[0].command.as_deref(), Some("echo hi"));
        assert_eq!(m.hooks[1].event, HookEvent::PostToolUse);
        assert!(m.hooks[1].command.is_none());
    }
}
