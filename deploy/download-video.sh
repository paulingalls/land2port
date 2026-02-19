#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 <filename> [destination] [bucket-name]"
    echo ""
    echo "Downloads a processed video from the GCS output folder."
    echo ""
    echo "Examples:"
    echo "  $0 input.mp4                    # downloads to ./input.mp4"
    echo "  $0 input.mp4 ./results/         # downloads to ./results/input.mp4"
    echo "  $0 input.mp4 . my-custom-bucket"
    exit 1
fi

FILENAME="$1"
DESTINATION="${2:-.}"
PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
BUCKET_NAME="${3:-${PROJECT_ID}-land2port}"

if [ -z "$PROJECT_ID" ]; then
    echo "Error: No project set. Run: gcloud config set project <PROJECT_ID>"
    exit 1
fi

BUCKET_URI="gs://${BUCKET_NAME}"
SOURCE="${BUCKET_URI}/output/${FILENAME}"

# Check the file exists
if ! gcloud storage ls "$SOURCE" &>/dev/null; then
    echo "Error: File not found: ${SOURCE}"
    echo ""
    echo "Available files in output/:"
    gcloud storage ls "${BUCKET_URI}/output/" 2>/dev/null || echo "  (none)"
    exit 1
fi

# Create destination directory if needed
if [ "$DESTINATION" != "." ] && [ ! -d "$DESTINATION" ]; then
    mkdir -p "$DESTINATION"
fi

echo "==> Downloading ${SOURCE}..."
gcloud storage cp "$SOURCE" "$DESTINATION"

echo ""
echo "Done! Saved to: ${DESTINATION}/${FILENAME}"
