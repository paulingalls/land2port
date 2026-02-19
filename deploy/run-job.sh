#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "Usage: $0 <filename> [options]"
    echo ""
    echo "Executes the land2port Cloud Run job on a video already uploaded to GCS."
    echo ""
    echo "Options:"
    echo "  --object <type>           Object type: face, head, ball, person, car, etc. (default: face)"
    echo "  --scale <s>               Model scale: n, s, m, l (default: s)"
    echo "  --ver <v>                 Model version: 6, 8, 10, 11 (default: 11)"
    echo "  --smooth-percentage <f>   Smooth percentage threshold (default: 7.5)"
    echo "  --smooth-duration <f>     Smooth duration in seconds (default: 1.0)"
    echo "  --object-prob-threshold <f>  Object probability threshold (default: 0.75)"
    echo "  --cut-similarity <f>      Cut similarity threshold (default: 0.4)"
    echo "  --cut-start <f>           Cut start threshold (default: 0.8)"
    echo "  --use-stack-crop          Enable stack crop"
    echo "  --use-simple-smoothing    Use simple smoothing instead of history smoothing"
    echo "  --keep-text               Keep text in frame"
    echo "  --prioritize-text         Prioritize text detection"
    echo "  --text-area-threshold <f> Text area threshold (default: 0.008)"
    echo "  --text-prob-threshold <f> Text probability threshold (default: 0.85)"
    echo "  --add-captions            Extract audio, transcribe, and burn captions"
    echo "  --device <d>              Device: trt:0, cuda:0 (default: trt:0)"
    echo "  --job-name <name>         Cloud Run job name (default: land2port)"
    echo "  --bucket <name>           GCS bucket name (default: <project-id>-land2port)"
    echo ""
    echo "Examples:"
    echo "  $0 input.mp4"
    echo "  $0 input.mp4 --object head --scale m"
    echo "  $0 input.mp4 --keep-text --add-captions"
    exit 1
}

if [ $# -lt 1 ]; then
    usage
fi

FILENAME="$1"
shift

# Defaults
DEVICE="trt:0"
JOB_NAME="land2port"
EXTRA_ARGS=""

PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
BUCKET_NAME="${PROJECT_ID}-land2port"
REGION=$(gcloud config get-value run/region 2>/dev/null)
REGION="${REGION:-europe-west1}"

# Parse options
while [ $# -gt 0 ]; do
    case "$1" in
        --device)
            DEVICE="$2"; shift 2 ;;
        --job-name)
            JOB_NAME="$2"; shift 2 ;;
        --bucket)
            BUCKET_NAME="$2"; shift 2 ;;
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

# Verify the input file exists in the bucket
if ! gcloud storage ls "gs://${BUCKET_NAME}/input/${FILENAME}" &>/dev/null; then
    echo "Error: File not found in bucket: gs://${BUCKET_NAME}/input/${FILENAME}"
    echo ""
    echo "Upload it first:"
    echo "  ./deploy/upload-video.sh /path/to/${FILENAME}"
    exit 1
fi

ARGS="--source,/data/input/${FILENAME},--output-filepath,/data/output/${FILENAME},--headless,--device,${DEVICE}${EXTRA_ARGS}"

echo "Job:      ${JOB_NAME}"
echo "Region:   ${REGION}"
echo "Input:    gs://${BUCKET_NAME}/input/${FILENAME}"
echo "Device:   ${DEVICE}"
echo ""

echo "==> Executing job..."
gcloud run jobs execute "${JOB_NAME}" \
    --region="${REGION}" \
    --args="${ARGS}"

echo ""
echo "To check status:"
echo "  gcloud run jobs executions list --job=${JOB_NAME} --region=${REGION} --limit=1"
echo ""
echo "To download the result when done:"
echo "  ./deploy/download-video.sh ${FILENAME}"
