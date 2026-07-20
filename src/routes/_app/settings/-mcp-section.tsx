/**
 * McpSection
 * 设置页「MCP Server」区域 — MCP 服务端口配置与启停控制
 */

import { useState, useCallback, useEffect } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { toast } from "sonner";
import { Bot, Copy, Check } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { usePreferencesStore } from "@/stores/preferences-store";
import { commands, type McpStatus } from "@/lib/bindings";
import { copyText } from "@/lib/clipboard";
import { getErrorMessage } from "@/lib/errors";
import {
  SettingsCard,
  SettingsRow,
  SettingsSection,
} from "./-settings-primitives";

export function McpSection() {
  const { t } = useLingui();
  const mcpPort = usePreferencesStore((s) => s.mcp.port);
  const setMcpPort = usePreferencesStore((s) => s.setMcpPort);
  const mcpAutoStart = usePreferencesStore((s) => s.mcp.autoStart);
  const setMcpAutoStart = usePreferencesStore((s) => s.setMcpAutoStart);

  const [status, setStatus] = useState<McpStatus>({
    running: false,
    addr: null,
  });
  const [loading, setLoading] = useState(false);
  const [portInput, setPortInput] = useState(String(mcpPort));
  const [copied, setCopied] = useState(false);

  // 挂载时查询后端真实状态
  useEffect(() => {
    commands.getMcpStatus().then(setStatus).catch(() => {});
  }, []);

  // mcpPort 变更时同步 portInput（hydration 后）
  useEffect(() => {
    setPortInput(String(mcpPort));
  }, [mcpPort]);

  const handleToggle = useCallback(async () => {
    setLoading(true);
    try {
      if (status.running) {
        const result = await commands.stopMcpServer();
        setStatus(result);
        toast.success(t`MCP Server 已停止`);
      } else {
        const port = Number(portInput);
        if (isNaN(port) || port < 1024 || port > 65535) {
          toast.error(t`端口号需在 1024 ~ 65535 之间`);
          setLoading(false);
          return;
        }
        setMcpPort(port);
        const result = await commands.startMcpServer(port);
        setStatus(result);
        toast.success(t`MCP Server 已启动`);
      }
    } catch (e) {
      toast.error(getErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [status.running, portInput, setMcpPort, t]);

  const mcpUrl = status.addr
    ? `http://${status.addr}`
    : `http://127.0.0.1:${portInput}`;

  const mcpConfig = JSON.stringify(
    {
      mcpServers: {
        swarmdrop: {
          url: mcpUrl,
        },
      },
    },
    null,
    2,
  );

  const handleCopy = useCallback(() => {
    copyText(mcpConfig).then(() => {
      setCopied(true);
      toast.success(t`已复制到剪贴板`);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [mcpConfig, t]);

  return (
    <SettingsSection title="MCP Server" icon={Bot} fill>
      <SettingsCard fill>
        <SettingsRow
          title={
            <span className="flex items-center gap-2">
              <span className="text-sm font-medium text-foreground">
                <Trans>服务状态</Trans>
              </span>
              <Badge variant={status.running ? "default" : "secondary"}>
                {status.running ? t`运行中` : t`已停止`}
              </Badge>
            </span>
          }
          description={
            status.running && status.addr ? (
              <Trans>监听地址: {status.addr}</Trans>
            ) : (
              <Trans>启动后可供 AI 助手通过 MCP 协议操控文件传输</Trans>
            )
          }
          action={
            <Button
              size="sm"
              variant={status.running ? "destructive" : "default"}
              onClick={handleToggle}
              disabled={loading}
            >
              {loading ? (
                <Trans>处理中...</Trans>
              ) : status.running ? (
                <Trans>停止</Trans>
              ) : (
                <Trans>启动</Trans>
              )}
            </Button>
          }
        />

        <SettingsRow
          title={<Trans>监听端口</Trans>}
          description={<Trans>MCP Server 的 HTTP 端口号</Trans>}
          action={
            <Input
              type="number"
              min={1024}
              max={65535}
              className="w-28 text-center"
              value={portInput}
              onChange={(e) => setPortInput(e.target.value)}
              disabled={status.running}
            />
          }
        />

        <SettingsRow
          title={<Trans>随节点自动启动</Trans>}
          description={<Trans>P2P 节点启动时自动启动 MCP Server</Trans>}
          action={
            <Switch checked={mcpAutoStart} onCheckedChange={setMcpAutoStart} />
          }
        />

        {/* 客户端配置 */}
        <div className="flex flex-col gap-2 p-4">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium text-foreground">
              <Trans>客户端配置</Trans>
            </span>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              onClick={handleCopy}
            >
              {copied ? (
                <Check className="size-3.5" />
              ) : (
                <Copy className="size-3.5" />
              )}
              {copied ? <Trans>已复制</Trans> : <Trans>复制</Trans>}
            </Button>
          </div>
          <span className="text-xs text-muted-foreground">
            <Trans>
              将以下 JSON 添加到 AI 客户端（如 Claude Desktop、Cursor）的 MCP
              配置中
            </Trans>
          </span>
          <pre className="overflow-x-auto rounded-xl bg-muted p-3 font-mono text-xs leading-relaxed text-muted-foreground">
            {mcpConfig}
          </pre>
        </div>
      </SettingsCard>
    </SettingsSection>
  );
}
