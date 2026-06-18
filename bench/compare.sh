#!/usr/bin/env bash
set -euo pipefail

# Compares two benchmark runs: per-stage timing deltas plus reliability checks
# (output video properties, frame count, duration, audio presence, and visual
# similarity vs the baseline output).
#
# Usage:
#   ./bench/compare.sh <baseline-label> <candidate-label>
#
# Environment:
#   BENCH_SSIM_MIN     minimum mean SSIM vs baseline (default 0.95)
#   BENCH_FRAME_TOL    allowed frame count difference (default 2)
#   BENCH_DURATION_TOL allowed duration difference in seconds (default 0.2)
#
# Exits non-zero if any reliability check fails.

if [ $# -ne 2 ]; then
    sed -n '4,16p' "$0"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BASE_DIR="${SCRIPT_DIR}/results/$1"
CAND_DIR="${SCRIPT_DIR}/results/$2"

SSIM_MIN="${BENCH_SSIM_MIN:-0.95}"
SSIM_MIN_FRAME="${BENCH_SSIM_MIN_FRAME:-0.85}"
FRAME_TOL="${BENCH_FRAME_TOL:-2}"
DURATION_TOL="${BENCH_DURATION_TOL:-0.2}"

FAILURES=0

fail() {
    echo "  FAIL: $1"
    FAILURES=$((FAILURES + 1))
}

pass() {
    echo "  ok:   $1"
}

for dir in "$BASE_DIR" "$CAND_DIR"; do
    for f in metrics.json output.mp4; do
        if [ ! -f "${dir}/${f}" ]; then
            echo "Error: missing ${dir}/${f}"
            exit 1
        fi
    done
done

# --- helpers to parse the (test-locked) metrics.json format -----------------

stage_total() { # file stage -> total_s or empty
    sed -n "s/^    \"$2\": { \"total_s\": \([0-9.]*\),.*/\1/p" "$1"
}

stage_names() { # file -> stage names
    sed -n 's/^    "\([a-z_]*\)": { "total_s".*/\1/p' "$1"
}

counter_value() { # file counter -> value or empty
    sed -n "s/^    \"$2\": \([0-9]*\),\{0,1\}$/\1/p" "$1"
}

top_level() { # file key -> value
    sed -n "s/^  \"$2\": \([0-9.]*\),\{0,1\}$/\1/p" "$1"
}

probe() { # file entries -> value
    ffprobe -v error -select_streams v:0 -show_entries "stream=$2" \
        -of default=noprint_wrappers=1:nokey=1 "$1" 2>/dev/null | head -1
}

# --- timing comparison -------------------------------------------------------

echo ""
echo "================ TIMING: $1 -> $2 ================"

BASE_WALL=$(top_level "${BASE_DIR}/metrics.json" "total_wall_s")
CAND_WALL=$(top_level "${CAND_DIR}/metrics.json" "total_wall_s")
awk -v b="$BASE_WALL" -v c="$CAND_WALL" 'BEGIN {
    printf "%-18s %10.2f %10.2f %+9.1f%%\n", "total_wall_s", b, c, (b > 0 ? (c - b) / b * 100 : 0)
}'

BASE_FRAMES=$(counter_value "${BASE_DIR}/metrics.json" "frames_written")
CAND_FRAMES=$(counter_value "${CAND_DIR}/metrics.json" "frames_written")
if [ -n "$BASE_FRAMES" ] && [ -n "$CAND_FRAMES" ]; then
    awk -v bf="$BASE_FRAMES" -v bw="$BASE_WALL" -v cf="$CAND_FRAMES" -v cw="$CAND_WALL" 'BEGIN {
        bfps = (bw > 0 ? bf / bw : 0); cfps = (cw > 0 ? cf / cw : 0)
        printf "%-18s %10.2f %10.2f %+9.1f%%\n", "throughput_fps", bfps, cfps, (bfps > 0 ? (cfps - bfps) / bfps * 100 : 0)
    }'
fi

echo ""
printf "%-18s %10s %10s %10s\n" "stage" "$1" "$2" "delta"

