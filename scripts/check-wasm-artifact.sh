#!/usr/bin/env bash
#
# 产物新鲜度护栏：wasm 侧的源码变了，入库的 wasm 产物就得跟着重建。
#
# 为什么需要它：`.github/workflows/docs.yml` 自 2026-08-13 起**不再在 CI 里重新生成
# wasm**，改吃仓库入库的 `packages/swarmdrop-web/`。原因写在那个文件里 —— 简短版是
# Ubuntu 的 apt binaryen 停在 108（2022 年），它重排 table 却不重映射导出索引，产出的
# `__wbindgen_externrefs` 会指到 funcref 表上（`min == max`，不可 grow），于是线上一
# 加载就 `RangeError: WebAssembly.Table.grow()`，而本地怎么试都复现不了。
#
# 代价是「产物可能过期」这条风险失去了 CI 兜底 —— 改了 wire 协议却忘了重建产物，
# 线上 Web 端会静默地停在旧协议上，与新版桌面/移动端不兼容。这个脚本就是那道兜底。
#
# 用法：
#   ./scripts/check-wasm-artifact.sh [base-ref]
#
# base-ref 默认 origin/main；CI 由 rust.yml 传入（PR 传 base sha，push 传 before sha）。
#
# 逃生舱：源码动了但产物字节确实不变时，`pnpm build:wasm` 跑完 git 是干净的，这条检查
# 会永远红。此时在**该分支任一 commit 的 message** 里写 [wasm-artifact-unchanged]
# 放行 —— 刻意要求留痕，而不是默默跳过。
#
# ⚠️ 它比听起来窄得多：wasm 里嵌着 panic location 元数据（`file:line`），**任何往文件
# 中间插代码的改动都会挪动行号、从而改掉字节**。实测过一次 —— 加一个 wasm 侧根本没人
# 调用的 `pub fn`，产物大小一字节没变、内容差 8 个字节，全是行号。所以逃生舱基本只对
# 「在文件末尾追加」成立；测试 / examples / `*.md` 已由下面的 EXCLUDE 自动排除，不必
# 走逃生舱。判不准就重建一次，差异落在路径字符串附近就是纯行号漂移。

set -euo pipefail

BASE="${1:-origin/main}"
ARTIFACT_DIR="packages/swarmdrop-web"
SKIP_MARKER="[wasm-artifact-unchanged]"

# 产物的输入面。前 8 个与 `scripts/check-wasm.sh` 的 CRATES 一一对齐（那份列表是
# 「哪些 crate 要能编到 wasm」的权威源），另加两项：
#   - crates/entity —— swarmdrop-web 依赖它，但它自己不在 check-wasm 的 -p 列表里
#   - Cargo.lock    —— 依赖版本变了，产物也会变，源码却一行没动
# 刻意**不含** webtransport-p2p / storage-sql / migration / bootstrap：它们是 native-only，
# 进不了 wasm 产物。
WASM_SOURCES=(
  "crates/webrtc-p2p"
  "crates/net-base"
  "crates/net"
  "crates/host"
  "crates/transfer"
  "crates/invite"
  "crates/core"
  "crates/entity"
  "crates/web"
  "Cargo.lock"
)

# `^{commit}` 不能省：新建分支时 GitHub 传来的 github.event.before 是 40 个 0，
# 那串东西格式合法、`rev-parse --verify` 会放行，随后 `git diff` 才炸。
if ! git rev-parse --verify --quiet "${BASE}^{commit}" >/dev/null 2>&1; then
  echo "⚠️  基准 ref '$BASE' 不可达，跳过产物新鲜度检查。"
  echo "    （CI 里这通常意味着 checkout 的 fetch-depth 不够，本地则是没有 origin/main。）"
  exit 0
fi

changed="$(git diff --name-only "$BASE...HEAD")"

# 进不了 wasm 产物的东西，即使躺在上面那些目录里也不算改动。
#
# `crates/*/tests/` 与 `examples/` 是**独立的编译单元**，从不链进 cdylib；`*.md` 更不用说。
# 不排除它们的话，「只改了一条 e2e 测试」也会要求重建 4.8 MB 的二进制 —— 2026-08-20
# 就这么红过一次（改 `crates/core/tests/e2e_transfer.rs`，产物一个字节都不会变）。
#
# ⚠️ **这里是黑名单而不是白名单，是刻意的。** 白名单（只认 `src/` + `Cargo.toml` +
# `build.rs`）看起来更精确，但它对**新出现的输入种类**默认放行 —— 而本脚本存在的全部
# 理由就是防「产物悄悄过期」。实例：`crates/web/bindings/bindings.ts` 被
# `src/node.rs` 用 `include_str!` 吃进二进制，它既不在 `src/` 下也不叫 `.rs`，白名单
# 会直接漏掉它。误报只是烦人，漏报会把旧协议发到线上。
#
# 目录部分**只认 crate 顶层**（`crates/<名>/tests/`），不写成 `(^|/)tests/`：后者会连
# `src/**/tests/` 一起吃掉，而那种是**编进 lib 的模块目录**。当前没有这样的目录（加规则
# 时 find 过），写死顶层是为了将来有人加时不会静默失效。
#
# 唯一已知的残余风险：将来若有人 `include_str!` 一个 `.md`，这里会漏掉它。
EXCLUDE='^crates/[^/]+/(tests|examples|benches)/|\.md$'

# 这些条目会拼成正则，所以 `.` 要转义 —— 否则 `Cargo.lock` 里的点是通配符。
pattern="$(IFS='|'; printf '%s' "${WASM_SOURCES[*]}" | sed 's/\./\\./g')"

touched_sources="$(printf '%s\n' "$changed" \
  | grep -E "^(${pattern})(/|$)" \
  | grep -Ev "$EXCLUDE" || true)"

if [ -z "$touched_sources" ]; then
  echo "✅ 本次改动不涉及 wasm 侧源码，无需重建产物。"
  exit 0
fi

touched_artifact="$(printf '%s\n' "$changed" | grep -E "^${ARTIFACT_DIR}/" || true)"

if [ -n "$touched_artifact" ]; then
  echo "✅ wasm 侧源码有改动，且 ${ARTIFACT_DIR}/ 已同步重建。"
  exit 0
fi

if git log --format=%B "$BASE..HEAD" | grep -qF "$SKIP_MARKER"; then
  echo "✅ 检出 ${SKIP_MARKER} 标记，按「产物字节不变」放行。"
  exit 0
fi

cat >&2 <<EOF
❌ wasm 侧源码有改动，但 ${ARTIFACT_DIR}/ 没跟着重建。

改动到的路径：
$(printf '%s\n' "$touched_sources" | sed 's/^/  /')

线上 Web 端吃的是入库产物（docs.yml 不再在 CI 里重新生成，理由见该文件注释），
不重建就意味着 Pages 上的 Web 端仍跑在旧代码上 —— 若这次动的是 wire 协议，
它会与新版桌面/移动端静默不兼容。

修复：
  cd docs && pnpm build:wasm && cd ..
  git add ${ARTIFACT_DIR} && git commit

如果重建后产物字节确实没变，在本分支任一 commit 的 message 里加上
${SKIP_MARKER} 放行。但先看清楚：往文件中间插代码会挪动 panic location 里的行号，
产物字节会变而行为不变 —— 那种情况老实提交重建后的产物即可。
EOF
exit 1
