import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  BottomSheetModal,
  BottomSheetView,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import {
  FileText,
  Image as ImageIcon,
  type LucideIcon,
  Video,
} from "lucide-react-native";
import { forwardRef, useCallback, useImperativeHandle, useRef } from "react";
import { Pressable, View } from "react-native";
import { Text } from "@/components/ui/text";
import {
  type MediaKind,
  pickFromMediaLibrary,
  pickTransferFiles,
} from "@/core/file-access";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { errorMessage } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";

export interface SendOptionsSheetRef {
  present: () => void;
  dismiss: () => void;
}

interface OptionDef {
  key: "files" | "photos" | "videos";
  icon: LucideIcon;
  label: React.ReactNode;
  description: React.ReactNode;
}

/**
 * 发送来源选择 BottomSheet ——「文件 / 照片 / 视频」三个入口。
 * 选完后会把 files 写入 mobile-core-store.selectedFiles 并 push /send/select-device。
 */
export const SendOptionsSheet = forwardRef<SendOptionsSheetRef, object>(
  function SendOptionsSheet(_props, ref) {
    const sheetRef = useRef<BottomSheetModal>(null);
    const colors = useThemeColors();
    const router = useRouter();
    const setSelectedFiles = useMobileCoreStore((s) => s.setSelectedFiles);
    const { t } = useLingui();

    useImperativeHandle(ref, () => ({
      present: () => sheetRef.current?.present(),
      dismiss: () => sheetRef.current?.dismiss(),
    }));

    const renderBackdrop = useCallback(
      (props: BottomSheetBackdropProps) => (
        <BottomSheetBackdrop
          {...props}
          opacity={0.4}
          appearsOnIndex={0}
          disappearsOnIndex={-1}
          pressBehavior="close"
        />
      ),
      [],
    );

    const handlePick = async (key: OptionDef["key"]) => {
      try {
        const files =
          key === "files"
            ? await pickTransferFiles()
            : await pickFromMediaLibrary(key as MediaKind);
        if (files.length === 0) return;
        setSelectedFiles(files);
        sheetRef.current?.dismiss();
        router.push("/send/select-device" as never);
      } catch (err) {
        toast.error(t`选择失败`, errorMessage(err));
      }
    };

    const options: OptionDef[] = [
      {
        key: "files",
        icon: FileText,
        label: <Trans>选择文件</Trans>,
        description: <Trans>任意类型,支持多选</Trans>,
      },
      {
        key: "photos",
        icon: ImageIcon,
        label: <Trans>照片</Trans>,
        description: <Trans>从相册选择图片</Trans>,
      },
      {
        key: "videos",
        icon: Video,
        label: <Trans>视频</Trans>,
        description: <Trans>从相册选择视频</Trans>,
      },
    ];

    return (
      <BottomSheetModal
        ref={sheetRef}
        enableDynamicSizing
        enablePanDownToClose
        backdropComponent={renderBackdrop}
        backgroundStyle={{ backgroundColor: colors.card }}
        handleIndicatorStyle={{ backgroundColor: colors.border }}
      >
        <BottomSheetView>
          <View className="px-5 pt-2 pb-6 gap-1">
            <Text className="text-base font-bold text-foreground px-1.5 pb-2">
              <Trans>发送什么?</Trans>
            </Text>
            {options.map((opt) => {
              const Icon = opt.icon;
              return (
                <Pressable
                  key={opt.key}
                  onPress={() => handlePick(opt.key)}
                  accessibilityRole="button"
                  className="flex-row items-center gap-3 rounded-xl px-3 py-3 active:bg-muted"
                >
                  <View className="size-10 items-center justify-center rounded-xl bg-primary/10">
                    <Icon color={colors.primary} size={20} />
                  </View>
                  <View className="flex-1 gap-0.5">
                    <Text className="text-[14px] font-medium text-foreground">
                      {opt.label}
                    </Text>
                    <Text className="text-[11px] text-muted-foreground">
                      {opt.description}
                    </Text>
                  </View>
                </Pressable>
              );
            })}
          </View>
        </BottomSheetView>
      </BottomSheetModal>
    );
  },
);
