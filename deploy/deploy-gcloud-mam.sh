#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
    echo "Usage: $0 <project-id> <bucket-name> <service-account-email> [job-name]"
    echo ""
    echo "Deploys land2port as a Cloud Run Job using an existing GCS bucket."
    echo ""
    echo "Arguments:"
    echo "  project-id             GCP project ID (required)"
    echo "  bucket-name            Name of an existing GCS bucket (required)"
    echo "  service-account-email  Service account email for job execution (required)"
    echo "  job-name               Cloud Run job name (default: land2port)"
    echo ""
    echo "Examples:"
    echo "  $0 my-project my-existing-bucket sa@project.iam.gserviceaccount.com"
    echo "  $0 my-project my-existing-bucket sa@project.iam.gserviceaccount.com land2port-prod"
    exit 1
}

if [ $# -lt 3 ]; then
    usage
fi

PROJECT_ID="$1"
BUCKET_NAME="$2"
SERVICE_ACCOUNT="$3"
JOB_NAME="${4:-land2port}"
IMAGE_NAME="land2port"

REGION=$(gcloud config get-value run/region 2>/dev/null)
REGION="${REGION:-us-central1}"


REPO="${ARTIFACT_REPO:-docker-repo}"
REGISTRY_REGION="${ARTIFACT_REGION:-us-central1}"
REGISTRY="${REGISTRY_REGION}-docker.pkg.dev/${PROJECT_ID}/${REPO}"
IMAGE_URI="${REGISTRY}/${IMAGE_NAME}:latest"

echo "Project:         ${PROJECT_ID}"
echo "Run region:      ${REGION}"
echo "Registry region: ${REGISTRY_REGION}"
echo "Image:           ${IMAGE_URI}"
echo "Job name:        ${JOB_NAME}"
echo "Bucket:          ${BUCKET_NAME}"
echo "Service account: ${SERVICE_ACCOUNT}"
echo ""

# Enable required APIs - unnecessary for MAM, commented out for this script for now
# echo "==> Enabling APIs..."
#gcloud services enable run.googleapis.com artifactregistry.googleapis.com

# Create Artifact Registry repo (idempotent)
echo "==> Creating Artifact Registry repo..."
gcloud artifacts repositories create "${REPO}" \
    --project="${PROJECT_ID}" \
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

# Create Cloud Run Job with GCS volume mounts
echo "==> Creating Cloud Run Job..."
gcloud run jobs create "${JOB_NAME}" \
    --project="${PROJECT_ID}" \
    --image="${IMAGE_URI}" \
    --region="${REGION}" \
    --service-account="${SERVICE_ACCOUNT}" \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --cpu=4 \
    --memory=16Gi \
    --no-gpu-zonal-redundancy \
    --task-timeout=3600 \
    --max-retries=0 \
    --add-volume=name=data,type=cloud-storage,bucket="${BUCKET_NAME}" \
    --add-volume-mount=volume=data,mount-path=/data

# Grant storage access to the service account
#echo "==> Granting storage access to service account..."
#gcloud storage buckets add-iam-policy-binding "gs://${BUCKET_NAME}" \
#    --member="serviceAccount:${SERVICE_ACCOUNT}" \
#    --role="roles/storage.objectAdmin" \
#    --quiet 2>/dev/null || true

echo ""
echo "==> Done! Job '${JOB_NAME}' created."
echo ""
echo "Upload a video:"
echo "  ./deploy/upload-video.sh ./video/input.mp4 ${BUCKET_NAME}"
echo ""
echo "Execute the job:"
echo "  ./deploy/run-job.sh VIDEO.mp4 --bucket ${BUCKET_NAME}"
echo ""
echo "Download the result:"
echo "  ./deploy/download-video.sh VIDEO.mp4"
