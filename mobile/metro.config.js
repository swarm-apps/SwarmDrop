const path = require("node:path");
const { getDefaultConfig } = require("expo/metro-config");
const { withNativewind } = require("nativewind/metro");

const config = getDefaultConfig(__dirname);

// Lingui catalogs (.po/.pot) need to be importable as JS modules.
config.resolver.sourceExts.push("po", "pot");
config.transformer.babelTransformerPath = require.resolve("@lingui/metro-transformer/expo");

// `@swarmdrop/shared-view` 住在仓库根的 packages/ 下，以 `link:` 符号链接进来，并且发布的是
// **TS 源**（openspec: web-ux-alignment 的 design D2）。Metro 默认只监视 projectRoot 之内的
// 文件，符号链接指到外面的目录必须显式登记，否则 bundle 时报 module 解析失败。
//
// 只加 `packages/`，不加整个仓库根——根下有 10G 量级的 `target/`（Rust 产物），
// 让 Metro 去监视它是纯粹的浪费。
config.watchFolders = [...(config.watchFolders ?? []), path.resolve(__dirname, "../packages")];

module.exports = withNativewind(config, { input: "./src/global.css" });
