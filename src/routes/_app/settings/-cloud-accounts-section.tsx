import { useRef, useState, type FormEvent } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { Cloud } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import {
  commands,
  type AccountStatus,
  type ConnectSession,
  type CloudAuthError,
} from "@/lib/bindings";
import { useCloudAccounts } from "@/hooks/use-cloud-accounts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { getErrorMessage } from "@/lib/errors";
import {
  SettingsCard,
  SettingsRow,
  SettingsSection,
} from "./-settings-primitives";

export function CloudAccountsSection() {
  const { t } = useLingui();
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<ConnectSession | null>(null);
  const pendingRef = useRef<string | null>(null);
  const errorMessage = (error: CloudAuthError) => {
    switch (error) {
      case "timeout":
        return t`授权等待超时，请重新连接。`;
      case "denied":
        return t`授权未完成，请重新连接。`;
      case "saveFailed":
        return t`凭证保存失败，请检查本机数据目录。`;
      case "reconnectRequired":
        return t`授权已失效，请重新连接。`;
      case "cancelled":
        return t`已取消授权。`;
      default:
        return t`云账户操作失败，请稍后重试。`;
    }
  };
  const { accounts, loading, error, refresh } = useCloudAccounts((update) => {
    if (update.sessionId && update.sessionId === pendingRef.current) {
      pendingRef.current = null;
      setPending(null);
      if (update.error) toast.error(errorMessage(update.error));
      else toast.success(t`云账户已连接`);
    }
  });
  const statusLabel = (status: AccountStatus) => {
    switch (status) {
      case "connected":
        return t`已连接`;
      case "refreshing":
        return t`正在刷新授权`;
      case "reconnectRequired":
        return t`需要重新连接`;
      case "saveFailed":
        return t`凭证保存失败`;
    }
  };
  async function authorize(start: () => Promise<ConnectSession>) {
    setBusy(true);
    try {
      const session = await start();
      pendingRef.current = session.id;
      setPending(session);
      setClientSecret("");
      try {
        await openUrl(session.authorizationUrl);
      } catch (error) {
        await commands.cancelCloudAccountConnect(session.id);
        throw error;
      }
    } catch (error) {
      toast.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  function connect(event: FormEvent) {
    event.preventDefault();
    if (!clientId.trim() || !clientSecret.trim()) {
      toast.error(t`请填写 Client ID 和 Client Secret。`);
      return;
    }
    void authorize(() =>
      commands.connectCloudAccount(
        clientId.trim(),
        clientSecret.trim(),
        label.trim(),
      ),
    );
  }
  async function disconnect(id: string) {
    setBusy(true);
    try {
      const result = await commands.disconnectCloudAccount(id);
      if (!result.revoked)
        toast.warning(
          t`本地凭证已清除，云端撤销失败；可前往 Google 账户移除授权。`,
        );
      await refresh();
    } catch (error) {
      toast.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <SettingsSection title={<Trans>云存储账户</Trans>} icon={Cloud}>
      <SettingsCard>
        <div className="space-y-4 p-4">
          <p className="text-xs leading-5 text-muted-foreground">
            <Trans>
              使用你自己的 Google OAuth 桌面客户端连接 Drive。凭证仅保存在本机。
            </Trans>
          </p>
          <form className="grid gap-3 sm:grid-cols-2" onSubmit={connect}>
            <div className="space-y-1.5">
              <Label htmlFor="cloud-client-id">Client ID</Label>
              <Input
                id="cloud-client-id"
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                autoComplete="off"
                required
                disabled={busy || !!pending}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="cloud-client-secret">Client Secret</Label>
              <Input
                id="cloud-client-secret"
                type="password"
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
                autoComplete="off"
                required
                disabled={busy || !!pending}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="cloud-label">
                <Trans>账户名称（可选）</Trans>
              </Label>
              <Input
                id="cloud-label"
                value={label}
                onChange={(e) => setLabel(e.target.value)}
                disabled={busy || !!pending}
              />
            </div>
            <div className="flex items-end">
              <Button type="submit" disabled={busy || !!pending}>
                <Trans>连接 Google Drive</Trans>
              </Button>
            </div>
          </form>
          {pending && (
            <div
              className="flex items-center justify-between gap-3"
              aria-live="polite"
            >
              <span className="text-sm">
                <Trans>请在系统浏览器完成授权。</Trans>
              </span>
              <Button
                variant="outline"
                onClick={() => {
                  void commands
                    .cancelCloudAccountConnect(pending.id)
                    .catch((error) => toast.error(getErrorMessage(error)));
                }}
              >
                <Trans>取消</Trans>
              </Button>
            </div>
          )}
          {loading && (
            <p className="text-xs text-muted-foreground">
              <Trans>正在读取云账户...</Trans>
            </p>
          )}
          {error != null && (
            <div
              role="alert"
              className="flex items-center justify-between gap-3 text-sm text-destructive-ink"
            >
              <Trans>无法读取云账户。</Trans>
              <Button variant="outline" onClick={() => void refresh()}>
                <Trans>重试</Trans>
              </Button>
            </div>
          )}
          {!loading && !error && accounts.length === 0 && (
            <p className="text-xs text-muted-foreground">
              <Trans>尚未连接云账户。</Trans>
            </p>
          )}
        </div>
        {accounts.map((account) => (
          <SettingsRow
            key={account.id}
            title={account.label}
            description={statusLabel(account.status)}
            action={
              <div className="flex gap-2">
                {account.status !== "connected" &&
                  account.status !== "refreshing" && (
                    <Button
                      variant="outline"
                      disabled={busy || !!pending}
                      onClick={() =>
                        void authorize(() =>
                          commands.reconnectCloudAccount(account.id),
                        )
                      }
                    >
                      <Trans>重新连接</Trans>
                    </Button>
                  )}
                <Button
                  variant="outline"
                  disabled={busy}
                  onClick={() => void disconnect(account.id)}
                >
                  <Trans>断开</Trans>
                </Button>
              </div>
            }
          />
        ))}
      </SettingsCard>
    </SettingsSection>
  );
}
