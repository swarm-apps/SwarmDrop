const { withAndroidManifest } = require("@expo/config-plugins");

// mDNS 局域网发现要求持有 MulticastLock(见 modules/lan-multicast),而
// WifiManager.createMulticastLock() 需要这条权限。它是 normal 权限,manifest
// 声明即授予,无运行时弹窗。
//
// 没有它的后果是静默的:createMulticastLock 抛 SecurityException 之前先在
// 日志里报一句,应用照常跑,只是同一个 Wi-Fi 下的设备永远发现不了彼此,全部
// 绕公网 relay 传输。这类"能用但慢"的退化最难在测试里被发现,所以权限与
// 原生模块必须一起进,别只加一半。
const MULTICAST_PERMISSION = "android.permission.CHANGE_WIFI_MULTICAST_STATE";

const withAndroidMulticast = (config) =>
	withAndroidManifest(config, (config) => {
		const root = config.modResults.manifest;
		root["uses-permission"] = root["uses-permission"] ?? [];

		const already = root["uses-permission"].some(
			(p) => p.$ && p.$["android:name"] === MULTICAST_PERMISSION,
		);
		if (!already) {
			root["uses-permission"].push({
				$: { "android:name": MULTICAST_PERMISSION },
			});
		}
		return config;
	});

module.exports = withAndroidMulticast;
