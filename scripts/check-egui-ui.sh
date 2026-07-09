#!/usr/bin/env bash
set -euo pipefail

cargo check --no-default-features --features egui-ui
cargo test --no-default-features --features egui-ui --lib

# Guard against accidentally pulling the GTK/Cairo UI stack into egui-only builds.
if cargo tree --no-default-features --features egui-ui | grep -E '(^|[[:space:]])(gtk4|gdk4|gio|cairo-rs|cairo-sys-rs) v' >/tmp/xmms-egui-gtk-tree.txt; then
	cat /tmp/xmms-egui-gtk-tree.txt
	echo "egui-ui build unexpectedly depends on GTK/GDK/GIO/Cairo" >&2
	exit 1
fi
