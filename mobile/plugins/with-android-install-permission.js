const { withAndroidManifest } = require("@expo/config-plugins");

const PERMISSION = "android.permission.REQUEST_INSTALL_PACKAGES";

function injectPermission(manifest) {
	const root = manifest.manifest;
	root["uses-permission"] = root["uses-permission"] ?? [];

	const already = root["uses-permission"].some(
		(p) => p.$ && p.$["android:name"] === PERMISSION,
	);
	if (already) return manifest;

	root["uses-permission"].push({ $: { "android:name": PERMISSION } });
	return manifest;
}

const withAndroidInstallPermission = (config) =>
	withAndroidManifest(config, (config) => {
		config.modResults = injectPermission(config.modResults);
		return config;
	});

module.exports = withAndroidInstallPermission;
module.exports.injectPermission = injectPermission;
