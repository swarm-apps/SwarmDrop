const { withGradleProperties } = require("@expo/config-plugins");

// CI 的 `expo prebuild` 会重新生成 android/gradle.properties，把堆重置回模板默认
// 的 -Xmx2048m。打包 arm64 release APK 时 ApkFlinger 用 readAllBytes 把体积可观的
// Rust .so 整份读进堆，2048m 在临界处会 OutOfMemoryError（v0.7.4 首次发版即中招）。
// 这里在 prebuild 阶段把 org.gradle.jvmargs 覆盖成 4g，稳过打包；runner 内存充足。
const JVM_ARGS = "-Xmx4096m -XX:MaxMetaspaceSize=512m";

function setJvmArgs(properties) {
	const next = properties.filter(
		(entry) =>
			!(entry.type === "property" && entry.key === "org.gradle.jvmargs"),
	);
	next.push({
		type: "property",
		key: "org.gradle.jvmargs",
		value: JVM_ARGS,
	});
	return next;
}

const withAndroidGradleMemory = (config) =>
	withGradleProperties(config, (config) => {
		config.modResults = setJvmArgs(config.modResults);
		return config;
	});

module.exports = withAndroidGradleMemory;
module.exports.setJvmArgs = setJvmArgs;
