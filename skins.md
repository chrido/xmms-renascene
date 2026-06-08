# Skin loader todo list

Each task is complete only when the behavior is covered by automated tests.

1. [ ] Wire selected skins into the live UI.
   - Load the configured skin instead of always using the bundled default skin.
   - Make skin browser selection and reload actually replace the active skin and redraw the main, playlist, and equalizer windows.
   - Add tests proving skin selection changes rendered output and reload re-reads the selected skin.

2. [ ] Support starting with a specific skin from the command line.
   - Implement startup behavior for commands such as `xmms --skin base-2.9.1.wsz`.
   - Ensure the selected skin path is applied during initial UI construction, not just stored in config.
   - Add tests for startup with a directory skin and a `.wsz` skin archive.

3. [ ] Add screenshot functionality for the player with a skin.
   - Provide a way to render the player using a specified skin and write an image file.
   - Support at least the main window first; include playlist and equalizer screenshots if the UI state requests those windows.
   - Add tests that generate screenshots with a custom test skin and verify dimensions and representative pixels.

4. [ ] Preserve default pixmap fallbacks for partial skins.
   - Match XMMS behavior by starting from bundled defaults and overlaying only the pixmaps present in the selected skin.
   - Keep existing balance-from-volume and legacy `numbers.bmp` fallback behavior.
   - Add tests for partial directory and archive skins where missing pixmaps still render from defaults.

5. [ ] Search skin directories recursively.
   - Match the original `find_file_recursively` behavior for directory skins.
   - Prefer files in the selected directory before descending into subdirectories.
   - Add tests for nested skin files and case-insensitive filename matching.

6. [ ] Implement `region.txt` skin masks.
   - Parse `Normal`, `WindowShade`, `Equalizer`, and `EqualizerWS` mask definitions.
   - Apply masks for normal, double-size, and shaded windows where supported by the GTK port.
   - Add parser tests and UI/render tests covering valid, missing, and malformed mask data.

7. [ ] Improve `pledit.txt` playlist color compatibility.
   - Parse the original `[text]` INI section and keys case-insensitively.
   - Accept optional `#` prefixes and short hex strings the same way XMMS does.
   - Add tests for full values, short values, comments, casing, and fallback defaults.

8. [ ] Align archive discovery with archive loading.
   - Ensure every archive type shown in the skin browser can actually be loaded.
   - Add `.tar` and `.tbz2` discovery if supported, or remove unsupported plain `.gz` and `.bz2` entries.
   - Add tests for discovery and loading of each supported archive extension.

9. [ ] Expose skin-derived text colors.
   - Derive text foreground/background colors from the `text` pixmap like the original skin loader.
   - Use those colors anywhere the port needs `SKIN_TEXTFG` or `SKIN_TEXTBG` parity.
   - Add tests using a custom `text` image with known foreground/background pixels.
