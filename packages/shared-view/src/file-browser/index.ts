/**
 * 文件浏览器的**纯逻辑层（L1）** —— 三端共用。
 *
 * L2 是 `@swarmdrop/file-browser`（React DOM 组件，桌面 + Web），L3 是各端的接线
 * （取图源、业务动作、视图偏好的持久化后端）。归属判据见那个包的 README。
 */

export * from "./types";
export * from "./identity";
export * from "./media-type";
export * from "./tree-data";
export * from "./adapters";
export * from "./thumbnail";
export * from "./view-preference";
