#!/bin/bash
set -e

# Symlink only the TensorRT engine cache to GCS FUSE for persistence
# across job executions. Other caches (model downloads) stay on local
# disk to avoid cross-device rename errors.
if [ -d /data ]; then
    mkdir -p /root/.cache/usls/caches
    mkdir -p /data/cache/tensorrt
    ln -sf /data/cache/tensorrt /root/.cache/usls/caches/tensorrt
fi

exec ./land2port "$@"
