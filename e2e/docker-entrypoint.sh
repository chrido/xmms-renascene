#!/usr/bin/env bash
set -euo pipefail

if [[ ! -d /workspace ]]; then
  echo "Expected the repository to be mounted at /workspace" >&2
  exit 2
fi

if [[ ! -e /workspace/repo ]]; then
  echo "Expected /workspace/repo to exist. Run with: docker run --rm -v \"\$PWD:/workspace\" <image>" >&2
  exit 2
fi

xvfb_pid=""
cleanup() {
  if [[ -n "${xvfb_pid}" ]] && kill -0 "${xvfb_pid}" 2>/dev/null; then
    kill "${xvfb_pid}" 2>/dev/null || true
    wait "${xvfb_pid}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

if [[ -z "${DISPLAY:-}" ]]; then
  export DISPLAY=:99
fi

if ! xdpyinfo -display "${DISPLAY}" >/dev/null 2>&1; then
  screen="${XVFB_SCREEN:-1024x768x24}"
  echo "Starting Xvfb on ${DISPLAY} with screen ${screen}"
  Xvfb "${DISPLAY}" -screen 0 "${screen}" -ac +extension RANDR >/tmp/xvfb.log 2>&1 &
  xvfb_pid="$!"

  for _ in $(seq 1 50); do
    if xdpyinfo -display "${DISPLAY}" >/dev/null 2>&1; then
      break
    fi
    sleep 0.1
  done

  if ! xdpyinfo -display "${DISPLAY}" >/dev/null 2>&1; then
    echo "Xvfb did not start successfully; log follows:" >&2
    cat /tmp/xvfb.log >&2 || true
    exit 1
  fi
fi

export GDK_BACKEND="${GDK_BACKEND:-x11}"
export GDK_DISABLE="${GDK_DISABLE:-gl}"
export GSK_RENDERER="${GSK_RENDERER:-cairo}"
export NO_AT_BRIDGE="${NO_AT_BRIDGE:-1}"

cd /workspace
exec "$@"
