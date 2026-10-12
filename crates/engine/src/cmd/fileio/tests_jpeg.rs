//! JPEG Options: colour model, coding method, scans, profile and image maps.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 60, "height": 40})).unwrap();
    s
}

fn b64(v: &Value) -> Vec<u8> {
    logocraft_format::base64_decode(v["dataBase64"].as_str().expect("dataBase64")).unwrap()
}

/// The JPEG's SOF marker and the number of components in its frame header.
fn frame(file: &[u8]) -> (u8, u8) {
    let mut i = 2;
    while !(0xC0..=0xC2).contains(&file[i + 1]) {
        i += 2 + u16::from_be_bytes([file[i + 2], file[i + 3]]) as usize;
    }
    (file[i + 1], file[i + 9])
}

#[test]
fn jpeg_options_reach_the_file() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 30, "height": 20})).unwrap();
    let jpg = |s: &mut Session, p: Value| b64(&s.execute("document.export", &merge(json!({"format": "jpg"}), p)).unwrap());
    assert_eq!(frame(&jpg(&mut s, json!({}))), (0xC0, 3), "baseline RGB by default");
    assert_eq!(frame(&jpg(&mut s, json!({"colorModel": "cmyk"}))), (0xC0, 4));
    assert_eq!(frame(&jpg(&mut s, json!({"colorModel": "grayscale"}))), (0xC0, 1));
    assert_eq!(frame(&jpg(&mut s, json!({"method": "progressive", "scans": 5}))).0, 0xC2);
    let icc = |f: &[u8]| f.windows(12).any(|w| w == b"ICC_PROFILE\0");
    assert!(icc(&jpg(&mut s, json!({}))) && !icc(&jpg(&mut s, json!({"embedIcc": false}))));
    assert!(jpg(&mut s, json!({"quality": 0})).len() < jpg(&mut s, json!({"quality": 100})).len());
    for bad in [json!({"colorModel": "lab"}), json!({"method": "fast"}), json!({"imageMap": "both"})] {
        assert!(s.execute("document.export", &merge(json!({"format": "jpg"}), bad.clone())).is_err(), "{bad}");
    }
    let r = s.execute("document.formats", &json!({})).unwrap();
    let jpg = r["formats"].as_array().unwrap().iter().find(|f| f["id"] == "jpg").unwrap();
    for k in ["colorModel", "method", "scans", "embedIcc", "imageMap"] {
        assert!(jpg["options"].get(k).is_some(), "{k}");
    }
}
