# LogoCraft app icon

**Mark:** a white pen nib outline, fading from the tip, on a dark plum tile.

**Source:** `source/logocraft-source.png` (1000 × 1000), the icon picture made by the repository owner
and supplied as a PNG. Licence: see `LICENSE.txt`.

**Tile:** the picture cropped square, scaled to 1024 px and given the corners of a 512 tile with
`rx = 112`. The macOS files add Apple's transparent margin (tile = 824/1024 of the canvas).

## Files

All rendered by `cargo xtask icons` from the source; commit them, so builds never need the command.

| File | What | Used by |
|---|---|---|
| `logocraft-1024.png` | full tile, 1024 px | docs, store listings |
| `logocraft-macos-512.png` | 512 px with Apple's margin | the window and Dock icon on macOS (`apps/logocraft/src/main.rs`) |
| `logocraft.icns` | 16 to 1024 px with Apple's margin | the app bundle (`cargo xtask bundle`, `packaging/macos/package.sh`) |
| `hicolor/128x128/apps/io.github.alaminador.logocraft.png` | 128 px tile | the in-app mark (`crates/ui-egui/src/brand.rs`) |
| `hicolor/256x256/apps/io.github.alaminador.logocraft.png` | 256 px tile | the window icon of development builds off macOS |