# Union of stage names across both files, preserving order of appearance
{ stage_names "${BASE_DIR}/metrics.json"; stage_names "${CAND_DIR}/metrics.json"; } \
    | awk '!seen[$0]++' \
    | while read -r stage; do
        b=$(stage_total "${BASE_DIR}/metrics.json" "$stage")
        c=$(stage_total "${CAND_DIR}/metrics.json" "$stage")
        awk -v s="$stage" -v b="${b:--1}" -v c="${c:--1}" 'BEGIN {
            if (b < 0)      printf "%-18s %10s %10.2f %10s\n", s, "-", c, "new"
            else if (c < 0) printf "%-18s %10.2f %10s %10s\n", s, b, "-", "gone"
            else            printf "%-18s %10.2f %10.2f %+9.1f%%\n", s, b, c, (b > 0 ? (c - b) / b * 100 : 0)
        }'
    done

# --- reliability checks ------------------------------------------------------

echo ""
echo "================ RELIABILITY: $2 vs $1 ================"

# Warn (not fail) if the runs were captured with different args or inputs
for key in args input input_sha256; do
    bmeta=$(grep "^${key}:" "${BASE_DIR}/meta.txt" 2>/dev/null || true)
    cmeta=$(grep "^${key}:" "${CAND_DIR}/meta.txt" 2>/dev/null || true)
    if [ -n "$bmeta" ] && [ -n "$cmeta" ] && [ "$bmeta" != "$cmeta" ]; then
        echo "  WARN: ${key} differs between runs — comparison may not be apples-to-apples"
        echo "        ${1}: ${bmeta}"
        echo "        ${2}: ${cmeta}"
    fi
done

BASE_VIDEO="${BASE_DIR}/output.mp4"
CAND_VIDEO="${CAND_DIR}/output.mp4"

CAND_SIZE=$(wc -c < "$CAND_VIDEO")
if [ "$CAND_SIZE" -gt 0 ]; then
    pass "output exists and is non-empty (${CAND_SIZE} bytes)"
else
    fail "output file is empty"
fi

for prop in width height r_frame_rate; do
    b=$(probe "$BASE_VIDEO" "$prop")
    c=$(probe "$CAND_VIDEO" "$prop")
    if [ "$b" = "$c" ]; then
        pass "${prop} matches (${c})"
    else
        fail "${prop} changed: ${b} -> ${c}"
    fi
done

duration_of() {
    ffprobe -v error -show_entries format=duration \
        -of default=noprint_wrappers=1:nokey=1 "$1" 2>/dev/null
}
BASE_DUR=$(duration_of "$BASE_VIDEO")
CAND_DUR=$(duration_of "$CAND_VIDEO")
if awk -v b="$BASE_DUR" -v c="$CAND_DUR" -v tol="$DURATION_TOL" \
    'BEGIN { d = b - c; if (d < 0) d = -d; exit !(d <= tol) }'; then
    pass "duration within ${DURATION_TOL}s (${BASE_DUR} vs ${CAND_DUR})"
else
    fail "duration drifted beyond ${DURATION_TOL}s: ${BASE_DUR} -> ${CAND_DUR}"
fi

frames_of() {
    ffprobe -v error -count_packets -select_streams v:0 \
        -show_entries stream=nb_read_packets \
        -of default=noprint_wrappers=1:nokey=1 "$1" 2>/dev/null
}
BASE_NF=$(frames_of "$BASE_VIDEO")
CAND_NF=$(frames_of "$CAND_VIDEO")
if [ -n "$BASE_NF" ] && [ -n "$CAND_NF" ]; then
    DIFF=$((BASE_NF - CAND_NF))
    [ "$DIFF" -lt 0 ] && DIFF=$((-DIFF))
    if [ "$DIFF" -le "$FRAME_TOL" ]; then
        pass "frame count within ±${FRAME_TOL} (${BASE_NF} vs ${CAND_NF})"
    else
        fail "frame count changed by ${DIFF}: ${BASE_NF} -> ${CAND_NF}"
    fi
