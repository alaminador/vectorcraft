//! Save for Web (Legacy), drawn headlessly and driven as an agent drives it.

use std::cell::RefCell;
use std::rc::Rc;

use logocraft_engine::Session;
use serde_json::json;

use super::tests_export::{frame, kind, set};
use super::*;
use crate::Services;

type Log = Rc<RefCell<Vec<String>>>;

/// An app with a 60×40 pt document (an ellipse), whose save panel answers inside `dir`; `web`
/// downloads instead (into the log).
fn app(dir: &std::path::Path, web: bool) -> (LogocraftApp, Log) {
    let log = Log::default();
    let (l1, l2) = (log.clone(), log.clone());
    let dir = dir.to_string_lossy().replace('\\', "/");
    let mut services = Services {
        pick_save: Some(Box::new(move |p: &crate::FilePick| Some(format!("{dir}/{}", p.name)))),
        open_url: Some(Box::new(move |u: &str| l1.borrow_mut().push(format!("open {u}")))),
        ..Default::default()
    };
    if web {
        services.download = Some(Box::new(move |name: &str, b: &[u8]| l2.borrow_mut().push(format!("download {name} {}", b.len()))));
    }
    let mut app = LogocraftApp::new(Session::new(), services);
    app.run("file.new", json!({"width": 60, "height": 40})).unwrap();
    app.run("paint.setStroke", json!({"none": true})).unwrap();
    app.run("shape.ellipse", json!({"x": 5, "y": 4, "width": 30, "height": 20})).unwrap();
    app.run("paint.setFill", json!({"color": "#cc3300"})).unwrap();
    (app, log)
}

#[test]
fn every_view_and_format_draws() {
    let dir = logocraft_testkit::temp_dir("sfw-views");
    let (mut app, _) = app(&dir, false);
    assert_eq!(crate::menus::shortcut_of("file.saveForWeb"), Some("Cmd+Shift+S"));
    assert!(crate::menus::enabled(&app, "file.saveForWeb"));
    assert_eq!((spec(save_for_web::KIND).ok_label.unwrap())(&app), "Save…");
    app.run("file.saveForWeb", json!({})).unwrap();
    assert_eq!(kind(&app), Some(save_for_web::KIND));
    let d = app.ui.dialog.as_ref().unwrap();
    assert_eq!((d.str("format"), d.str("__view"), d.str("__zoom"), d.f64("colors", 0.0)), ("gif".into(), "optimized".into(), "fit".into(), 128.0));
    // Only the optimized image shows: no view tabs.
    let text = crate::tests_labels::painted_text(&mut app, |app, ui| crate::dialogs::show(app, ui.ctx()));
    for tab in ["Original", "2-Up", "4-Up"] {
        assert!(!text.contains(tab), "{tab} in {text}");
    }
    for view in ["original", "optimized", "2up", "4up"] {
        set(&mut app, "__view", json!(view));
        for format in ["gif", "jpg", "png8", "png24"] {
            set(&mut app, "format", json!(format));
            frame(&mut app);
            assert_eq!(kind(&app), Some(save_for_web::KIND), "{view} {format}");
        }
    }
    // Fit, other speeds, a colour selected in the table, a bad value (an error in the panes).
    set(&mut app, "format", json!("gif"));
    set(&mut app, "__zoom", json!("fit"));
    set(&mut app, "__kbps", json!(1024));
    set(&mut app, "__color", json!("#cc3300"));
    frame(&mut app);
    set(&mut app, "percent", json!(0));
    frame(&mut app);
    assert_eq!(kind(&app), Some(save_for_web::KIND));
}

