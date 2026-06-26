# egui Parity Backlog

## Purpose

This file tracks what is still missing or incomplete in the `egui` frontend when compared with the GTK frontend. The goal is to make the egui UI behave like the GTK/XMMS-compatible UI, not merely to show rough placeholders.

## Audit method

The comparison was made by reading the GTK implementation in `src/ui.rs` and the current egui implementation in `src/ui/egui/`.

GTK reference areas checked:

- Main menu popover: `build_main_menu_popover`
- Main player window, hit testing, push/toggle/slider behavior, keyboard shortcuts, wheel behavior
- Playlist window/panel, rows, footer controls, bottom menu buttons, context menu, sort popover
- Equalizer window/panel, controls, sliders, presets popover, preset dialogs/file actions
- Preferences window and all pages
- Prompt windows: Open Location and Jump to Time
- Skin Browser window
- Skin Editor window
- File Info dialog
- File/directory dialogs and drag/drop handling
- Keyboard shortcuts and panel focus behavior
- Live screenshot comparison tooling

Current egui areas checked:

- `src/ui/egui/app.rs`
- `src/ui/egui/main_player.rs`
- `src/ui/egui/playlist.rs`
- `src/ui/egui/equalizer.rs`
- `src/ui/egui/preferences.rs`
- `src/ui/egui/runtime.rs`
- `src/ui/egui/skin_texture.rs`
- `src/ui/egui/screenshots.rs`

Legend:

- `[x]` implemented enough for first parity milestone
- `[~]` partially implemented, visible or partly wired but not parity-complete
- `[ ]` missing from egui
- `[gtk-only]` intentionally GTK-only for now

---

## 1. Global frontend/window behavior

### Implemented / near parity

- [x] `./repo run --egui` builds an egui-capable binary and runs `--frontend egui`.
- [x] `./repo run --gtk` builds the default GTK binary and runs `--frontend gtk`.
- [x] `frontend-screenshot-diff` now defaults to live frontend capture instead of the offscreen `--screenshot` path.
- [x] Main player default live screenshot parity currently reaches zero changed pixels.
- [x] GTK remains default when no frontend is specified.
- [x] egui-only build avoids GTK UI modules.

### Missing / incomplete

- [ ] Window placement parity: GTK persists/restores player position and panel positions; egui currently opens a simple fixed undecorated viewport.
- [ ] Detached panel parity: GTK can detach playlist/equalizer into separate windows; egui currently renders only docked panels.
- [ ] Dock/undock behavior and snapping parity for playlist/equalizer.
- [ ] Multi-window state visibility persistence for preferences, prompts, skin browser, and panel windows.
- [ ] GTK-style skinned window borders/CSS for non-skin texture dialogs in egui.
- [ ] Cross-window focus semantics: GTK tracks focused main/playlist/equalizer panels and renders focused titlebar variants.
- [ ] Full keyboard shortcut parity across main, playlist, equalizer, prompts, and dialogs.
- [ ] Full mouse wheel parity across main sliders, shaded sliders, playlist rows, equalizer sliders.
- [ ] Drag/drop parity for files, directories, URLs, and playlist replacement/append modes.

---

## 2. Main player

### Implemented / near parity

- [x] Main player uses the shared Cairo skin renderer as an egui texture.
- [x] Default live GTK-vs-egui main player screenshot is pixel-identical.
- [x] Main push buttons are hit-test wired:
  - Previous
  - Play
  - Pause/toggle pause
  - Stop
  - Next
  - Eject
  - Shade
  - Menu
  - Minimize
  - Close
- [x] Main toggles are hit-test wired:
  - Shuffle
  - Repeat
  - Equalizer visibility
  - Playlist visibility
- [x] Volume and balance sliders are hit-test wired.
- [x] Position slider computes seek target from track duration or current playlist entry duration.
- [x] Pressed visual state is wired for buttons/toggles/sliders.

### Missing / incomplete

- [x] Main menu button parity. GTK opens `build_main_menu_popover`; egui now opens an egui main menu with Open Files, Open Location, Preferences, Skin Browser, Skin Editor notice, and Quit.
  - Missing Open Files
  - Missing Open Location
  - Missing Preferences via real menu item
  - Missing Skin Browser
  - Missing Skin Editor entry/GTK-only notice
  - Missing Quit
