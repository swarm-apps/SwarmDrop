import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

const appPath = process.env.SWARMDROP_IOS_APP_PATH;
const udid = process.env.SWARMDROP_IOS_UDID;
const deviceName = process.env.SWARMDROP_IOS_DEVICE_NAME ?? "iPhone 17 Pro";
const platformVersion = process.env.SWARMDROP_IOS_PLATFORM_VERSION;
const appiumPort = Number(process.env.APPIUM_PORT ?? 4723);
const mjpegServerPort = Number(
  process.env.SWARMDROP_MJPEG_SERVER_PORT ?? 10086,
);
const useAppiumScreenRecording =
  process.env.SWARMDROP_APPIUM_SCREEN_RECORDING === "1";
const noReset = process.env.SWARMDROP_IOS_NO_RESET === "1";

const capabilities: WebdriverIO.Capabilities[] = [
  {
    platformName: "iOS",
    "appium:automationName": "XCUITest",
    ...(udid ? { "appium:udid": udid } : { "appium:deviceName": deviceName }),
    ...(platformVersion ? { "appium:platformVersion": platformVersion } : {}),
    ...(appPath
      ? { "appium:app": resolve(appPath) }
      : { "appium:bundleId": "com.yexiyue.swarmdrop" }),
    "appium:noReset": noReset,
    ...(useAppiumScreenRecording
      ? {
          // 真机 Appium 录屏通过 WDA 的 MJPEG 流读取；固定到项目约定的 10086 端口。
          "appium:mjpegServerPort": mjpegServerPort,
        }
      : {}),
    "appium:newCommandTimeout": 120,
    "appium:autoAcceptAlerts": true,
  },
];

export const config: WebdriverIO.Config = {
  runner: "local",
  tsConfigPath: resolve(__dirname, "tsconfig.json"),
  specs: ["./test/specs/**/*.e2e.ts"],
  exclude: [],
  maxInstances: 1,
  capabilities,
  logLevel: process.env.SWARMDROP_E2E_RECORDING === "1" ? "warn" : "info",
  bail: 0,
  waitforTimeout: 15_000,
  connectionRetryTimeout: 180_000,
  connectionRetryCount: 1,
  port: appiumPort,
  path: "/",
  services: [
    [
      "appium",
      {
        args: {
          port: appiumPort,
          relaxedSecurity: true,
        },
        logPath: "./build/webdriver/appium",
      },
    ],
  ],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 180_000,
  },
};
