// Node 18+ 严格执行 package.json 的 "exports"。uniffi-bindgen-react-native 生成的
// Android CMakeLists 用 `node -p "require.resolve('uniffi-bindgen-react-native/package.json')"`
// 定位其 C++ 运行时头目录（cpp/includes），但 ubrn 的 exports 只导出 "."、没导出
// "./package.json" → Node 24 抛 ERR_PACKAGE_PATH_NOT_EXPORTED → require.resolve 输出为空 →
// CMake 得到 `-I/cpp/includes`（根路径）→ 编译报 `UniffiCallInvoker.h file not found`。
//
// 安装期给 ubrn 的 exports 补上 ./package.json 自导出，让 require.resolve 恢复工作。
// 用 readPackage 而非 pnpm patch：不绑版本号（ubrn 升级不会失配报 unused-patch）。
function readPackage(pkg) {
  if (
    pkg.name === "uniffi-bindgen-react-native" &&
    pkg.exports &&
    typeof pkg.exports === "object" &&
    !pkg.exports["./package.json"]
  ) {
    pkg.exports["./package.json"] = "./package.json";
  }

  // ubrn ≥0.31.0-3 生成的绑定 import @ubjs/core，但上游没把它声明为 ubrn 的
  // dependency——生成器与 runtime 的版本联动只能靠消费方自律。失配时不会立刻
  // 报错（行为悄悄漂移），这里在安装期把哑错变响错：两者必须完全同版本。
  if (pkg.name === "react-native-swarmdrop-core") {
    const generator = pkg.devDependencies?.["uniffi-bindgen-react-native"];
    const runtime = pkg.dependencies?.["@ubjs/core"];
    if (generator && runtime && generator !== runtime) {
      throw new Error(
        `[pnpmfile] react-native-swarmdrop-core: uniffi-bindgen-react-native(${generator}) ` +
          `与 @ubjs/core(${runtime}) 版本失配——生成器与 runtime 必须完全同版本`,
      );
    }
  }
  return pkg;
}

module.exports = { hooks: { readPackage } };
