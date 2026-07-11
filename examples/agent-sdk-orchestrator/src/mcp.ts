import type { McpServerConfig } from "@anthropic-ai/claude-agent-sdk";

/**
 * Dynamic MCP servers — spawn isolated tsx/uvx/npx processes per session.
 */
export function buildDefaultMcpServers(workspace: string): Record<string, McpServerConfig> {
  return {
    playwright: {
      command: "npx",
      args: ["-y", "@playwright/mcp@latest"],
    },
    filesystem: {
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", workspace],
    },
    fetch: {
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-fetch"],
    },
    context7: {
      command: "npx",
      args: ["-y", "@upstash/context7-mcp"],
    },
    sequential_thinking: {
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-sequential-thinking"],
    },
    notes: {
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-memory"],
    },
  };
}

/** Launch a one-off MCP via tsx/uvx for ecosystem parity harnesses. */
export function dynamicMcpLaunch(
  runtime: "tsx" | "uvx" | "npx",
  packageName: string,
  extraArgs: string[] = [],
): McpServerConfig {
  const command = runtime === "uvx" ? "uvx" : runtime === "tsx" ? "tsx" : "npx";
  const args =
    runtime === "npx"
      ? ["-y", packageName, ...extraArgs]
      : [packageName, ...extraArgs];
  return { command, args };
}
