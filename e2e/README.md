# Python E2E tests

This directory contains Python-based GUI end-to-end tests for XMMS Renascene.

The tests are intentionally black-box: they build/start the real application, find its X11 window, and drive it by coordinate clicks. Coordinate-based tests are useful for XMMS skin parity because many controls are bitmap regions rather than native toolkit widgets.

## Requirements

Install the system tools used by the GTK/egui smoke tests. On Debian/Ubuntu:

```bash
sudo apt-get install -y \
  build-essential \
  dbus \
  dbus-x11 \
  ffmpeg \
  imagemagick \
  libasound2-dev \
  libcairo2-dev \
  libgdk-pixbuf-2.0-dev \
  libglib2.0-dev \
  libgstreamer-plugins-base1.0-dev \
  libgstreamer1.0-dev \
  libgtk-4-dev \
  libxkbcommon-x11-0 \
  pkg-config \
  python3 \
  python3-venv \
  x11-apps \
  x11-utils \
  xauth \
  xdotool \
  xvfb
```

The screenshot helper prefers ImageMagick's `import` command for PNG output. If `import` is not installed but `xwd` is available, pressed-button screenshots are saved as `.xwd` files instead.

Run the tests locally:

```bash
./repo pye2e
```

`./repo pye2e` creates/updates `e2e/.venv` from `e2e/requirements.txt`, checks common local E2E tools, and runs pytest under `xvfb-run -a` by default when `xvfb-run` is installed. This keeps GNOME/Wayland sessions from showing screen-sharing prompts or being controlled by the tests. Set `XMMS_E2E_VENV_DIR` to use a different virtualenv path. Extra arguments are passed to pytest, for example:

```bash
./repo pye2e -k gtk
./repo pye2e -k mpris
```

Set `XMMS_E2E_USE_XVFB=0` to disable the automatic Xvfb wrapper and run against the current `DISPLAY`. This is useful for debugging but may trigger GNOME screen-sharing prompts. Set `XMMS_E2E_FORCE_XVFB=1` to require Xvfb and fail if `xvfb-run` is unavailable. Override Xvfb's server args with `XMMS_E2E_XVFB_SERVER_ARGS`, defaulting to `-screen 0 1024x768x24`.

## Docker/X server image

For machines or CI jobs without a local X server, build and run the Docker image. It contains Rust, GTK/GStreamer/egui X11 runtime dependencies, Xvfb, `xdotool`, ImageMagick `import`, `xwd`, and `ffmpeg`:

```bash
./repo pye2e-docker
```

`./repo pye2e-docker` builds the image when needed, starts the container with the repository mounted at `/workspace`, mounts `./testoutput` at `/testoutput`, and uses the container entrypoint to start Xvfb automatically when `DISPLAY` is not already reachable. It then runs `./repo pye2e` inside the container. Extra arguments are passed through to pytest.

Equivalent raw Docker commands:

```bash
docker build -f e2e/Dockerfile -t xmms-renascene-pye2e .
mkdir -p testoutput
docker run --rm -v "$PWD:/workspace" -v "$PWD/testoutput:/testoutput" -e XMMS_E2E_SCREENSHOT_DIR=/testoutput xmms-renascene-pye2e ./repo pye2e
```

Set `XMMS_E2E_DOCKER_IMAGE` to override the image tag or `XMMS_E2E_DOCKER_SKIP_BUILD=1` to reuse an existing image without rebuilding. The Docker runner uses `/tmp/xmms-renascene-pye2e-venv` for its Python virtualenv so it does not accidentally reuse a host-created `e2e/.venv` with incompatible Python shared libraries.

If `DISPLAY` is not set, or `xdotool` is unavailable, the tests skip with an explanatory message. Screenshot-specific tests also skip when neither ImageMagick `import` nor `xwd` is available.

Pressed-button screenshots are written to `testoutput` by default. Override that location with `XMMS_E2E_SCREENSHOT_DIR`. Each test invocation gets its own sanitized folder name, including pytest parameter text when present, and screenshots are numbered in capture order. Player button tests are parameterized over `gtk` and `egui` and capture before, pressed, and after states, for example `test_gui_main_button_pressed_screenshot_gtk_pause/1.png`, `2.png`, and `3.png`. Full-control tests are also parameterized over `gtk` and `egui`; they use `ffmpeg` to synthesize temporary WAV tracks, click the player transport/toggle/slider controls plus equalizer and playlist controls, and assert the application's console log contains the corresponding command/action entries. Additional egui-only button-event tests click every individual player push/toggle button, equalizer control/title button, and playlist menu/footer/title button and assert the exact emitted egui console/store event for each case. Zoom tests start both `gtk` and `egui` at `--scale-factor` 1.0, 1.5, and 2.0, assert that the actual X11 window geometry matches the requested zoom, and click main/equalizer/playlist controls using the dynamically calculated skin-coordinate scale. GTK-only tests remain for GTK-specific Preferences/menu behavior and socket smoke coverage. After each test, numbered PNG screenshots in that test folder are encoded to `screenshots.mp4` with `ffmpeg`.

## Build behavior

By default the test session builds one binary with both GTK and egui frontends enabled:

```bash
cargo build --manifest-path Cargo.toml --features egui-ui --quiet
```

Set `XMMS_E2E_SKIP_BUILD=1` to reuse an existing `target/debug/xmms-rs` binary.
