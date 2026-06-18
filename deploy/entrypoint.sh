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

# Run the converter as a child (not `exec`) so that:
#   1. Cloud Run's SIGTERM (sent on cancel/timeout) is forwarded to it, giving
#      it a chance to finalize the output mp4 and exit gracefully; and
#   2. pending FUSE writes are still flushed to GCS afterwards, on success or
#      failure.
trap 'kill -TERM "$child" 2>/dev/null' TERM INT
./land2port "$@" &
child=$!

# Don't let `set -e` abort on a non-zero conversion before we sync.
set +e
wait "$child"
EXIT_CODE=$?
# `wait` returns early when interrupted by the trap; wait again until the child
# has actually exited so we capture its real status.
while kill -0 "$child" 2>/dev/null; do
    wait "$child"
    EXIT_CODE=$?
done
set -e

# Flush all pending FUSE writes to GCS before the container exits
sync

exit "$EXIT_CODE"
