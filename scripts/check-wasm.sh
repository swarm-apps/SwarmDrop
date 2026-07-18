#!/usr/bin/env bash
# 新网络内核的 wasm32 双 target 检查（M0 起每次 PR 必跑，CI 同款）。
# 用法：scripts/check-wasm.sh [--clippy]
set -euo pipefail
cd "$(dirname "$0")/.."

# wasm32 target 未安装则补装
rustup target list --installed | grep -q '^wasm32-unknown-unknown$' \
  || rustup target add wasm32-unknown-unknown

# macOS：Apple clang 无 wasm backend（`clang -print-targets` 一条 wasm 都没有），
# ring 等 C 依赖编 wasm 必挂，需 Homebrew LLVM。Linux 发行版 clang 通常自带 wasm。
if [[ "$(uname)" == "Darwin" ]]; then
  BREW_LLVM="/opt/homebrew/opt/llvm/bin"
  if [[ -x "$BREW_LLVM/clang" ]]; then
    export CC_wasm32_unknown_unknown="$BREW_LLVM/clang"
    export AR_wasm32_unknown_unknown="$BREW_LLVM/llvm-ar"
  else
    echo "warn: Homebrew LLVM 未安装（brew install llvm）；若 C 依赖编 wasm 失败请先安装" >&2
  fi
fi

CRATES=(-p swarmdrop-net-base -p swarmdrop-net -p swarmdrop-host -p swarmdrop-transfer -p swarmdrop-web)

if [[ "${1:-}" == "--clippy" ]]; then
  cargo clippy "${CRATES[@]}" --target wasm32-unknown-unknown -- -D warnings
else
  cargo check "${CRATES[@]}" --target wasm32-unknown-unknown
fi
