#!/usr/bin/env bash
#
# CLI 发版：构造正确的 tag 并在打 tag 前校验 release notes 的来源。
#
# 为什么需要它：dist 的 **announcement 粒度由 tag 形式决定**，而粒度又决定了 release
# notes 取哪份 CHANGELOG —— 两者之间没有任何配置项可以解耦，全靠打 tag 的人记对格式。
#
#   cli/v0.1.0                 → 剥掉 namespace 只剩版本号 → dist 判定「整个 workspace
#                                统一发布」→ 取 **workspace root** 的 CHANGELOG.md
#                                （那是**桌面端**的版本线！）
#   cli/swarmdrop-cli-v0.1.0   → 带包名 → 包级 announcement → 取 crates/cli/CHANGELOG.md
#
# 前者不会报错：dist 会在根 CHANGELOG.md 里按版本号找到桌面端的同号条目，"successfully
# parsed changelog" 然后把它当作 CLI 的 release notes 发出去。cli/v0.1.0 就是这么把
# 2026-02-14 的「限制 Android 构建目标为 aarch64」发成 CLI 首个版本的说明的。
#
# 三条版本线共存放大了这个坑：桌面已经走到 0.23.x，CLI 从 0.1.0 重新起步，于是 CLI 的
# 版本号**必然**与桌面历史条目撞车 —— 0.1.x / 0.2.x 一路都会撞，撞上就静默取错。
#
# 用法：
#   ./scripts/release-cli.sh              # 校验并打 tag（不推）
#   ./scripts/release-cli.sh --push       # 校验、打 tag、推送（触发 cli-release.yml）
#   ./scripts/release-cli.sh --check-only # 只校验，不碰 git

set -euo pipefail

cd "$(dirname "$0")/.."

MODE="${1:-tag}"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/cli/Cargo.toml | head -1)
[ -n "$VERSION" ] || { echo "✗ 读不出 crates/cli/Cargo.toml 的 version" >&2; exit 1; }

# 包级形式。**不要**简写成 cli/v$VERSION —— 见文件头。
TAG="cli/swarmdrop-cli-v$VERSION"

echo "版本 $VERSION → tag $TAG"

command -v dist >/dev/null || { echo "✗ 没装 dist：cargo install cargo-dist" >&2; exit 1; }

# ── 1. CLI 自己的 changelog 里得有这个版本 ────────────────────────────────────
grep -q "^## \[$VERSION\]" crates/cli/CHANGELOG.md \
  || { echo "✗ crates/cli/CHANGELOG.md 里没有 ## [$VERSION] 条目" >&2; exit 1; }

# ── 2. dist 解析出来的 notes 必须真的来自 CLI 的 changelog ────────────────────
# 判据是内容而不是配置：取 dist 给出的正文前三行非空文本，逐行回查 crates/cli/CHANGELOG.md。
# dist 若取了根（桌面）的那份，这些行一行都对不上。
NOTES=$(dist plan --tag="$TAG" --output-format=json 2>/dev/null | jq -r '.announcement_changelog // empty')
[ -n "$NOTES" ] || { echo "✗ dist 没能为 $TAG 解析出任何 changelog" >&2; exit 1; }

while IFS= read -r line; do
  grep -qF -- "$line" crates/cli/CHANGELOG.md || {
    echo "✗ release notes 不是来自 crates/cli/CHANGELOG.md —— dist 取到了别处的内容：" >&2
    echo "    对不上的行：$line" >&2
    echo "  多半是 tag 形式退回了 cli/v${VERSION}（版本级 announcement）。见本脚本文件头。" >&2
    exit 1
  }
done < <(grep -v '^[[:space:]]*$' <<<"$NOTES" | head -3)

echo "✓ release notes 来自 crates/cli/CHANGELOG.md"

[ "$MODE" = "--check-only" ] && exit 0

# ── 3. 打 tag ────────────────────────────────────────────────────────────────
[ -z "$(git status --porcelain)" ] || { echo "✗ 工作树不干净" >&2; exit 1; }

git tag -a "$TAG" -m "swarmdrop-cli $VERSION"
echo "✓ 已打 tag $TAG"

if [ "$MODE" = "--push" ]; then
  git push origin "$TAG"
  echo "✓ 已推送，cli-release.yml 将被触发"
else
  echo "  推送：git push origin $TAG"
fi
