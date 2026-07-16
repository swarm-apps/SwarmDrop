const { withDangerousMod } = require("@expo/config-plugins");
const fs = require("node:fs");
const path = require("node:path");

// notify-kit(notifee)的通知小图标 = Android 状态栏 24dp 单色剪影。系统只用 alpha
// 通道渲染(纯白+透明),彩色 launcher 图标当小图标会被渲染成深色块。资源名
// ic_notification 对应代码里 notifee `android.smallIcon` 的引用。
//
// android/ 是 gitignore 的 prebuild 产物(CI 每次 expo prebuild 重生成),故这里用
// config plugin 在 prebuild 时把 assets/android-notification-icon/drawable-*/
// ic_notification.png 复制进各 density 的 res/drawable-*,而不是手改原生目录。
const DENSITIES = ["mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi"];
const ICON = "ic_notification.png";

const withAndroidNotificationIcon = (config) =>
	withDangerousMod(config, [
		"android",
		(config) => {
			const { projectRoot, platformProjectRoot } = config.modRequest;
			const srcRoot = path.join(
				projectRoot,
				"assets/android-notification-icon",
			);
			const resRoot = path.join(platformProjectRoot, "app/src/main/res");
			for (const density of DENSITIES) {
				const src = path.join(srcRoot, `drawable-${density}`, ICON);
				if (!fs.existsSync(src)) {
					throw new Error(
						`[with-android-notification-icon] 缺少资源: ${src}`,
					);
				}
				const dstDir = path.join(resRoot, `drawable-${density}`);
				fs.mkdirSync(dstDir, { recursive: true });
				fs.copyFileSync(src, path.join(dstDir, ICON));
			}
			return config;
		},
	]);

module.exports = withAndroidNotificationIcon;
