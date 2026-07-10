import type { McpServerConfig } from "@anthropic-ai/claude-agent-sdk";

/**
 * Dynamic MCP servers — spawn isolated tsx/uvx/npx processes per need.
 * Merge with user's .claude/settings.json mcpServers when orchestrating.
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
  };
}
