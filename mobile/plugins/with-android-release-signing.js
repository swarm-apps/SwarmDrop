const { withAppBuildGradle } = require("@expo/config-plugins");

const SIGNING_CONFIGS_REGEX =
	/(signingConfigs\s*\{\s*\n[\s\S]*?debug\s*\{[\s\S]*?\}\s*\n)(\s*\})/;
const RELEASE_BUILD_TYPE_REGEX =
	/(release\s*\{[\s\S]*?signingConfig\s+)signingConfigs\.debug/;

const RELEASE_SIGNING_BLOCK = `
        release {
            storeFile file('release.keystore')
            storePassword System.getenv('ANDROID_KEYSTORE_PASSWORD')
            keyAlias System.getenv('ANDROID_KEY_ALIAS')
            keyPassword System.getenv('ANDROID_KEYSTORE_PASSWORD')
        }`;

function injectReleaseSigning(contents) {
	if (contents.includes("release.keystore")) return contents;

	if (!RELEASE_BUILD_TYPE_REGEX.test(contents)) {
		throw new Error(
			"with-android-release-signing: could not locate buildTypes.release.signingConfig in app/build.gradle",
		);
	}
	if (!SIGNING_CONFIGS_REGEX.test(contents)) {
		throw new Error(
			"with-android-release-signing: could not locate signingConfigs block in app/build.gradle",
		);
	}

	return contents
		.replace(RELEASE_BUILD_TYPE_REGEX, "$1signingConfigs.release")
		.replace(SIGNING_CONFIGS_REGEX, `$1${RELEASE_SIGNING_BLOCK}\n$2`);
}

const withAndroidReleaseSigning = (config) =>
	withAppBuildGradle(config, (config) => {
		config.modResults.contents = injectReleaseSigning(
			config.modResults.contents,
		);
		return config;
	});

module.exports = withAndroidReleaseSigning;
module.exports.injectReleaseSigning = injectReleaseSigning;
