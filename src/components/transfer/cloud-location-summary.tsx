import { Trans } from "@lingui/react/macro";
import type { CloudObjectRef } from "@/lib/bindings";
import { useCloudAccounts } from "@/hooks/use-cloud-accounts";

export function CloudLocationSummary({
  objects,
}: {
  objects: CloudObjectRef[];
}) {
  const { accounts, loading } = useCloudAccounts();
  return (
    <div className="space-y-2 rounded-xl border border-border bg-muted/20 p-3 text-sm">
      {objects.map((object) => {
        const account = accounts.find(
          (account) => account.id === object.accountId,
        );
        return (
          <div key={object.objectId}>
            <p className="font-medium">
              Google Drive · {account?.label ?? object.accountId}
            </p>
            <p className="break-all text-xs text-muted-foreground">
              {object.displayPath}
            </p>
            {!loading && account?.status !== "connected" && (
              <p className="text-xs text-muted-foreground">
                <Trans>
                  云账户未连接，历史记录仍保留；打开文件前请重新连接。
                </Trans>
              </p>
            )}
          </div>
        );
      })}
      <p className="text-xs leading-5 text-muted-foreground">
        <Trans>
          点击文件可在浏览器中打开云盘。传输段端到端加密，云盘保存明文文件，访问由云盘权限控制。
        </Trans>
      </p>
    </div>
  );
}
