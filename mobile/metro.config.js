const { getDefaultConfig } = require("expo/metro-config");
const { withNativewind } = require("nativewind/metro");

const config = getDefaultConfig(__dirname);

// Lingui catalogs (.po/.pot) need to be importable as JS modules.
config.resolver.sourceExts.push("po", "pot");
config.transformer.babelTransformerPath = require.resolve("@lingui/metro-transformer/expo");

module.exports = withNativewind(config, { input: "./src/global.css" });
