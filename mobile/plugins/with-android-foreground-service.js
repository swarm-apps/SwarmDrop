const { withAndroidManifest } = require("@expo/config-plugins");

// notify-kit 自带的 ForegroundService 声明未带 foregroundServiceType,
// 且未声明 CONNECTED_DEVICE 权限。Android 14+(targetSdk 34+)必须显式声明,
// 否则启动前台服务会抛 MissingForegroundServiceTypeException。
// 用 connectedDevice 规避 Android 15 对 dataSync 的 ~6h/24h 时长上限(见 design D8)。
const CONNECTED_DEVICE_PERMISSION =
	"android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE";
const NOTIFEE_FGS = "app.notifee.core.ForegroundService";
const FGS_TYPE = "connectedDevice";

function injectPermission(manifest) {
	const root = manifest.manifest;
	root["uses-permission"] = root["uses-permission"] ?? [];

	const already = root["uses-permission"].some(
		(p) => p.$ && p.$["android:name"] === CONNECTED_DEVICE_PERMISSION,
	);
	if (!already) {
		root["uses-permission"].push({
			$: { "android:name": CONNECTED_DEVICE_PERMISSION },
		});
	}
	return manifest;
}

function setForegroundServiceType(manifest) {
	const root = manifest.manifest;
	// tools:replace 需要 tools 命名空间
	root.$ = root.$ ?? {};
	if (!root.$["xmlns:tools"]) {
		root.$["xmlns:tools"] = "http://schemas.android.com/tools";
	}

	const application = root.application?.[0];
	if (!application) return manifest;
	application.service = application.service ?? [];

	let service = application.service.find(
		(s) => s.$ && s.$["android:name"] === NOTIFEE_FGS,
	);
	if (!service) {
		service = {
			$: { "android:name": NOTIFEE_FGS, "android:exported": "false" },
		};
		application.service.push(service);
	}
	// 库声明未带 type,这里补上并用 tools:replace 兜底 merge 冲突。
	service.$["android:foregroundServiceType"] = FGS_TYPE;
	service.$["tools:replace"] = "android:foregroundServiceType";
	return manifest;
}

const withAndroidForegroundService = (config) =>
	withAndroidManifest(config, (config) => {
		config.modResults = injectPermission(config.modResults);
		config.modResults = setForegroundServiceType(config.modResults);
		return config;
	});

module.exports = withAndroidForegroundService;
module.exports.injectPermission = injectPermission;
module.exports.setForegroundServiceType = setForegroundServiceType;
