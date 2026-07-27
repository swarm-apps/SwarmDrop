#!/usr/bin/env bash
# 跑一个对照用例：run-case.sh <标签> <按钮id: bench|down> [等待秒数]
# 环境变量 SPIKE_RWND / SPIKE_SEND_LIMIT 由调用方传入。
set -uo pipefail
cd "$(dirname "$0")"

LABEL="$1"; BTN="$2"; WAIT="${3:-25}"
LOG="/tmp/spike-${LABEL}.log"

pkill -f "release/webrtc-ice-browser-spike" 2>/dev/null
pkill -f "agent-browser" 2>/dev/null
sleep 2

SPIKE_BIND="${SPIKE_BIND:-192.168.50.105,127.0.0.1}" \
  ./target/release/webrtc-ice-browser-spike > "$LOG" 2>&1 &
sleep 3

agent-browser open http://127.0.0.1:8099 >/dev/null 2>&1
sleep 1
agent-browser click "#go" >/dev/null 2>&1
sleep 8
agent-browser click "#$BTN" >/dev/null 2>&1
sleep "$WAIT"

echo "════ [$LABEL] rwnd=${SPIKE_RWND:-default} send_limit=${SPIKE_SEND_LIMIT:-none} 方向=$BTN"
echo "──── Rust 端"
grep -E "cfg|bch|snd|state:" "$LOG" | tail -12
echo "──── 浏览器端"
agent-browser get text "#log" 2>/dev/null | grep -E "^\[(bch|dn |sts)" | tail -6

pkill -f "release/webrtc-ice-browser-spike" 2>/dev/null
