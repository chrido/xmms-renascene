# Bundled fonts

This directory vendors the Arimo font family for deterministic software text rendering
across GTK, egui, CI, and Android builds.

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
