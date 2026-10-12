//! Help → the project's source repository. The command returns the URL; the frontend opens it.

use serde_json::{Value, json};

use super::*;

/// The source repository (its GitHub name predates the LogoCraft name).
pub const GITHUB_URL: &str = "https://github.com/alaminador/vectorcraft";

/// This app's source repository.
pub fn github_url() -> String {
    GITHUB_URL.to_string()
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "help.github", "LogoCraft on GitHub", ["Help"], None, "{} → {url} source code, issues and releases", always, |_, _| url(github_url())),
        cmd!(query "help.links", "Links", [], None, "{} → {github}", always, |_, _| Ok(json!({ "github": github_url() }))),
    ]
}

fn url(u: String) -> Result<Value> {
    Ok(json!({ "url": u }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let mut s = Session::new();
        assert_eq!(s.execute("help.github", &json!({})).unwrap()["url"], GITHUB_URL);
        assert_eq!(s.execute("help.links", &json!({})).unwrap()["github"], GITHUB_URL);
        assert!(s.execute("help.discord", &json!({})).is_err(), "no community links of the original project");
    }
}
