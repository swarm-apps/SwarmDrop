import { useLingui } from "@lingui/react/macro";
import { View } from "react-native";
import { Surface } from "@/components/mobile/screen";
import { Separator } from "@/components/ui/separator";
import { Text } from "@/components/ui/text";
import { cn, truncateMiddle } from "@/lib/utils";

/** 配对设备信息（found-device / success 两屏共享的 params 形态）。 */
export interface PeerSummary {
  name?: string;
  hostname: string;
  os: string;
  platform: string;
  arch: string;
  peerId: string;
}

/** 优先设备名、回退主机名 —— 两屏标题/卡片统一取这个显示名。 */
export function peerDisplayName(
  name: string | undefined,
  hostname: string,
): string {
  return name?.trim() || hostname;
}

interface PeerSummaryCardProps extends PeerSummary {
  /** found-device 需要展示「平台」行，success 不需要。 */
  showPlatform?: boolean;
}

/**
 * 配对设备摘要卡 —— found-device（确认）与 success（成功）两屏复用同一张
 * label/value 卡，仅「平台」行按 showPlatform 取舍。
 */
export function PeerSummaryCard({
  name,
  hostname,
  os,
  platform,
  arch,
  peerId,
  showPlatform,
}: PeerSummaryCardProps) {
  const { t } = useLingui();
  return (
    <Surface className="w-full p-4">
      <SpecRow label={t`设备名`} value={peerDisplayName(name, hostname)} />
      {name && name !== hostname ? (
        <>
          <Separator />
          <SpecRow label={t`主机名`} value={hostname} />
        </>
      ) : null}
      <Separator />
      <SpecRow label={t`系统`} value={`${os} · ${arch}`} />
      {showPlatform ? (
        <>
          <Separator />
          <SpecRow label={t`平台`} value={platform} />
        </>
      ) : null}
      <Separator />
      <SpecRow label={t`设备 ID`} value={truncateMiddle(peerId, 8, 6)} mono />
    </Surface>
  );
}

function SpecRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <View className="flex-row items-center justify-between py-2.5">
      <Text className="text-[13px] text-muted-foreground">{label}</Text>
      <Text
        className={cn(
          "max-w-[60%] text-[13px] font-semibold text-foreground",
          mono && "font-mono",
        )}
        numberOfLines={1}
      >
        {value}
      </Text>
    </View>
  );
}
