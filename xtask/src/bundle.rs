//! `cargo xtask bundle`: build the release app and wrap it as `dist/LogoCraft.app` (macOS), with the
//! committed app icon `assets/app-icon/logocraft.icns` (regenerate it with `packaging/icons.sh`).

use std::path::Path;
use std::process::Command;

pub fn run(root: &Path) -> Result<(), String> {
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(root)
        .args(["build", "--release", "-p", "logocraft", "-p", "logocraft-cli"])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("release build failed".into());
    }
    let target = std::env::var("CARGO_TARGET_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| root.join("target"));
    let app = root.join("dist/LogoCraft.app/Contents");
    let _ = std::fs::remove_dir_all(root.join("dist/LogoCraft.app"));
    std::fs::create_dir_all(app.join("MacOS")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(app.join("Resources")).map_err(|e| e.to_string())?;
    std::fs::copy(target.join("release/logocraft"), app.join("MacOS/LogoCraft")).map_err(|e| format!("copy app: {e}"))?;
    std::fs::copy(target.join("release/logocraft-cli"), app.join("MacOS/logocraft-cli")).map_err(|e| format!("copy cli: {e}"))?;
    std::fs::copy(root.join("assets/app-icon/logocraft.icns"), app.join("Resources/LogoCraft.icns")).map_err(|e| format!("copy icon: {e}"))?;
    let sha = std::env::var("LOGOCRAFT_BUILD_SHA").unwrap_or_else(|_| "unknown".into());
    std::fs::write(app.join("Info.plist"), info_plist(env!("CARGO_PKG_VERSION"), &sha)).map_err(|e| e.to_string())?;
    // Ad-hoc sign the bundle: Apple silicon refuses an unsigned app copied off the internet as
    // "damaged". Signed this way (no Developer ID) macOS asks once instead (right-click › Open).
    if cfg!(target_os = "macos") {
        let ok = Command::new("codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(root.join("dist/LogoCraft.app"))
            .status()
            .map_err(|e| format!("codesign: {e}"))?
            .success();
        if !ok {
            return Err("ad-hoc signing (codesign --sign -) failed".into());
        }
    }
    println!("built {}", root.join("dist/LogoCraft.app").display());
    Ok(())
}

/// The release `Info.plist` template, which `packaging/macos/package.sh` fills in the same way: the
/// development bundle declares the same document types (Finder opens them with the app).
const INFO_PLIST: &str = include_str!("../../packaging/macos/Info.plist.in");

/// The bundle's `Info.plist`: [`INFO_PLIST`] for `version` and the build commit `sha`.
fn info_plist(version: &str, sha: &str) -> String {
    let short = version.split('-').next().unwrap_or(version);
    INFO_PLIST.replace("@VERSION@", version).replace("@SHORT_VERSION@", short).replace("@BUILD_SHA@", sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// File types and wording a raster image editor's packaging carries (lowercase).
    const RASTER_EDITOR: &[&str] = &["pcraft", "psd", "psb", "qoi", "photo", "rastergraphics", "image editor"];

    fn text_files(dir: &Path, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                text_files(&p, out);
            } else if let Ok(t) = std::fs::read_to_string(&p) {
                out.push((p.display().to_string(), t.to_ascii_lowercase()));
            }
        }
    }

    #[test]
    fn packaging_describes_a_vector_app() {
        let mut files = vec![];
        text_files(&crate::root().join("packaging"), &mut files);
        assert!(files.len() > 5, "packaging files found");
        for (path, text) in &files {
            for w in RASTER_EDITOR {
                assert!(!text.contains(w), "{path} mentions `{w}`");
            }
        }
        let read = |p: &str| std::fs::read_to_string(crate::root().join(p)).unwrap();
        let plist = read("packaging/macos/Info.plist.in");
        assert!(
            plist.contains("<array><string>logocraft</string><string>vectorcraft</string><string>drawcraft</string></array>"),
            "native format extensions"
        );
    }

    #[test]
    fn dev_bundle_plist_is_the_release_one() {
        let p = info_plist("1.2.3-rc.1", "abc123");
        assert!(p.contains("<string>1.2.3</string>") && p.contains("<string>1.2.3-rc.1</string>"), "{p}");
        assert!(p.contains("<string>abc123</string>"));
        assert!(!p.contains('@'), "every placeholder filled");
        assert!(p.contains("<key>CFBundleDocumentTypes</key>") && p.contains("<string>pdf</string>"), "Finder opens files with it");
    }
}
