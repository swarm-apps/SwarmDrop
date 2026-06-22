import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** 运行平台检测(同步,基于 navigator.platform) */
export const isMac =
  typeof navigator !== "undefined" && navigator.platform.includes("Mac");
