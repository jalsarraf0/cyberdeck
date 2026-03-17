#!/usr/bin/env bash
set -euo pipefail

##############################################################################
# docker-runner-parallel: Run ephemeral containers in parallel with fanout
##############################################################################

CACHE_DIR="${DOCKER_RUNNER_CACHE:-/tmp/docker-runner}"
CACHE_ARTIFACTS="${CACHE_DIR}/artifacts"
PARALLEL_JOBS="${DOCKER_RUNNER_PARALLEL_JOBS:-5}"
RUNNER_ID="docker-parallel-$$-$(date +%s%N | tail -c 7)"

mkdir -p "${CACHE_ARTIFACTS}"

# Track all background processes
declare -a PIDS=()
declare -A PID_NAMES=()
declare -A PID_RESULTS=()
FAILED=0

cleanup() {
    # Kill all remaining background processes
    for pid in "${PIDS[@]}"; do
        kill $pid 2>/dev/null || true
    done

    # Wait for all to finish
    for pid in "${PIDS[@]}"; do
        wait $pid 2>/dev/null || true
    done
}
trap cleanup EXIT

run_container() {
    local name="$1"
    local image="$2"
    local command="$3"
    local output_file="${CACHE_ARTIFACTS}/${name}-${RUNNER_ID}.log"

    echo ">>> [${name}] Starting in ${image}" >&2

    # Pull image
    docker pull "${image}" >/dev/null 2>&1 || return 1

    # Run container and log output
    if docker run \
        --rm \
        --name "${RUNNER_ID}-${name}" \
        -v "$(pwd):/workspace" \
        -w "/workspace" \
        -e "CI=${CI:-false}" \
        -e "GITHUB_RUN_ID=${GITHUB_RUN_ID:-}" \
        -e "GITHUB_RUN_ATTEMPT=${GITHUB_RUN_ATTEMPT:-}" \
        "${image}" \
        /bin/sh -c "${command}" > "${output_file}" 2>&1; then
        echo ">>> [${name}] ✓ Passed" >&2
        PID_RESULTS["${name}"]=0
        return 0
    else
        echo ">>> [${name}] ✗ Failed (see ${output_file})" >&2
        PID_RESULTS["${name}"]=1
        cat "${output_file}" >&2
        FAILED=$((FAILED + 1))
        return 1
    fi
}

# Parse JSON config format: [{"name":"test","image":"alpine","command":"echo test"},...]
if [[ $# -lt 1 ]]; then
    cat >&2 <<EOF
Usage: docker-runner-parallel '<json-config>' [--max-parallel N] [--prune]

JSON Format:
[
  {"name": "test1", "image": "alpine:latest", "command": "apk update && apk add curl"},
  {"name": "test2", "image": "debian:bookworm-slim", "command": "apt-get update && apt-get install -y curl"}
]

Environment variables:
  DOCKER_RUNNER_CACHE              Cache directory (default: /tmp/docker-runner)
  DOCKER_RUNNER_PARALLEL_JOBS      Max parallel jobs (default: 5, overridable by --max-parallel)
  DOCKER_RUNNER_PRUNE              Prune old images after running

Options:
  --max-parallel N   Limit concurrent containers to N (default: 5)
  --prune           Prune unused images after completion

Examples:
  docker-runner-parallel '[{"name":"rust","image":"rust:latest","command":"cargo test"}]'
  docker-runner-parallel '[...json...]' --max-parallel 8 --prune
EOF
    exit 1
fi

CONFIG="$1"
shift || true

# Parse options
while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-parallel)
            PARALLEL_JOBS="$2"
            shift 2
            ;;
        --prune)
            SHOULD_PRUNE=true
            shift
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Parse and validate JSON
if ! command -v jq &>/dev/null; then
    echo "Error: jq is required for JSON parsing. Install it and retry." >&2
    exit 1
fi

if ! JOBS=$(echo "$CONFIG" | jq -c '.[]' 2>/dev/null); then
    echo "Error: Invalid JSON configuration" >&2
    exit 1
fi

# Run containers in parallel (with job limit)
ACTIVE_JOBS=0
TOTAL_JOBS=0

while IFS= read -r job; do
    NAME=$(echo "$job" | jq -r '.name // "job"')
    IMAGE=$(echo "$job" | jq -r '.image // "alpine"')
    COMMAND=$(echo "$job" | jq -r '.command // "echo ok"')

    TOTAL_JOBS=$((TOTAL_JOBS + 1))

    # Wait if at max parallel jobs
    while [[ $ACTIVE_JOBS -ge $PARALLEL_JOBS ]]; do
        for i in "${!PIDS[@]}"; do
            if ! kill -0 "${PIDS[$i]}" 2>/dev/null; then
                wait "${PIDS[$i]}" || true
                unset 'PIDS[$i]'
                ACTIVE_JOBS=$((ACTIVE_JOBS - 1))
                break
            fi
        done
        sleep 0.1
    done

    # Start job in background
    run_container "$NAME" "$IMAGE" "$COMMAND" &
    local pid=$!
    PIDS+=($pid)
    PID_NAMES[$pid]="$NAME"
    ACTIVE_JOBS=$((ACTIVE_JOBS + 1))
done <<< "$JOBS"

# Wait for all remaining jobs
for pid in "${PIDS[@]}"; do
    wait $pid 2>/dev/null || true
done

# Summary
echo "" >&2
echo "=== Parallel Execution Summary ===" >&2
echo "Total jobs: $TOTAL_JOBS" >&2
echo "Failed: $FAILED" >&2
echo "Artifacts: ${CACHE_ARTIFACTS}" >&2

# Prune if requested
if [[ "${SHOULD_PRUNE:-false}" == "true" ]]; then
    echo ">>> Pruning unused Docker images..." >&2
    docker image prune -a --force --filter "until=72h" || true
fi

exit $FAILED