#[test]
fn save_remembers_and_writes_and_done_only_remembers() {
    let dir = logocraft_testkit::temp_dir("sfw-save");
    let (mut app, _) = app(&dir, false);
    app.run("file.saveForWeb", json!({})).unwrap();
    // A preset field loads the preset; the keys set after it win.
    set(&mut app, "preset", json!("JPEG, quality 50"));
    frame(&mut app);
    let d = app.ui.dialog.as_ref().unwrap();
    assert_eq!((d.str("format"), d.f64("quality", 0.0)), ("jpg".into(), 50.0));
    assert!(!d.fields.contains_key("preset"));
    set(&mut app, "quality", json!(70));
    let r = confirm(&mut app).unwrap();
    assert!(app.ui.dialog.is_none());
    let path = dir.join("Untitled-1.jpg");
    assert_eq!(r["path"], json!(path.to_string_lossy().replace('\\', "/")));
    assert!(std::fs::read(&path).unwrap().starts_with(&[0xFF, 0xD8]));
    assert_eq!(app.session.last_web_settings().quality, 70, "remembered");
    // It reopens on them; Done remembers without writing.
    app.run("file.saveForWeb", json!({})).unwrap();
    assert_eq!(app.ui.dialog.as_ref().unwrap().f64("quality", 0.0), 70.0);
    set(&mut app, "format", json!("png24"));
    set(&mut app, "discard", json!(true));
    assert_eq!(confirm(&mut app).unwrap()["remembered"], true);
    assert!(app.ui.dialog.is_none());
    assert_eq!(app.session.last_web_settings().format, logocraft_engine::cmd::webexport::WebFormat::Png24);
    assert!(!dir.join("Untitled-1.png").exists());
    // Bad settings keep the dialog open, with nothing written.
    app.run("file.saveForWeb", json!({})).unwrap();
    set(&mut app, "colorTable", json!({"locked": ["nope"]}));
    assert!(confirm(&mut app).is_err());
    assert!(!dir.join("Untitled-1.png").exists());
}

#[test]
fn the_web_downloads_and_has_no_browser_preview() {
    let dir = logocraft_testkit::temp_dir("sfw-web");
    let (mut app, log) = app(&dir, true);
    assert_eq!((spec(save_for_web::KIND).ok_label.unwrap())(&app), "Download");
    let r = app.run("file.saveForWeb", json!({"format": "png8", "output": "html"})).unwrap();
    assert_eq!(r["files"], json!(["Untitled-1.html", "images/Untitled-1.png"]));
    let log = log.borrow().clone();
    assert!(log[0].starts_with("download Untitled-1.html ") && log[1].starts_with("download Untitled-1.png "), "{log:?}");
    assert!(app.run("file.saveForWeb.browser", json!({})).is_err());
}

#[test]
fn preview_in_browser_opens_the_page() {
    let dir = logocraft_testkit::temp_dir("sfw-browser");
    let (mut app, log) = app(&dir, false);
    let r = app.run("file.saveForWeb.browser", json!({"format": "jpg"})).unwrap();
    let page = r["path"].as_str().unwrap().to_string();
    assert!(page.ends_with("/Untitled-1.html"));
    assert!(std::fs::read_to_string(&page).unwrap().contains("images/Untitled-1.jpg"));
    let log = log.borrow();
    assert!(log.last().unwrap().starts_with("open file://") && log.last().unwrap().ends_with("Untitled-1.html"), "{log:?}");
}

#[test]
fn colour_table_buttons_edit_the_lists() {
    let mut d = Dialog::new(save_for_web::KIND, json!({}));
    save_for_web::toggle(&mut d, "locked", "#cc3300", true);
    save_for_web::toggle(&mut d, "webShift", "#123456", true);
    save_for_web::toggle(&mut d, "locked", "#CC3300", true);
    assert_eq!(d.fields["colorTable"], json!({"locked": ["#cc3300"], "webShift": ["#123456"]}));
    save_for_web::toggle(&mut d, "locked", "#cc3300", false);
    save_for_web::set_table(&mut d, "sort", json!("hue"));
    assert_eq!(d.fields["colorTable"], json!({"locked": [], "webShift": ["#123456"], "sort": "hue"}));
}

/// LogoCraft: Cmd+Shift+S is the image export (Save for Web), Save As moves to Cmd+Alt+Shift+S, and
/// Asset Export is tabbed with Artboards.
#[test]
fn cmd_shift_s_opens_save_for_web_and_asset_export_sits_with_artboards() {
    assert_eq!(crate::shortcut_editor::command_shortcut("file.saveForWeb"), Some("Cmd+Shift+S"));
    assert_eq!(crate::shortcut_editor::command_shortcut("file.saveAs"), Some("Cmd+Alt+Shift+S"));
    assert!(crate::state::ICON_PANEL_GROUPS.iter().any(|g| *g == ["artboards", crate::panels::asset_export::ID]));
    let mut app = LogocraftApp::new(Session::new(), Default::default());
    app.run("file.new", json!({"width": 100, "height": 100})).unwrap();
    app.run("file.saveForWeb", json!({})).unwrap();
    assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some(save_for_web::KIND));
}