- [ ] Eject button parity. GTK opens file dialog; egui currently routes through playlist Add menu index 0 and may not produce the same UX.
- [x] Proper Open Location prompt from main menu / keyboard shortcut.
- [x] Jump to Time prompt from keyboard shortcut.
- [ ] Titlebar dragging / easy-move behavior.
- [ ] Minimize/close behavior needs platform verification outside tests.
- [ ] Time display parity:
  - elapsed/remaining mode;
  - current playback position updates;
  - stopped/paused display details;
  - large-duration behavior.
- [ ] Stream info parity:
  - bitrate text;
  - frequency text;
  - mono/stereo indicator;
  - title text and scrolling behavior.
- [ ] Visualization parity in live egui:
  - analyzer/scope display receives spectrum data;
  - falloff/peaks updates;
  - visualization mode/submode updates.
- [ ] Shaded main player parity beyond static rendering:
  - shaded position slider visibility and dragging;
  - shaded playback buttons;
  - shaded volume/balance behavior through equalizer shade strip;
  - shaded title text/time behavior.
- [ ] Mouse wheel over main sliders and position seek.
- [ ] Full keyboard shortcut parity for playback/menu/preferences/skin browser/jump/open location/no-advance.
- [ ] Stop-with-fadeout behavior parity in egui runtime.
- [ ] Pause-between-songs behavior parity in egui runtime.

---

## 3. Playlist window/panel

### Implemented / near parity

- [x] Playlist panel is rendered from the shared Cairo skin renderer as an egui texture.
- [x] Playlist docks under the main/equalizer panels in the egui viewport.
- [x] Playlist rows are rendered into the skinned panel.
- [x] Row click toggles selection.
- [x] Row double-click sets current row and starts playback.
- [x] Footer playback buttons are initially wired:
  - Previous
  - Play
  - Pause/toggle pause
  - Stop
  - Next
  - Eject
- [x] Footer scroll up/down is wired with a local egui scroll offset.
- [x] Footer duration summary is computed.

### Missing / incomplete

- [x] Playlist bottom menu popovers are missing. GTK opens menus for:
  - Add
  - Remove
  - Select
  - Misc
  - List
- [x] Current egui bottom menu buttons use simplified direct actions instead of opening the real XMMS-style popup menu.
- [x] Add menu parity:
  - Open Location prompt;
  - Open Directory dialog;
  - Open File dialog.
- [x] Remove menu parity:
  - Clear List;
  - Crop to Selection;
  - Remove Selected / Current;
  - any index-specific menu behavior.
- [x] Select menu parity:
  - Invert Selection;
  - Select None;
  - Select All.
- [x] Misc menu parity:
  - Sort submenu popover;
  - File Info dialog;
  - Options/preferences.
- [x] List menu parity:
  - Clear list;
  - Save playlist;
  - Load playlist.
- [x] Playlist sort popover missing. GTK has `build_playlist_sort_popover` with:
  - Sort List by Title;
  - Sort List by Filename;
  - Sort List by Path + Filename;
  - Sort List by Date;
  - Sort Selection by Title;
  - Sort Selection by Filename;
  - Sort Selection by Path + Filename;
  - Sort Selection by Date;
  - Randomize List;
  - Reverse List.
- [x] Playlist context menu missing. GTK right-click context offers:
  - Remove Selected;
  - Remove Dead Files;
  - Physically Delete;
  - Select All;
  - Select None;
  - Invert Selection.
- [ ] Physical delete confirmation dialog missing.
- [ ] File Info dialog from playlist missing.
- [ ] Playlist search overlay missing.
- [ ] Playlist keyboard navigation missing/incomplete:
  - arrow keys;
  - page up/down;
  - home/end;
  - delete;
  - enter/double-click activation;
  - Vim-style navigation preferences.
- [ ] Drag/reorder playlist entries missing.
- [ ] Drag/drop file/URL import parity missing.
- [ ] Scrollbar thumb behavior missing:
  - thumb drag;
  - proportional thumb geometry;
  - wheel scroll parity.
