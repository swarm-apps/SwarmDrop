#!/usr/bin/env bash
# `crates/web` 的 wasm 测试**执行**门禁（check-wasm.sh 只保证编得过）。
# 用法：scripts/test-wasm.sh [额外的 wasm-pack test 参数]
#
# ## 为什么单独一个脚本
#
# `crates/web` 整个 crate 是 `#[cfg(wasm_browser)]`，进不了 `cargo test --workspace`；
# 而 `check-wasm.sh` 的 `--all-targets` 只把 `#[wasm_bindgen_test]` 模块**编**进来。
# 于是这批测试长期处于「写了、编得过、从没跑过」的状态——它们提供的是虚假的安全感：
# `invite_store.rs` 的写读不对称（写 `to_string` + `put_string`，读却按对象解析，
# 每一行都被静默丢弃）能活到 2026-08 被手动验证撞见，正是因为往返测试从没执行过。
#
# ## chromedriver 的主版本必须与 Chrome 一致
#
# 不一致时 wasm-bindgen 的 test runner 会以一句 `Error: http status: 404` 失败——
# 那是 driver 拒绝了 runner 的 W3C 端点，与「测试挂了」看起来毫无区别，极易误判成代码问题。
# 所以这里**自己解析 Chrome 版本并取匹配的 driver**，不信任 PATH 里碰巧存在的那个
# （Homebrew 的 chromedriver 跟着自己的节奏升级，与本机 Chrome 常年错位）。
#
# wasm-pack 从 **PATH** 取 chromedriver，`CHROMEDRIVER` 环境变量会被它覆盖掉，
# 所以下面是把匹配的目录前置进 PATH，而不是导出那个变量。
set -euo pipefail
cd "$(dirname "$0")/.."

# driver 缓存进 target/：已被 .gitignore 覆盖，且与 toolchain 产物同生命周期。
DRIVER_ROOT="target/wasm-test-driver"

find_chrome() {
  if [[ "$(uname)" == "Darwin" ]]; then
    local mac_chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    [[ -x "$mac_chrome" ]] && { echo "$mac_chrome"; return; }
  fi
  for candidate in google-chrome-stable google-chrome chromium chromium-browser; do
    command -v "$candidate" 2>/dev/null && return
  done
  return 1
}

CHROME="$(find_chrome)" || {
  echo "error: 找不到 Chrome/Chromium——wasm 测试需要真浏览器（IndexedDB / OPFS 都没有 node 替身）" >&2
  exit 1
}

# "Google Chrome 150.0.7871.187 " → "150.0.7871.187"
CHROME_VERSION="$("$CHROME" --version | grep -oE '[0-9]+(\.[0-9]+){3}')"
CHROME_MAJOR="${CHROME_VERSION%%.*}"

# PATH 里已有匹配主版本的 driver 就直接用，省一次下载。
if command -v chromedriver >/dev/null 2>&1 \
  && [[ "$(chromedriver --version | grep -oE '[0-9]+' | head -1)" == "$CHROME_MAJOR" ]]; then
  echo "使用 PATH 中的 chromedriver（主版本 $CHROME_MAJOR 与 Chrome 匹配）"
else
  # `|| true` 不是保险丝而是必需：缓存目录首次运行时不存在，`find` 会非零退出，
  # 在 `set -euo pipefail` 下整个脚本当场静默结束（赋值语句的退出码 = 命令替换的退出码）。
  find_cached_driver() {
    find "$DRIVER_ROOT" -name chromedriver -type f -path "*-$CHROME_VERSION/*" 2>/dev/null | head -1 || true
  }
  DRIVER_DIR="$(find_cached_driver)"
  if [[ -z "$DRIVER_DIR" ]]; then
    echo "为 Chrome $CHROME_VERSION 取匹配的 chromedriver…"
    npx -y @puppeteer/browsers install "chromedriver@$CHROME_VERSION" --path "$DRIVER_ROOT" >/dev/null
    DRIVER_DIR="$(find_cached_driver)"
  fi
  [[ -n "$DRIVER_DIR" ]] || { echo "error: chromedriver@$CHROME_VERSION 获取失败" >&2; exit 1; }
  # macOS：下载来的二进制带 quarantine 且签名过不了 Gatekeeper，被启动即 SIGKILL。
  # 自签名（`--sign -`）足以放行本地执行。
  if [[ "$(uname)" == "Darwin" ]]; then
    xattr -dr com.apple.quarantine "$DRIVER_DIR" 2>/dev/null || true
    codesign --force --sign - "$DRIVER_DIR" >/dev/null 2>&1 || true
  fi
  # **必须是绝对路径**：wasm-pack 内部 `cd crates/web` 再跑 cargo，相对路径当场失效
  # （表现为 `No such file or directory (os error 2)`，与「driver 没装」无从区分）。
  PATH="$(cd "$(dirname "$DRIVER_DIR")" && pwd):$PATH"
  export PATH
fi

exec wasm-pack test --headless --chrome crates/web "$@"
