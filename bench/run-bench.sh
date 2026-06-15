#!/usr/bin/env bash
set -euo pipefail

# Runs a labeled local benchmark against a fixed input video and stores all
# artifacts under bench/results/<label>/ for later comparison.
#
# Usage:
#   ./bench/run-bench.sh <label> <input-video> [extra land2port args...]
#
# Examples:
#   ./bench/run-bench.sh baseline ./video/bench-input.mp4 --device cuda:0
#   ./bench/run-bench.sh fast-cut-detect ./video/bench-input.mp4 --device cuda:0
#
# IMPORTANT: use the same input video and the same extra args for every run
# you intend to compare. compare.sh warns if the recorded args differ.

usage() {
    sed -n '4,14p' "$0"
    exit 1
}

if [ $# -lt 2 ]; then
    usage
fi

LABEL="$1"
INPUT="$2"
shift 2

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
RESULT_DIR="${SCRIPT_DIR}/results/${LABEL}"

if [ ! -f "$INPUT" ]; then
    echo "Error: input video not found: $INPUT"
    exit 1
fi

if [ -d "$RESULT_DIR" ]; then
    echo "Error: results for label '${LABEL}' already exist: ${RESULT_DIR}"
    echo "Pick a new label or delete the old results first."
    exit 1
fi

mkdir -p "$RESULT_DIR"

cd "$PROJECT_ROOT"

# On Windows, cargo inside Git Bash picks up the wrong linker — build from a
# native shell first and run with BENCH_NO_BUILD=1 to use the existing binary.
if [ "${BENCH_NO_BUILD:-0}" != "1" ]; then
    echo "==> Building release binary..."
    cargo build --release
fi

BIN="${PROJECT_ROOT}/target/release/land2port"
if [ -f "${BIN}.exe" ]; then
    BIN="${BIN}.exe"
fi
if [ ! -f "$BIN" ]; then
    echo "Error: release binary not found at ${BIN}"
    exit 1
fi

GIT_SHA=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
GIT_DIRTY=$(git status --porcelain 2>/dev/null | head -1 | grep -q . && echo "dirty" || echo "clean")

OUTPUT_VIDEO="${RESULT_DIR}/output.mp4"

{
    echo "label: ${LABEL}"
    echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "git_sha: ${GIT_SHA} (${GIT_DIRTY})"
    echo "input: ${INPUT}"
    echo "input_sha256: $(sha256sum "$INPUT" | cut -d' ' -f1)"
    echo "args: --headless $*"
    echo "mode: local"
} > "${RESULT_DIR}/meta.txt"

echo "==> Running benchmark '${LABEL}'..."
"$BIN" \
    --source "$INPUT" \
    --output-filepath "$OUTPUT_VIDEO" \
    --headless \
    "$@" 2>&1 | tee "${RESULT_DIR}/run.log"

# The binary writes <output>.metrics.json next to the delivered video
if [ -f "${OUTPUT_VIDEO}.metrics.json" ]; then
    mv "${OUTPUT_VIDEO}.metrics.json" "${RESULT_DIR}/metrics.json"
else
    echo "Error: metrics file not found at ${OUTPUT_VIDEO}.metrics.json"
    exit 1
fi

echo ""
echo "==> Benchmark '${LABEL}' complete."
echo "    ${RESULT_DIR}/output.mp4"
echo "    ${RESULT_DIR}/metrics.json"
echo "    ${RESULT_DIR}/run.log"
echo ""
echo "Compare against another run:"
echo "  ./bench/compare.sh <baseline-label> ${LABEL}"
