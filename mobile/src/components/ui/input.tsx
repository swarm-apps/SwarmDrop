import { Platform, TextInput } from "react-native";
import { cn } from "@/lib/utils";

function Input({
  className,
  style,
  ...props
}: React.ComponentProps<typeof TextInput>) {
  return (
    <TextInput
      className={cn(
        "dark:bg-input/30 border-input bg-background text-foreground flex w-full min-w-0 flex-row items-center rounded-md border px-3 py-2.5 text-base leading-5 shadow-sm shadow-black/5",
        props.editable === false &&
          cn(
            "opacity-50",
            Platform.select({
              web: "disabled:pointer-events-none disabled:cursor-not-allowed",
            }),
          ),
        Platform.select({
          web: cn(
            "placeholder:text-muted-foreground selection:bg-primary selection:text-primary-foreground outline-none transition-[color,box-shadow] md:text-sm",
            "focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
            "aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
          ),
          native: "placeholder:text-muted-foreground/50",
        }),
        className,
      )}
      // 不设固定 height,用 paddingVertical(py-2.5≈40px 触控)让高度随 font line box
      // 自适应 —— RN 官方推荐:固定小高 + textAlignVertical 会让 CJK 文字被裁/可滚。
      // Android 再去掉字体额外 padding。
      style={[
        Platform.OS === "android" && { includeFontPadding: false },
        style,
      ]}
      {...props}
    />
  );
}

export { Input };
