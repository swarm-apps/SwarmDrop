/**
 * MCP Server commands
 * MCP 服务启停控制
 */

import { invoke } from "@tauri-apps/api/core";
import type { McpStatus } from "@/lib/bindings";

export type { McpStatus };

/** 查询 MCP Server 当前状态 */
export function getMcpStatus(): Promise<McpStatus> {
  return invoke("get_mcp_status");
}

/** 启动 MCP Server */
export function startMcpServer(port?: number): Promise<McpStatus> {
  return invoke("start_mcp_server", { port });
}

/** 停止 MCP Server */
export function stopMcpServer(): Promise<McpStatus> {
  return invoke("stop_mcp_server");
}