- [ ] Playlist resize handle behavior missing.
- [ ] Playlist shade mode behavior incomplete.
- [ ] Detached playlist window behavior missing.
- [ ] Playlist focused/unfocused titlebar rendering and focus switching missing.
- [ ] Current row and selected row behavior needs parity audit for multi-select/range behavior.
- [ ] Save/load playlist through egui is still placeholder/pending; `rfd` selections produce pending messages for some paths.
- [ ] Footer time fields are placeholder in egui (`"   "`, `"  "`) rather than live elapsed/remaining split.

---

## 4. Equalizer window/panel

### Implemented / near parity

- [x] Equalizer panel is rendered from the shared Cairo skin renderer as an egui texture.
- [x] Equalizer docks under the main panel.
- [x] On and Auto toggles are wired.
- [x] Preamp slider is wired.
- [x] Ten band sliders are wired.
- [x] Pressed state is wired for controls/sliders.
- [x] Equalizer state dispatches backend equalizer updates through app effects.

### Missing / incomplete

- [ ] Presets button does not open the GTK-equivalent nested presets popover. It currently only records a pending message.
- [ ] Equalizer presets popover missing. GTK has `build_equalizer_presets_popover` with nested sections:
  - Load;
  - Import;
  - Save;
  - Delete;
  - Configure Equalizer.
- [ ] Load preset actions missing:
  - named preset list dialog;
  - auto-load preset list dialog;
  - default preset;
  - zero preset;
  - load from XMMS preset file;
  - load from WinAMP EQF file;
  - built-in Winamp original presets submenu;
  - user preset submenu.
- [ ] Import WinAMP presets missing.
- [ ] Save preset actions missing:
  - save named preset;
  - save auto-load preset using current playlist basename;
  - save default preset;
  - save to XMMS preset file;
  - save to WinAMP EQF file.
- [ ] Delete preset actions missing:
  - delete named preset list with checkboxes;
  - delete auto-preset list with checkboxes.
- [ ] Equalizer configure dialog missing:
  - directory preset file;
  - file preset extension.
- [ ] Equalizer file dialogs are placeholder/pending for several actions.
- [ ] Shaded equalizer mode interaction incomplete:
  - shaded volume slider;
  - shaded balance slider;
  - shaded controls;
  - shaded panel titlebar hit behavior.
- [ ] Detached equalizer window behavior missing.
- [ ] Equalizer titlebar shade/close behavior missing/incomplete for egui panel.
- [ ] Equalizer graph interaction/parity needs manual audit.
- [ ] Equalizer keyboard focus/arrow adjustment behavior missing.
- [ ] Auto-preset load-on-track behavior parity needs verification in egui.

---

## 5. Preferences window

### Implemented / partial

- [~] egui has a preferences window with pages:
  - Options;
  - Audio;
  - Playlist;
  - Visualization;
  - Titles.
- [~] Some config mutations are wired directly.
- [~] Title format preview is present.

### Missing / incomplete

- [ ] Page naming/order differs from GTK. GTK notebook pages are:
  - Audio I/O Plugins;
  - Visualization Plugins;
  - Options;
  - Fonts;
  - Title.
- [ ] Reset to Defaults button missing.
- [ ] GTK-sized/skinned preferences window styling missing.
- [ ] Audio I/O page incomplete:
  - output device combo;
  - system/default output selection;
  - discovered GStreamer output devices;
  - Configure button behavior/notice;
  - input plugin explanatory parity.
- [ ] Options page incomplete. GTK options include:
  - Volume spin button;
  - Balance spin button;
  - Zoom level slider with read-only value text;
  - Podcast cache TTL;
  - Podcast refresh interval;
  - Pause between songs time;
  - Mouse wheel volume step;
  - Repeat;
  - Shuffle;
  - No playlist advance;
  - Pause between songs;
  - Stop with fadeout;
  - Time remaining;
  - Dock playlist;
  - Dock equalizer;
  - Convert `%20` to space;
  - Convert underscore to space;
  - Show numbers in playlist;
  - Vim-style playlist navigation.
- [ ] Fonts page missing as a dedicated page:
  - playlist font family combo;
  - main window skin bitmap font explanation;
  - Open Skin Browser button.
