//! Project links (Home screen and About): the source repository. Icons are Lucide's (ISC).

use egui::Ui;

use crate::theme::Tokens;
use crate::{LogocraftApp, icons};

/// One link row: icon, label, opens the command's URL.
fn link(app: &mut LogocraftApp, ui: &mut Ui, icon: &str, label: &str, cmd: &str, url: &str) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        icons::icon(ui, icon, 16.0, t.icon);
        let r = ui.add(egui::Button::new(egui::RichText::new(label).color(t.accent).size(13.0)).frame(false)).on_hover_text(url);
        if r.clicked() {
            app.open_link(cmd);
        }
    });
}

/// The GitHub link.
pub fn links(app: &mut LogocraftApp, ui: &mut Ui) {
    let l = app.session.execute("help.links", &serde_json::json!({})).unwrap_or_default();
    let github = l["github"].as_str().unwrap_or("").to_string();
    link(app, ui, "git-branch", tl!("Source code on GitHub"), "help.github", &github);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_github_link_opens_the_repository() {
        use std::sync::{Arc, Mutex};
        let opened = Arc::new(Mutex::new(vec![]));
        let o = opened.clone();
        let services = crate::Services { open_url: Some(Box::new(move |u: &str| o.lock().unwrap().push(u.to_string()))), ..Default::default() };
        let mut app = LogocraftApp::new(logocraft_engine::Session::new(), services);
        crate::menus::invoke(&mut app, "help.github", serde_json::json!({}));
        assert_eq!(*opened.lock().unwrap(), ["https://github.com/alaminador/vectorcraft"]);
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| links(&mut app, ui));
        out.textures_delta.clear();
    }
}
