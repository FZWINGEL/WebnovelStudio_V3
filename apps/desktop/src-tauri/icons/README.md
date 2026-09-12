# Windows application icon source

`icon.svg` is original vector geometry authored for this repository on 5 September 2026. `icon.ico` was generated from that source with the locked Tauri CLI 2.11.4 and is the current Windows application icon.

To regenerate, run the Tauri `icon` command against `src-tauri/icons/icon.svg` with an output directory under the repository's ignored `.local/`, then copy `icon.ico` into this directory. Keep unused platform outputs in `.local/`.
