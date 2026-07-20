/**
 * 邀请二维码（移动端）
 *
 * 模块矩阵由 core `inviteQrMatrix` 生成（三端统一编码规范：大写 alphanumeric +
 * ECL::M + quiet zone，见 crates/invite/src/qr.rs），本组件用 react-native-svg 按矩阵
 * 绘 `<Rect>`。深模块 + 白底、不随暗色主题反色（摄像头对反色 QR 识别差）。
 */

import { useMemo } from "react";
import { ActivityIndicator, View } from "react-native";
import Svg, { Rect } from "react-native-svg";
import { getMobileCore } from "@/core/mobile-core";

interface InviteQrProps {
  /** 邀请串（小写规范形态）；null 时展示占位 */
  invite: string | null;
  /** 边长（px），默认 220 */
  size?: number;
}

/** 深模块坐标（预计算，避免渲染时用数组下标作 key） */
interface QrCells {
  dim: number;
  cells: Array<{ x: number; y: number }>;
}

export function InviteQr({ invite, size = 220 }: InviteQrProps) {
  // inviteQrMatrix 是同步 uniffi 方法（Rust pub fn）→ useMemo 直接算，无首帧 spinner 闪烁
  const matrix = useMemo<QrCells | null>(() => {
    if (invite === null) return null;
    try {
      const m = getMobileCore().inviteQrMatrix(invite);
      const dim = Number(m.size);
      const cells: Array<{ x: number; y: number }> = [];
      m.modules.forEach((on, i) => {
        if (on) cells.push({ x: i % dim, y: Math.floor(i / dim) });
      });
      return { dim, cells };
    } catch {
      return null;
    }
  }, [invite]);

  // 白卡容器（padding 12），二维码本体铺满
  const pad = 12;
  const inner = size - pad * 2;

  return (
    <View
      style={{
        width: size,
        height: size,
        padding: pad,
        borderRadius: 20,
        backgroundColor: "#ffffff",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {matrix ? (
        <Svg
          width={inner}
          height={inner}
          viewBox={`0 0 ${matrix.dim} ${matrix.dim}`}
        >
          {/* 白底已由容器提供，只画深模块 */}
          {matrix.cells.map(({ x, y }) => (
            <Rect
              key={`${x}-${y}`}
              x={x}
              y={y}
              width={1}
              height={1}
              fill="#0a0a0a"
            />
          ))}
        </Svg>
      ) : (
        <ActivityIndicator color="#a3a3a3" />
      )}
    </View>
  );
}
