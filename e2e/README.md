# Python E2E tests

This directory contains Python-based GUI end-to-end tests for XMMS Renascene.

The tests are intentionally black-box: they build/start the real application, find its X11 window, and drive it by coordinate clicks. Coordinate-based tests are useful for XMMS skin parity because many controls are bitmap regions rather than native toolkit widgets.

## Requirements

Install the system tools used by the first GTK smoke test:

```bash
sudo apt-get install -y xvfb xdotool
python -m pip install pytest
```

Run the tests under Xvfb so GTK has an X11 display:

```bash
xvfb-run -a -s "-screen 0 1024x768x24" python -m pytest e2e
```

If `DISPLAY` is not set, or `xdotool` is unavailable, the tests skip with an explanatory message.

## Build behavior

By default the test session builds the GTK frontend with:

```bash
cargo build --manifest-path Cargo.toml --quiet
```

Set `XMMS_E2E_SKIP_BUILD=1` to reuse an existing `target/debug/xmms-rs` binary.
