# Python E2E tests

This directory contains Python-based GUI end-to-end tests for XMMS Renascene.

The tests are intentionally black-box: they build/start the real application, find its X11 window, and drive it by coordinate clicks. Coordinate-based tests are useful for XMMS skin parity because many controls are bitmap regions rather than native toolkit widgets.

## Requirements

Install the system tools used by the GTK smoke tests:

```bash
sudo apt-get install -y xvfb xdotool imagemagick
```

The screenshot helper prefers ImageMagick's `import` command for PNG output. If `import` is not installed but `xwd` is available, pressed-button screenshots are saved as `.xwd` files instead.

Create/update the local test virtualenv:

```bash
python e2e/create_venv.py
```

Run the tests under Xvfb so GTK has an X11 display:

```bash
xvfb-run -a -s "-screen 0 1024x768x24" ./repo pye2e
```

`./repo pye2e` creates `e2e/.venv` from `e2e/requirements.txt` when needed and then runs `python -m pytest e2e` from that virtualenv. Extra arguments are passed to pytest, for example:

```bash
xvfb-run -a ./repo pye2e -q -k gtk
```

## Docker/X server image

For machines or CI jobs without a local X server, build and run the Docker image. It contains Rust, GTK/GStreamer build dependencies, Xvfb, `xdotool`, ImageMagick `import`, and `xwd`:

```bash
./repo pye2e-docker -q
```

`./repo pye2e-docker` builds the image when needed, starts the container with the repository mounted at `/workspace`, and uses the container entrypoint to start Xvfb automatically when `DISPLAY` is not already reachable. It then runs `./repo pye2e` inside the container. Extra arguments are passed through to pytest.

Equivalent raw Docker commands:

```bash
docker build -f e2e/Dockerfile -t xmms-renascene-pye2e .
docker run --rm -v "$PWD:/workspace" xmms-renascene-pye2e ./repo pye2e -q
```

Set `XMMS_E2E_DOCKER_IMAGE` to override the image tag or `XMMS_E2E_DOCKER_SKIP_BUILD=1` to reuse an existing image without rebuilding.

If `DISPLAY` is not set, or `xdotool` is unavailable, the tests skip with an explanatory message. Screenshot-specific tests also skip when neither ImageMagick `import` nor `xwd` is available.

Pressed-button screenshots are written to `target/e2e-screenshots` by default. Override that location with `XMMS_E2E_SCREENSHOT_DIR`.

## Build behavior

By default the test session builds the GTK frontend with:

```bash
cargo build --manifest-path Cargo.toml --quiet
```

Set `XMMS_E2E_SKIP_BUILD=1` to reuse an existing `target/debug/xmms-rs` binary.
