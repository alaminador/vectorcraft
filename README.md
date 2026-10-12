# LogoCraft

A lean vector app for designing logos, for macOS. Written in Rust.

LogoCraft is based on [VectorCraft](https://github.com/storytold/vectorcraft) by the ArtCraft Team
and contributors. It keeps the drawing, type and export tools a logo needs and drops the rest
(3D, CAD, print production, web slicing, symbol sprays, liquify, perspective grids and so on).

## Status

Early. The code builds and its test suite passes on Linux CI machines; the macOS app is built by
GitHub Actions (see below) but is not signed or notarized yet.

What it does:
- Pen, curvature, pencil, shapes, Shape Builder, Pathfinder, width tool, gradients, Image Trace.
- Type with Illustrator-style Character and Paragraph controls: kerning Auto / Optical / value,
  tracking, small caps, superscript and subscript, No Break, Justification… and Hyphenation…
- Opens and saves its own `.logocraft` files, and opens VectorCraft `.vectorcraft` and older
  `.drawcraft` files. Opens SVG, PDF, AI, EPS and Affinity files; places PNG, JPEG and WebP.
- Exports SVG, PDF, EPS, PNG, JPEG, WebP and PNG-8, with Save for Web (Cmd+Shift+S), Export for
  Screens and the Asset Export panel.

## Get the app

Every push to `main` and to `claude/*` branches builds `LogoCraft.app` in GitHub Actions: open the
repository's **Actions** tab, pick the latest **macOS app** run, and download the `LogoCraft-mac`
artifact. Unzip it and move `LogoCraft.app` to Applications. It is unsigned, so open it the first
time with right-click › Open (or run `xattr -dr com.apple.quarantine /Applications/LogoCraft.app`).

Or build it on a Mac:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # once: install Rust
cargo xtask bundle                                               # builds dist/LogoCraft.app
```

`cargo run --release -p logocraft` starts it without bundling.

## Development

`cargo xtask ci` runs the checks every change must pass: format, clippy (with the no-panic lints),
the tests, asset attribution, vendor-name and layering checks. See [`CLAUDE.md`](CLAUDE.md) for the
rules (clean-room behaviour from public documentation only, no panics in shipped code, every asset
attributed in [`ASSETS.md`](ASSETS.md)).

## License

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
Required notices are in [NOTICE](NOTICE). LogoCraft is not affiliated with or endorsed by the
ArtCraft Team.
