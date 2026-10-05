# Bundled fonts

This directory vendors the Arimo font family and Droid Sans Fallback for deterministic
software text rendering across GTK, egui, CI, and Android builds.

## Contents

- `Arimo-Regular.ttf`
- `Arimo-Bold.ttf`
- `Arimo-Italic.ttf`
- `Arimo-BoldItalic.ttf`
- `OFL.txt`

## Source

Upstream source: <https://github.com/google/fonts/tree/main/ofl/arimo>

The shipped static instances were generated from the upstream variable fonts:

- `Arimo[wght].ttf`
- `Arimo-Italic[wght].ttf`

using `fontTools.varLib.instancer` with `wght=400` and `wght=700`.

## License

Arimo is licensed under the SIL Open Font License, Version 1.1.
See `OFL.txt` in this directory for the full license text.

## Playlist fallback

`DroidSansFallback.ttf` is loaded lazily when Arimo lacks a character. It provides
additional coverage, including Chinese, Japanese, and Korean, for playlist/vector
text. The same per-character font selection is used for measurement and rendering.
The fallback has a single regular style; Arimo retains the requested bold/italic
style for its own glyphs. Characters missing from both fonts retain a missing-glyph
placeholder. This does not add complex-script shaping or color emoji support and
does not affect the skin bitmap marquee.

- Source: <https://github.com/aosp-mirror/platform_frameworks_base/blob/android-4.4_r1/data/fonts/DroidSansFallback.ttf>
- SHA-256: `05d71b179ef97b82cf1bb91cef290c600a510f77f39b4964359e3ef88378c79d`
- License: Apache License, Version 2.0
- Upstream copyright and license: `DroidSansFallback-NOTICE.txt`
- Upstream notice: <https://github.com/aosp-mirror/platform_frameworks_base/blob/android-4.4_r1/data/fonts/NOTICE>
