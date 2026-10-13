//! `cargo xtask icons`: render every app icon from the source picture
//! `assets/app-icon/source/logocraft-source.png`, itself rendered by LogoCraft from the vector
//! master `source/logocraft-icon.svg` (see `assets/app-icon/README.md`).
//!
//! The picture is cropped to a square, scaled to a 1024 px tile and given the rounded corners of a
//! 512 tile with `rx = 112`. macOS icons sit on Apple's grid: the tile is 824/1024 of the canvas,
//! centred on a transparent margin. Outputs (committed, so builds never need this command):
//!
//! - `logocraft-1024.png`: the full tile.
//! - `logocraft-macos-512.png`: the runtime window and Dock icon on macOS.
//! - `hicolor/{128,256}x…/apps/<id>.png`: the in-app mark (128) and the runtime icon elsewhere (256).
//! - `logocraft.icns`: the bundle icon, PNG entries from 16 to 1024 px.

use std::path::Path;

use image::imageops::FilterType;
use image::{ImageFormat, Rgba, RgbaImage};

const ID: &str = "io.github.alaminador.logocraft";
/// The master tile's size and its corner radius (112 in a 512 tile).
const TILE: u32 = 1024;
const RADIUS: f64 = 224.0;
/// The tile's share of a macOS icon canvas.
const MAC_TILE: f64 = 824.0 / 1024.0;

pub fn run(root: &Path) -> Result<(), String> {
    let dir = root.join("assets/app-icon");
    let source = image::open(dir.join("source/logocraft-source.png")).map_err(|e| format!("source picture: {e}"))?.to_rgba8();
    let tile = master(&source);
    save(&tile, &dir.join("logocraft-1024.png"))?;
    save(&mac(&tile, 512), &dir.join("logocraft-macos-512.png"))?;
    for s in [128, 256] {
        let d = dir.join(format!("hicolor/{s}x{s}/apps"));
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        save(&scaled(&tile, s), &d.join(format!("{ID}.png")))?;
    }
    std::fs::write(dir.join("logocraft.icns"), icns(&tile)?).map_err(|e| e.to_string())?;
    println!("icons written to {}", dir.display());
    Ok(())
}

/// The square, rounded 1024 px tile of `src`.
fn master(src: &RgbaImage) -> RgbaImage {
    let side = src.width().min(src.height());
    let (x, y) = ((src.width() - side) / 2, (src.height() - side) / 2);
    let square = image::imageops::crop_imm(src, x, y, side, side).to_image();
    let mut tile = image::imageops::resize(&square, TILE, TILE, FilterType::Lanczos3);
    let r = RADIUS * f64::from(tile.width()) / f64::from(TILE);
    round_corners(&mut tile, r);
    tile
}

/// Fade the pixels outside a rounded rectangle of radius `r` to transparent, anti-aliased by the
/// distance from each pixel centre to the corner arc.
fn round_corners(img: &mut RgbaImage, r: f64) {
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    for (x, y, px) in img.enumerate_pixels_mut() {
        let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let dx = if cx < r {
            r - cx
        } else if cx > w - r {
            cx - (w - r)
        } else {
            0.0
        };
        let dy = if cy < r {
            r - cy
        } else if cy > h - r {
            cy - (h - r)
        } else {
            0.0
        };
        if dx > 0.0 && dy > 0.0 {
            let coverage = (r - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
            px.0[3] = (f64::from(px.0[3]) * coverage).round() as u8;
        }
    }
}

fn scaled(tile: &RgbaImage, size: u32) -> RgbaImage {
    image::imageops::resize(tile, size, size, FilterType::Lanczos3)
}

/// The tile on a transparent `size` px macOS canvas.
fn mac(tile: &RgbaImage, size: u32) -> RgbaImage {
    let inner = ((f64::from(size) * MAC_TILE).round() as u32).max(1);
    let mut canvas = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    let at = i64::from((size - inner) / 2);
    image::imageops::overlay(&mut canvas, &scaled(tile, inner), at, at);
    canvas
}

fn png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

fn save(img: &RgbaImage, path: &Path) -> Result<(), String> {
    std::fs::write(path, png(img)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// An `.icns` of PNG entries for every size macOS asks for (16 to 512 at 1× and 2×).
fn icns(tile: &RgbaImage) -> Result<Vec<u8>, String> {
    let entries: [(&[u8; 4], u32); 10] = [
        (b"icp4", 16),
        (b"ic11", 32),
        (b"icp5", 32),
        (b"ic12", 64),
        (b"ic07", 128),
        (b"ic13", 256),
        (b"ic08", 256),
        (b"ic14", 512),
        (b"ic09", 512),
        (b"ic10", 1024),
    ];
    let mut body = Vec::new();
    for (kind, size) in entries {
        let data = png(&mac(tile, size))?;
        body.extend_from_slice(kind);
        body.extend_from_slice(&u32::try_from(data.len() + 8).map_err(|_| "icon too large")?.to_be_bytes());
        body.extend_from_slice(&data);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&u32::try_from(body.len() + 8).map_err(|_| "icns too large")?.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_the_middle_is_kept() {
        let src = RgbaImage::from_pixel(300, 200, Rgba([200, 60, 50, 255]));
        let tile = master(&src);
        assert_eq!(tile.dimensions(), (TILE, TILE));
        assert_eq!(tile.get_pixel(0, 0).0[3], 0, "corner cut");
        assert_eq!(tile.get_pixel(TILE - 1, TILE - 1).0[3], 0);
        assert_eq!(tile.get_pixel(TILE / 2, 0).0[3], 255, "edge midpoints stay");
        assert_eq!(*tile.get_pixel(TILE / 2, TILE / 2), Rgba([200, 60, 50, 255]));
        let m = mac(&tile, 512);
        assert_eq!(m.get_pixel(256, 20).0[3], 0, "Apple's margin");
        assert_eq!(m.get_pixel(256, 256).0[3], 255);
    }

    #[test]
    fn the_icns_lists_every_size_as_png() {
        let tile = master(&RgbaImage::from_pixel(64, 64, Rgba([1, 2, 3, 255])));
        let b = icns(&tile).unwrap();
        assert_eq!(&b[..4], b"icns");
        assert_eq!(u32::from_be_bytes([b[4], b[5], b[6], b[7]]) as usize, b.len());
        let mut at = 8;
        let mut kinds = vec![];
        while at < b.len() {
            let len = u32::from_be_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
            assert_eq!(&b[at + 8..at + 12], b"\x89PNG");
            kinds.push(String::from_utf8_lossy(&b[at..at + 4]).to_string());
            at += len;
        }
        assert_eq!(at, b.len());
        assert_eq!(kinds.len(), 10);
        assert!(kinds.contains(&"ic10".to_string()));
    }
}
