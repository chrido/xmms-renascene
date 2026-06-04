#!/bin/sh
set -eu

cd "$(dirname "$0")"

if [ "${XMMS_EXEC_SKIP_BUILD:-}" != "1" ]; then
    if [ ! -f builddir/build.ninja ]; then
        meson setup builddir
    fi

    meson compile -C builddir
fi

if [ "${1:-}" = "screenshot" ]; then
    shift

    if [ "${XMMS_SCREENSHOT_UNDER_XVFB:-}" != "1" ]; then
        if ! command -v xvfb-run >/dev/null 2>&1; then
            echo "xvfb-run is required for ./exec.sh screenshot." >&2
            exit 127
        fi

        xvfb_server_args="${XMMS_XVFB_SERVER_ARGS:--screen 0 1024x768x24}"
        exec xvfb-run -a -s "$xvfb_server_args" \
            env -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS \
                GDK_BACKEND=x11 GSK_RENDERER=cairo NO_AT_BRIDGE=1 \
                XMMS_EXEC_SKIP_BUILD=1 \
                XMMS_SCREENSHOT_UNDER_XVFB=1 ./exec.sh screenshot "$@"
    fi

    screenshot_file="${XMMS_SCREENSHOT_FILE:-screenshot.png}"
    screenshot_delay="${XMMS_SCREENSHOT_DELAY:-3}"

    take_screenshot() {
        if command -v import >/dev/null 2>&1; then
            import -window root -screen "$screenshot_file"
        elif command -v scrot >/dev/null 2>&1; then
            scrot "$screenshot_file"
        elif command -v gnome-screenshot >/dev/null 2>&1; then
            gnome-screenshot -f "$screenshot_file"
        elif command -v grim >/dev/null 2>&1; then
            grim "$screenshot_file"
        elif command -v spectacle >/dev/null 2>&1; then
            spectacle -b -n -o "$screenshot_file"
        else
            echo "No screenshot tool found. Install ImageMagick import, scrot, gnome-screenshot, grim, or spectacle." >&2
            return 127
        fi
    }

    ./builddir/xmms "$@" &
    xmms_pid=$!

    cleanup() {
        if kill -0 "$xmms_pid" 2>/dev/null; then
            kill "$xmms_pid" 2>/dev/null || true
            wait "$xmms_pid" 2>/dev/null || true
        fi
    }
    trap cleanup EXIT INT TERM

    sleep "$screenshot_delay"

    if ! kill -0 "$xmms_pid" 2>/dev/null; then
        xmms_status=0
        wait "$xmms_pid" || xmms_status=$?
        echo "xmms exited before the screenshot could be taken." >&2
        exit "$xmms_status"
    fi

    mkdir -p "$(dirname "$screenshot_file")"
    take_screenshot
    if [ ! -s "$screenshot_file" ]; then
        echo "Screenshot command did not create $screenshot_file." >&2
        exit 1
    fi
    echo "Screenshot saved to $screenshot_file"
    exit 0
fi

exec ./builddir/xmms "$@"
