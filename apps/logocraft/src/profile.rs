//! Native profile paths. Isolated profiles never migrate preferences from a user's old profile.

use std::ffi::OsString;
use std::path::PathBuf;

/// The app's own settings folder name (`name` on macOS and Windows, `lower` elsewhere).
pub const CURRENT: (&str, &str) = ("LogoCraft", "logocraft");
/// Former names whose settings are read once when LogoCraft has none yet: VectorCraft (which
/// LogoCraft is based on), then DrawCraft (VectorCraft's own former name).
#[allow(dead_code)] // Read by the app, not by logocraft-cli (which shares this file).
pub const FORMER: &[(&str, &str)] = &[("VectorCraft", "vectorcraft"), ("DrawCraft", "drawcraft")];

/// `ui.json` of the app called `app` (see [`CURRENT`] and [`FORMER`]).
pub fn prefs_path(app: (&str, &str)) -> Option<PathBuf> {
    prefs_path_with(std::env::consts::OS, app, |key| std::env::var_os(key))
}

fn prefs_path_with(os: &str, (name, lower): (&str, &str), env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let former = (name, lower) != CURRENT;
    if let Some(root) = env("LOGOCRAFT_CONFIG_DIR") {
        // Never fall back to a former app's real profile when the isolated file does not exist.
        return (!former && !root.is_empty()).then(|| PathBuf::from(root).join("ui.json"));
    }
    let base = match os {
        "macos" => env("HOME").map(|p| PathBuf::from(p).join("Library/Application Support").join(name)),
        "windows" => env("APPDATA").map(|p| PathBuf::from(p).join(name)),
        _ => env("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| env("HOME").map(|p| PathBuf::from(p).join(".config"))).map(|p| p.join(lower)),
    };
    base.map(|p| p.join("ui.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTORCRAFT: (&str, &str) = ("VectorCraft", "vectorcraft");
    const DRAWCRAFT: (&str, &str) = ("DrawCraft", "drawcraft");

    fn environment(key: &str) -> Option<OsString> {
        match key {
            "HOME" => Some("/user".into()),
            "APPDATA" => Some("/roaming".into()),
            "XDG_CONFIG_HOME" => Some("/xdg-config".into()),
            _ => None,
        }
    }

    #[test]
    fn isolated_profile_has_no_former_fallback_on_any_platform() {
        for os in ["macos", "windows", "linux"] {
            let env = |key: &str| {
                if key == "LOGOCRAFT_CONFIG_DIR" { Some("/isolated".into()) } else { environment(key) }
            };
            assert_eq!(prefs_path_with(os, CURRENT, env), Some("/isolated/ui.json".into()));
            for former in FORMER {
                assert_eq!(prefs_path_with(os, *former, env), None);
            }
        }
    }

    #[test]
    fn empty_override_does_not_fall_back_to_user_files() {
        let env = |key: &str| if key == "LOGOCRAFT_CONFIG_DIR" { Some(OsString::new()) } else { environment(key) };
        assert_eq!(prefs_path_with("macos", CURRENT, env), None);
        assert_eq!(prefs_path_with("macos", VECTORCRAFT, env), None);
    }

    #[test]
    fn unset_override_keeps_platform_paths_and_former_names() {
        assert_eq!(prefs_path_with("macos", CURRENT, environment), Some("/user/Library/Application Support/LogoCraft/ui.json".into()));
        assert_eq!(prefs_path_with("macos", VECTORCRAFT, environment), Some("/user/Library/Application Support/VectorCraft/ui.json".into()));
        assert_eq!(prefs_path_with("macos", DRAWCRAFT, environment), Some("/user/Library/Application Support/DrawCraft/ui.json".into()));
        assert_eq!(prefs_path_with("windows", CURRENT, environment), Some("/roaming/LogoCraft/ui.json".into()));
        assert_eq!(prefs_path_with("linux", CURRENT, environment), Some("/xdg-config/logocraft/ui.json".into()));
        assert_eq!(
            prefs_path_with("linux", DRAWCRAFT, |key| if key == "HOME" { environment(key) } else { None }),
            Some("/user/.config/drawcraft/ui.json".into())
        );
        assert_eq!(FORMER, [VECTORCRAFT, DRAWCRAFT], "VectorCraft's settings first");
    }
}
