#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

JOB_NAME="${1:-land2port}"
IMAGE_NAME="land2port"

# Read project and region from gcloud config
PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
REGION=$(gcloud config get-value run/region 2>/dev/null)
REGION="${REGION:-us-central1}"

if [ -z "$PROJECT_ID" ]; then
    echo "Error: No project set. Run: gcloud config set project <PROJECT_ID>"
    exit 1
fi

REPO="${ARTIFACT_REPO:-docker-repo}"
REGISTRY_REGION="${ARTIFACT_REGION:-europe-west1}"
REGISTRY="${REGISTRY_REGION}-docker.pkg.dev/${PROJECT_ID}/${REPO}"
IMAGE_URI="${REGISTRY}/${IMAGE_NAME}:latest"

echo "Project:         ${PROJECT_ID}"
echo "Run region:      ${REGION}"
echo "Registry region: ${REGISTRY_REGION}"
echo "Image:           ${IMAGE_URI}"
echo "Job name:        ${JOB_NAME}"
echo ""

# Build image
echo "==> Building Docker image..."
docker build --platform linux/amd64 -f "${SCRIPT_DIR}/Dockerfile.gcloud" -t "${IMAGE_URI}" "${PROJECT_ROOT}"

# Push image
echo "==> Pushing image to Artifact Registry..."
docker push "${IMAGE_URI}"

# Update Cloud Run Job with latest image
echo "==> Updating Cloud Run Job..."
gcloud run jobs update "${JOB_NAME}" \
    --image="${IMAGE_URI}" \
    --region="${REGION}" \
    --set-env-vars=XDG_CACHE_HOME=/data/cache

echo ""
echo "==> Done! Job '${JOB_NAME}' updated with latest image."
