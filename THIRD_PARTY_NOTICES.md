# Third-party notices

## Bundled font: Arimo

The repository vendors static Arimo font instances in `data/fonts/` for deterministic
text rendering.

- Upstream: <https://github.com/google/fonts/tree/main/ofl/arimo>
- Files:
  - `data/fonts/Arimo-Regular.ttf`
  - `data/fonts/Arimo-Bold.ttf`
  - `data/fonts/Arimo-Italic.ttf`
  - `data/fonts/Arimo-BoldItalic.ttf`
- License: SIL Open Font License, Version 1.1
- Full text: `data/fonts/OFL.txt`

The bundled static instances were generated from the upstream Arimo variable fonts
using `fontTools.varLib.instancer` to pin `wght=400` and `wght=700`.

## Bundled font: Droid Sans Fallback

The playlist/vector text renderer uses Droid Sans Fallback when Arimo lacks a glyph.

- Upstream: <https://github.com/aosp-mirror/platform_frameworks_base/tree/android-4.4_r1/data/fonts>
- File: `data/fonts/DroidSansFallback.ttf`
- Copyright (c) 2005-2008, The Android Open Source Project
- License: Apache License, Version 2.0
- Full upstream notice and license: `data/fonts/DroidSansFallback-NOTICE.txt`