- [ ] Visualization page incomplete. GTK has controls for:
  - visualization mode analyzer/scope/off;
  - analyzer mode;
  - analyzer style;
  - scope mode;
  - peaks toggle;
  - analyzer falloff;
  - peaks falloff;
  - WindowShade VU mode;
  - refresh rate;
  - sensitivity/enablement behavior based on selected mode.
- [ ] Title page is partial:
  - needs exact token help text parity;
  - save/apply semantics and config persistence parity.
- [ ] Preferences changes do not consistently trigger redraw/window resize/config save exactly like GTK.
- [ ] Preferences open/close visibility state persistence missing.

---

## 6. Main menu, prompts, and dialogs

### Missing / incomplete

- [ ] Main menu popover missing. GTK menu entries:
  - Open Files...
  - Open Location...
  - Preferences
  - Skin Browser
  - Skin Editor
  - Quit
- [ ] Open Location prompt window missing. GTK behavior:
  - modal prompt;
  - text entry;
  - OK/Cancel;
  - adds URI/path to playlist;
  - starts playback where appropriate;
  - tracks last open location.
- [ ] Jump to Time prompt missing. GTK behavior:
  - parse seconds and `mm:ss`;
  - seek current playback position;
  - modal OK/Cancel.
- [ ] File Info dialog missing. GTK dialog includes:
  - metadata/tag display;
  - editable fields where supported;
  - Save;
  - Remove ID3/metadata action;
  - Close;
  - local-file vs stream handling.
- [ ] Delete selected files confirmation dialog missing.
- [ ] Error/message dialog parity missing; egui mostly queues strings in `pending_messages`.
- [ ] Open path behavior missing; egui currently queues an “open path pending” message.

---

## 7. File/directory dialogs

### Implemented / partial

- [~] egui uses `rfd` for some non-GTK file dialog operations.
- [~] Add files and add directory are partly wired.

### Missing / incomplete

- [ ] Open Location is not a proper egui prompt.
- [ ] Playlist load/save implementation is incomplete/pending.
- [ ] Equalizer preset load/save/import/export is incomplete/pending.
- [ ] Skin import/export is incomplete/pending for egui.
- [ ] File dialog filters/extensions are not fully matched to GTK behavior.
- [ ] Directory recursion/add behavior needs UI parity verification.
- [ ] Error reporting from failed imports/loads/saves needs real dialogs.

---

## 8. Skin Browser

### GTK reference

GTK has `build_skin_browser_window` with:

- skinned “Skin selector” window;
- list of discovered skins;
- `default` row;
- sorted discovered skin directories/archives;
- Add button;
- Close button;
- selecting a skin immediately reloads and updates main/equalizer/playlist;
- import copied archives/directories to user skin dir;
- user/system/legacy/`SKINSDIR` search paths.

### egui status

- [ ] No egui skin browser window yet.
- [ ] No skin list/discovery UI.
- [ ] No default skin row.
- [ ] No Add/import skin workflow.
- [ ] No live skin reload from egui selection.
- [ ] No skin browser open action from preferences/main menu.
- [ ] No parity with GTK selection/reload tests.

---

## 9. Skin Editor

### Status

- [gtk-only] The first egui milestone intentionally keeps the skin editor GTK-only.

### Required egui behavior for parity UX

- [ ] Main menu should still show an entry or clear message that Skin Editor is GTK-only.
- [ ] `AppEffect::OpenSkinEditor` should produce a visible egui message/dialog, not just a queued pending message.
- [ ] Documentation should state that skin editor remains GTK-only until a later milestone.

---

## 10. Playlist and equalizer detached windows

### Missing / incomplete

- [ ] Separate egui windows for detached playlist/equalizer.
- [ ] Detached window geometry, resizing, close/shade buttons.
- [ ] Dock/undock toggle behavior and saved config updates.
- [ ] Main window resize should exclude detached panels.
- [ ] Detached panel screenshots should be captured and compared.

---

## 11. Input handling and shortcuts

### Missing / incomplete

