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

# Enable required APIs
echo "==> Enabling APIs..."
gcloud services enable run.googleapis.com artifactregistry.googleapis.com

# Create Artifact Registry repo (idempotent)
echo "==> Creating Artifact Registry repo..."
gcloud artifacts repositories create "${REPO}" \
    --repository-format=docker \
    --location="${REGISTRY_REGION}" \
    --description="Land2Port Docker images" \
    2>/dev/null || true

# Configure Docker auth
echo "==> Configuring Docker auth..."
gcloud auth configure-docker "${REGISTRY_REGION}-docker.pkg.dev" --quiet

# Build image
echo "==> Building Docker image..."
docker build --platform linux/amd64 -f "${SCRIPT_DIR}/Dockerfile.gcloud" -t "${IMAGE_URI}" "${PROJECT_ROOT}"

# Push image
echo "==> Pushing image to Artifact Registry..."
docker push "${IMAGE_URI}"

BUCKET_NAME="${BUCKET_NAME:-${PROJECT_ID}-land2port}"

# Create Cloud Run Job with GCS volume mounts
echo "==> Creating Cloud Run Job..."
gcloud run jobs create "${JOB_NAME}" \
    --image="${IMAGE_URI}" \
    --region="${REGION}" \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --cpu=4 \
    --memory=16Gi \
    --no-gpu-zonal-redundancy \
    --task-timeout=3600 \
    --max-retries=0 \
    --set-env-vars=XDG_CACHE_HOME=/data/cache \
    --add-volume=name=data,type=cloud-storage,bucket="${BUCKET_NAME}" \
    --add-volume-mount=volume=data,mount-path=/data

# Grant storage access to the default compute service account
echo "==> Granting storage access to compute service account..."
PROJECT_NUMBER=$(gcloud projects describe "${PROJECT_ID}" --format="value(projectNumber)")
gcloud storage buckets add-iam-policy-binding "gs://${BUCKET_NAME}" \
    --member="serviceAccount:${PROJECT_NUMBER}-compute@developer.gserviceaccount.com" \
    --role="roles/storage.objectAdmin" \
    --quiet 2>/dev/null || true

echo ""
echo "==> Done! Job '${JOB_NAME}' created."
echo ""
echo "Upload a video:"
echo "  ./deploy/upload-video.sh ./video/input.mp4"
echo ""
echo "Execute the job:"
echo "  gcloud run jobs execute ${JOB_NAME} --region=${REGION} \\"
echo "    --args=\"--source,/data/input/VIDEO.mp4,--output-filepath,/data/output/VIDEO.mp4,--headless,--device,trt:0\""
echo ""
echo "Download the result:"
echo "  ./deploy/download-video.sh VIDEO.mp4"
