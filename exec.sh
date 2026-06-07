#!/bin/sh
set -eu

cd "$(dirname "$0")"

export GDK_DISABLE="${XMMS_GDK_DISABLE:-gl}"
export GSK_RENDERER="${XMMS_GSK_RENDERER:-cairo}"

usage() {
    cat <<EOF
Usage: ./exec.sh [--rust] [screenshot] [app args...]

Options:
  --rust      Start the Rust version (default).
  screenshot Capture a root-window screenshot after starting the selected version.

Rust preview app args:
  --playlist / --equalizer    Show floating windows on startup.
  --dock-playlist / --undock-playlist
                              Control playlist docking on startup.
  --dock-equalizer / --undock-equalizer
                              Control equalizer docking on startup.
  --shade-main / --shade-playlist / --shade-equalizer
                              Start windows in WindowShade mode.
  --skin PATH                 Select a skin path on startup.
  --playlist-size=WIDTHxHEIGHT
                              Show the playlist and set its startup size.
EOF
}

screenshot=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --rust)
            shift
            ;;
        --c)
            echo "The C version has been removed; ./exec.sh now runs the Rust port only." >&2
            exit 2
            ;;
        screenshot)
            screenshot=1
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --)
            shift
            break
            ;;
        *)
            break
            ;;
    esac
done

rust_bin="target/debug/xmms-rs"

build_selected_app() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo is required to run the Rust version." >&2
        exit 127
    fi

    cargo build --manifest-path Cargo.toml --quiet
}

rust_args_include_gtk_mode() {
    for arg do
        case "$arg" in
            --gtk|--gtk-smoke)
                return 0
                ;;
        esac
    done

    return 1
}

start_selected_app() {
    if [ ! -x "$rust_bin" ]; then
        echo "Rust binary '$rust_bin' is missing. Run without XMMS_EXEC_SKIP_BUILD=1 first." >&2
        exit 127
    fi

    if rust_args_include_gtk_mode "$@"; then
        exec "$rust_bin" "$@"
    fi

    exec "$rust_bin" --gtk "$@"
}

start_selected_app_in_background() {
    if rust_args_include_gtk_mode "$@"; then
        "$rust_bin" "$@" &
    else
        "$rust_bin" --gtk "$@" &
    fi

    xmms_pid=$!
}

if [ "${XMMS_EXEC_SKIP_BUILD:-}" != "1" ]; then
    build_selected_app
fi

if [ "$screenshot" = "1" ]; then
    if [ "${XMMS_SCREENSHOT_UNDER_XVFB:-}" != "1" ]; then
        if ! command -v xvfb-run >/dev/null 2>&1; then
            echo "xvfb-run is required for ./exec.sh screenshot." >&2
            exit 127
        fi

        xvfb_server_args="${XMMS_XVFB_SERVER_ARGS:--screen 0 1024x768x24}"
        exec xvfb-run -a -s "$xvfb_server_args" \
            env -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS \
                GDK_BACKEND=x11 GSK_RENDERER=cairo GDK_DISABLE="$GDK_DISABLE" \
                NO_AT_BRIDGE=1 \
                XMMS_NON_UNIQUE=1 \
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

    start_selected_app_in_background "$@"

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

start_selected_app "$@"