- [ ] Global keyboard shortcuts matching GTK/XMMS:
  - playback control shortcuts;
  - playlist toggle;
  - equalizer toggle;
  - no-advance toggle;
  - preferences;
  - skin browser;
  - open files/directory/location;
  - jump to time;
  - shade/unshade selected panel.
- [ ] Playlist key handling:
  - Delete removes selected/current;
  - Ctrl+Delete crops;
  - Enter plays selected/current;
  - search typing;
  - Vim key mode;
  - page/home/end navigation.
- [ ] Equalizer key handling:
  - selecting sliders;
  - arrow adjustment;
  - active/auto toggles.
- [ ] Main/equalizer/playlist focus switching and selected panel behavior.
- [ ] Pointer drag parity for sliders: egui mostly jumps by normalized position; GTK tracks knob offset exactly.
- [ ] Pointer press/release inside/outside behavior for buttons/toggles needs parity tests.

---

## 12. Playback/runtime parity

### Implemented / partial

- [~] egui can instantiate the GStreamer backend behind `gstreamer-backend`.
- [~] egui interprets playback start/resume/pause/stop/seek/volume/balance/equalizer effects.
- [~] egui now applies playback events into `AppController` player state.

### Missing / incomplete

- [ ] Periodic timer/state update parity:
  - playback position polling;
  - UI time display updates;
  - visualization ticking;
  - pause-between-songs countdown;
  - fadeout countdown.
- [ ] EOF handling parity with repeat/no-advance/pause-between-songs.
- [ ] Stop-with-fadeout parity.
- [ ] Backend error display parity.
- [ ] Stream metadata/tag display parity.
- [ ] MPRIS parity for egui is not established; current MPRIS is tied to GTK/default runtime expectations.

---

## 13. Screenshot/parity tooling gaps

### Implemented / partial

- [x] Live screenshot diff can capture actual GTK and egui windows.
- [x] Offscreen screenshot diff mode still exists.
- [x] Named scenarios exist.

### Missing / incomplete

- [ ] Live screenshot scenarios should fail or warn when egui still uses placeholder/pending behavior.
- [ ] Add scenario-specific assertions for:
  - playlist with bottom menu popover open;
  - playlist sort popover open;
  - equalizer presets popover open;
  - preferences pages one by one;
  - open location prompt;
  - jump-to-time prompt;
  - file info dialog;
  - skin browser;
  - detached playlist/equalizer;
  - shaded main/equalizer/playlist interactions.
- [ ] Automate crop-to-window/content bounds comparison so root-window black background does not hide window-size/placement mistakes.
- [ ] Add documented accepted-diff thresholds per scenario until full pixel parity is reached.
- [ ] Add CI guard for a subset of live screenshot diffs if feasible under Xvfb.

---

## 14. Suggested implementation order

1. **Finish main player interaction parity**
   - exact slider drag offset behavior;
   - live time/position updates;
   - main menu popover;
   - open location and jump-to-time prompts.
2. **Finish playlist menus**
   - bottom menu popovers;
   - sort popover;
   - context menu;
   - load/save playlist actions;
   - file info dialog.
3. **Finish equalizer presets**
   - presets popover;
   - load/save/delete/configure dialogs;
   - WinAMP EQF import/export.
4. **Finish preferences parity**
   - match GTK page names/order and controls;
   - reset defaults;
   - visualization combos/sensitivity;
   - fonts page and skin browser button.
5. **Add skin browser**
   - discover/select/import/reload skins.
6. **Detached panels and input parity**
   - separate windows;
   - shortcuts;
   - focus and mouse wheel behavior.
7. **Runtime polish**
   - timers, EOF, fadeout, metadata, visualization.

---

## 15. Current high-priority missing items

- [x] Main menu popover.
- [x] Open Location prompt.
- [x] Jump to Time prompt.
- [x] Playlist bottom menu popovers.
- [x] Playlist sort popover.
- [x] Playlist context menu.
- [ ] File Info dialog.
- [ ] Equalizer presets popover.
- [ ] Equalizer preset load/save/delete/configure dialogs.
- [ ] Preferences full control parity.
- [ ] Skin Browser.
- [ ] Detached playlist/equalizer windows.
- [ ] Live playback timer/visualization updates.
