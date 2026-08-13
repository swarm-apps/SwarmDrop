/**
 * `@swarmdrop/shared-view` 的唯一公开面。
 *
 * 三端（桌面 `src/` · Web `docs/app/app` · 移动 `mobile/src`）都从这里导入，
 * 不深链子路径——子模块的划分是本包的内部事，改动它不该惊动三个消费者。
 *
 * 什么该进这个包、什么不该，见 README 的「归属判据」。
 */

export * from "./device";
export * from "./file-browser";
export * from "./format";
export * from "./network";
export * from "./transfer";
