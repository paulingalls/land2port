#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 <video-file> [bucket-name]"
    echo ""
    echo "Uploads a video to GCS for processing by the land2port Cloud Run job."
    echo "Creates the bucket and folder structure if they don't exist."
    echo ""
    echo "Examples:"
    echo "  $0 ./video/input.mp4"
    echo "  $0 ./video/input.mp4 my-custom-bucket"
    exit 1
fi

VIDEO_FILE="$1"
PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
BUCKET_NAME="${2:-${PROJECT_ID}-land2port}"
BUCKET_LOCATION="${BUCKET_LOCATION:-europe-west1}"

if [ -z "$PROJECT_ID" ]; then
    echo "Error: No project set. Run: gcloud config set project <PROJECT_ID>"
    exit 1
fi

if [ ! -f "$VIDEO_FILE" ]; then
    echo "Error: File not found: ${VIDEO_FILE}"
    exit 1
fi

BUCKET_URI="gs://${BUCKET_NAME}"

# Create bucket if it doesn't exist
if gcloud storage buckets describe "${BUCKET_URI}" &>/dev/null; then
    echo "Bucket ${BUCKET_URI} already exists."
else
    echo "==> Creating bucket ${BUCKET_URI}..."
    gcloud storage buckets create "${BUCKET_URI}" --location="${BUCKET_LOCATION}" --uniform-bucket-level-access
fi

# Ensure input/ and output/ folders exist (GCS uses placeholder objects)
for folder in input output; do
    if ! gcloud storage ls "${BUCKET_URI}/${folder}/" &>/dev/null; then
        echo "==> Creating ${folder}/ folder..."
        echo -n | gcloud storage cp - "${BUCKET_URI}/${folder}/.keep"
    fi
done

# Upload video
FILENAME=$(basename "$VIDEO_FILE")
echo "==> Uploading ${VIDEO_FILE} to ${BUCKET_URI}/input/${FILENAME}..."
gcloud storage cp "$VIDEO_FILE" "${BUCKET_URI}/input/${FILENAME}"

REGION=$(gcloud config get-value run/region 2>/dev/null)
REGION="${REGION:-europe-west1}"

echo ""
echo "Done! Video uploaded to: ${BUCKET_URI}/input/${FILENAME}"
echo ""
echo "To run the job:"
echo "  gcloud run jobs execute land2port --region=${REGION} \\"
echo "    --args=\"--source,/input/input/${FILENAME},--output-filepath,/output/output/${FILENAME},--headless,--device,trt:0\""
echo ""
echo "To download the result:"
echo "  ./deploy/download-video.sh ${FILENAME}"
