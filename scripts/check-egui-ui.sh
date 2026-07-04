#!/usr/bin/env bash
set -euo pipefail

cargo check --no-default-features --features egui-ui
cargo test --no-default-features --features egui-ui --lib

# Guard against accidentally pulling the GTK UI stack into egui-only builds.
# Note: cairo-rs currently pulls glib/glib-sys even without gtk-ui; that is
# tracked by the renderer abstraction work and is not the GTK widget stack.
if cargo tree --no-default-features --features egui-ui | grep -E '(^|[[:space:]])(gtk4|gdk4|gio) v' >/tmp/xmms-egui-gtk-tree.txt; then
  cat /tmp/xmms-egui-gtk-tree.txt
  echo "egui-ui build unexpectedly depends on GTK/GDK/GIO" >&2
  exit 1
fi
