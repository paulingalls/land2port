#!/usr/bin/env bash
set -euo pipefail

# Runs a labeled benchmark on the Cloud Run job against a video already
# uploaded to the GCS bucket (see deploy/upload-video.sh), waits for it to
# finish, then downloads the output video + metrics into bench/results/<label>/.
#
# Usage:
#   ./bench/run-bench-cloud.sh <label> <filename-in-bucket> [run-job options...]
#
# Examples:
#   ./bench/run-bench-cloud.sh baseline bench-input.mp4
#   ./bench/run-bench-cloud.sh fast-cut-detect bench-input.mp4 --scale m
#
# Options are the same as deploy/run-job.sh (--object, --scale, --device, ...).
# Use the same input file and options for every run you intend to compare.

usage() {
    sed -n '4,16p' "$0"
    exit 1
}

if [ $# -lt 2 ]; then
    usage
fi

LABEL="$1"
FILENAME="$2"
shift 2

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
RESULT_DIR="${SCRIPT_DIR}/results/${LABEL}"

DEVICE="trt:0"
JOB_NAME="land2port"
EXTRA_ARGS=""

PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
BUCKET_NAME="${PROJECT_ID}-land2port"
REGION=$(gcloud config get-value run/region 2>/dev/null)
REGION="${REGION:-us-central1}"
# Optional path prefix inside the bucket (e.g. "temp/"). Trailing slash is
# normalized below. Inputs are read from <prefix>input/, outputs written to
# <prefix>output/bench/<label>/.
PREFIX="${BENCH_BUCKET_PREFIX:-}"

while [ $# -gt 0 ]; do
    case "$1" in
        --device)
            DEVICE="$2"; shift 2 ;;
        --job-name)
            JOB_NAME="$2"; shift 2 ;;
        --bucket)
            BUCKET_NAME="$2"; shift 2 ;;
        --prefix)
            PREFIX="$2"; shift 2 ;;
        --object|--scale|--ver|--smooth-percentage|--smooth-duration|--object-prob-threshold|--cut-similarity|--cut-start|--text-area-threshold|--text-prob-threshold)
            EXTRA_ARGS="${EXTRA_ARGS},${1},${2}"; shift 2 ;;
        --use-stack-crop|--use-simple-smoothing|--keep-text|--prioritize-text|--add-captions)
            EXTRA_ARGS="${EXTRA_ARGS},${1}"; shift ;;
        -h|--help)
            usage ;;
        *)
            echo "Unknown option: $1"
            usage ;;
    esac
done

if [ -d "$RESULT_DIR" ]; then
    echo "Error: results for label '${LABEL}' already exist: ${RESULT_DIR}"
    echo "Pick a new label or delete the old results first."
    exit 1
fi

# Normalize the prefix: strip any leading slash, ensure a single trailing slash
# when non-empty (so "temp" and "temp/" both become "temp/", "" stays "").
PREFIX="${PREFIX#/}"
if [ -n "$PREFIX" ]; then
    PREFIX="${PREFIX%/}/"
fi

if ! gcloud storage ls "gs://${BUCKET_NAME}/${PREFIX}input/${FILENAME}" &>/dev/null; then
    echo "Error: file not found in bucket: gs://${BUCKET_NAME}/${PREFIX}input/${FILENAME}"
    echo "Upload it first: gcloud storage cp /path/to/${FILENAME} gs://${BUCKET_NAME}/${PREFIX}input/"
    exit 1
fi

mkdir -p "$RESULT_DIR"

# Write each benchmark's output under its own prefix so runs never overwrite
# each other or the regular output/ files.
REMOTE_OUTPUT="/data/${PREFIX}output/bench/${LABEL}/output.mp4"
ARGS="--source,/data/${PREFIX}input/${FILENAME},--output-filepath,${REMOTE_OUTPUT},--headless,--device,${DEVICE}${EXTRA_ARGS}"

GIT_SHA=$(git -C "$PROJECT_ROOT" rev-parse HEAD 2>/dev/null || echo "unknown")

{
    echo "label: ${LABEL}"
    echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "git_sha: ${GIT_SHA}"
    echo "input: gs://${BUCKET_NAME}/${PREFIX}input/${FILENAME}"
    echo "args: --headless --device ${DEVICE}$(echo "${EXTRA_ARGS}" | tr ',' ' ')"
    echo "mode: cloud (job=${JOB_NAME}, region=${REGION})"
} > "${RESULT_DIR}/meta.txt"

echo "==> Executing job '${JOB_NAME}' (waits for completion)..."
gcloud run jobs execute "${JOB_NAME}" \
    --region="${REGION}" \
    --args="${ARGS}" \
    --wait

EXECUTION=$(gcloud run jobs executions list --job="${JOB_NAME}" --region="${REGION}" \
    --limit=1 --format="value(name)")

echo "==> Recording execution timing for ${EXECUTION}..."
gcloud run jobs executions describe "${EXECUTION}" --region="${REGION}" \
    --format="value(status.startTime,status.completionTime)" >> "${RESULT_DIR}/meta.txt"

echo "==> Downloading results..."
gcloud storage cp "gs://${BUCKET_NAME}/${PREFIX}output/bench/${LABEL}/output.mp4" "${RESULT_DIR}/output.mp4"
gcloud storage cp "gs://${BUCKET_NAME}/${PREFIX}output/bench/${LABEL}/output.mp4.metrics.json" "${RESULT_DIR}/metrics.json"

echo ""
echo "==> Benchmark '${LABEL}' complete."
echo "    ${RESULT_DIR}/output.mp4"
echo "    ${RESULT_DIR}/metrics.json"
echo ""
echo "Compare against another run:"
echo "  ./bench/compare.sh <baseline-label> ${LABEL}"