else
    fail "could not read frame counts (ffprobe)"
fi

audio_streams() {
    ffprobe -v error -select_streams a -show_entries stream=index \
        -of csv=p=0 "$1" 2>/dev/null | grep -c . || true
}
BASE_AUDIO=$(audio_streams "$BASE_VIDEO")
CAND_AUDIO=$(audio_streams "$CAND_VIDEO")
if [ "$BASE_AUDIO" = "$CAND_AUDIO" ]; then
    pass "audio stream count matches (${CAND_AUDIO})"
else
    fail "audio stream count changed: ${BASE_AUDIO} -> ${CAND_AUDIO}"
fi

echo "  computing SSIM + PSNR vs baseline (this decodes both videos)..."
# Run SSIM and PSNR in a single decode pass. SSIM per-frame values are
# written to a stats file so we can flag the WORST frame, not just the mean
# (a brief glitch can hide under a high average). ffmpeg cannot take an
# MSYS-style path inside a -filter_complex string, so we cd into the
# candidate dir and use a bare relative stats filename + relative input.
SSIM_STATS_NAME="ssim-frames.log"
SSIM_STATS="${CAND_DIR}/${SSIM_STATS_NAME}"
FF_STDERR="${CAND_DIR}/ffmpeg-quality.stderr"
CAND_BASENAME=$(basename "$CAND_VIDEO")
(
    cd "$CAND_DIR" &&
    ffmpeg -nostats -i "$CAND_BASENAME" -i "$BASE_VIDEO" -filter_complex \
        "[0:v]split[c1][c2];[1:v]split[b1][b2];[c1][b1]ssim=stats_file=${SSIM_STATS_NAME};[c2][b2]psnr" \
        -f null - >/dev/null 2>"$(basename "$FF_STDERR")"
) || true

# Mean SSIM + mean PSNR from the ffmpeg summary lines on stderr.
SSIM=$(grep -oE 'All:[0-9.]+' "$FF_STDERR" 2>/dev/null | tail -1 | cut -d: -f2)
PSNR=$(grep -oE 'average:[0-9.infa]+' "$FF_STDERR" 2>/dev/null | tail -1 | cut -d: -f2)

# Worst single-frame SSIM and the frame number where it occurs.
if [ -f "$SSIM_STATS" ]; then
    read -r MIN_SSIM MIN_FRAME < <(awk '
        { for (i = 1; i <= NF; i++) if ($i ~ /^All:/) {
            v = substr($i, 5) + 0
            if (min == "" || v < min) { min = v; fn = $1 }
        } }
        END { print min, fn }' "$SSIM_STATS")
fi

if [ -z "$SSIM" ]; then
    fail "could not compute SSIM (ffmpeg ssim filter)"
else
    [ -n "$PSNR" ] && echo "  info: mean PSNR ${PSNR} dB"
    if awk -v s="$SSIM" -v min="$SSIM_MIN" 'BEGIN { exit !(s >= min) }'; then
        pass "mean SSIM ${SSIM} >= ${SSIM_MIN}"
    else
        fail "mean SSIM ${SSIM} below threshold ${SSIM_MIN} — output visually diverged"
    fi

    if [ -n "$MIN_SSIM" ]; then
        if awk -v s="$MIN_SSIM" -v min="$SSIM_MIN_FRAME" 'BEGIN { exit !(s >= min) }'; then
            pass "worst-frame SSIM ${MIN_SSIM} (${MIN_FRAME}) >= ${SSIM_MIN_FRAME}"
        else
            fail "worst-frame SSIM ${MIN_SSIM} at ${MIN_FRAME} below ${SSIM_MIN_FRAME} — localized divergence (see ${SSIM_STATS_NAME})"
        fi
    else
        fail "could not read per-frame SSIM stats from ${SSIM_STATS_NAME}"
    fi
fi

echo ""
if [ "$FAILURES" -gt 0 ]; then
    echo "RESULT: ${FAILURES} reliability check(s) FAILED"
    exit 1
fi
echo "RESULT: all reliability checks passed"
