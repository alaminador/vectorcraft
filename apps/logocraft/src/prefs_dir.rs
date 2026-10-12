//! Where LogoCraft keeps its settings, `ui.json`, and next to it folders of the user's own
//! (Swatches, Graphic Styles, Fonts, Data Recovery, logs). The desktop app and `logocraft-cli`
//! (which shares this file) read the same Fonts folder.

use std::path::PathBuf;

#[path = "profile.rs"]
mod profile;

/// The settings file `ui.json` of the app `name`: in a folder named `name` in ~/Library/Application
/// Support (macOS) or %APPDATA% (Windows), or `lower` in $XDG_CONFIG_HOME or ~/.config (Linux and
/// BSD).
pub fn prefs_path_for(name: &str, lower: &str) -> Option<PathBuf> {
    profile::prefs_path((name, lower))
}

/// The settings files of the apps LogoCraft was called before, newest first: read once when
/// LogoCraft has none yet, so settings survive the renames.
// The app reads them; logocraft-cli, which shares this file, doesn't.
#[allow(dead_code)]
pub fn former_prefs_paths() -> Vec<PathBuf> {
    profile::FORMER.iter().filter_map(|app| profile::prefs_path(*app)).collect()
}

/// Have the font scans read LogoCraft's own Fonts folder, next to the settings, which
/// `text.addFontFiles` copies font files into. The folder is set whether or not
/// `LOGOCRAFT_NO_PREFS` is set, as the Swatches and Graphic Styles folders are.
pub fn install_fonts() {
    if let Some(dir) = prefs_path_for("LogoCraft", "logocraft").and_then(|p| Some(p.parent()?.join("Fonts"))) {
        logocraft_text::set_app_font_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_font_scans_read_the_fonts_folder_next_to_the_settings() {
        install_fonts();
        let fonts = prefs_path_for("LogoCraft", "logocraft").and_then(|p| Some(p.parent()?.join("Fonts")));
        assert!(fonts.is_some(), "a home folder to keep the settings in");
        assert_eq!(logocraft_text::app_font_dir().map(std::path::Path::to_path_buf), fonts);
        assert!(logocraft_text::system_font_dirs().contains(&fonts.unwrap_or_default()));
    }
}
