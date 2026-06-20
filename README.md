# XMMS Renascene

A modernized version of the classic [XMMS](https://en.wikipedia.org/wiki/XMMS)
(X Multimedia System) music player, rebuilt in Rust with GTK 4 and GStreamer
while preserving Winamp 2.x skin compatibility.

## Features

- **Winamp-compatible skins** — load classic `.wsz`/`.zip` skin archives or directories
- **10-band equalizer** with preamp and real-time response curve
- **Spectrum analyzer** visualization, Milkdrop-inspired
- **Playlist editor** with drag-and-drop support
- **MPRIS 2 D-Bus interface** for media key integration
- **Unified output device picker** — switch between local and network audio devices
- **Skin browser** for switching between installed skins
- **Skin editor** for live pixel editing, cloning/saving skins, and exporting
  Winamp `.wsz` files
- Wide audio format support via GStreamer (MP3, OGG, FLAC, WAV, AAC, and more)
- Vim-like playlist navigation (j/k, p for playback, / for incremental search)

## Screenshots

### Default skin

![XMMS Renascene default skin](screenshots/screenshot_default.png)

### Classic Winamp skin

Browse this collection of classic Winamp skins and try XMMS Renascene with one
of the available skins: <https://skins.webamp.org/>

![XMMS Renascene with Winamp classic skin](screenshots/screenshot_winamp_skin.png)

## Dependencies

- GTK 4 (>= 4.6)
- GStreamer 1.x (>= 1.16) with `gstreamer-plugins-base` and `gstreamer-plugins-good`
- Rust/Cargo
- C compiler and `pkg-config` for native Rust dependencies

### Ubuntu / Debian

```sh
sudo apt install libgtk-4-dev libgstreamer1.0-dev \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    build-essential pkg-config cargo
```

## Skins

XMMS Renascene supports Winamp 2.x-compatible skins. Place skin files in:

- `~/.config/xmms/Skins/` — user skin directory
- `/usr/share/xmms/Skins/` — system skin directory

Skins can be `.wsz`, `.zip`, `.tar`, `.tar.gz`, or `.tar.bz2` archives,
or unpacked directories. You can browse classic Winamp skins at the
[Webamp Skin Museum](https://skins.webamp.org/). Use **Alt+S** to open the skin
browser.

The **Skin Editor** is available from the player menu. It opens in a separate
window, shows every skin pixmap on one canvas, exposes playlist, visualization,
and text color swatches, provides a popup custom color wheel with a 32-slot color
shelf, supports brush, spraycan, color picker, drag/pan, line, rectangle,
select/copy/cut/paste, lighten, darken, and dither tools, updates the player
live as you paint, saves edited skins into the user skin directory, and exports
Winamp-compatible `.wsz` archives.

## Keyboard Shortcuts

|Key|Action|
|---|---|
|`z`|Previous track|
|`x`|Play|
|`c`|Pause|
|`v`|Stop|
|`b`|Next track|
|`Alt+E`|Toggle playlist window|
|`Alt+G`|Toggle equalizer window|
|`Alt+S`|Open skin browser|
|`Up/Down`|Volume up/down|
|`Left/Right`|Seek backward/forward 5 seconds|

## Building and running

Run the helper script to list available commands:

```sh
./repo
```

## License

GNU General Public License v2.0 or later. See [COPYING](COPYING) for details.

## Credits

Originally written by Peter Alm, Thomas Nilsson, Olle Hallnas, and Havard
Kvalen <https://sourceforge.net/projects/xmms/>.
Modernized for GTK 4 and GStreamer by Christian Schaller
<https://gitlab.com/cschalle/xmms-renascene>.
Ported to Rust (AI-assisted).
