//! Export As Text, WebP options and Save for Office Documents.

use serde_json::{Value, json};

use super::*;

fn b64(v: &Value) -> Vec<u8> {
    vectorcraft_format::base64_decode(v["dataBase64"].as_str().expect("dataBase64")).unwrap()
}

#[test]
fn webp_is_lossless_and_says_so_when_lossy_is_asked_for() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 30, "height": 20})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 2, "y": 2, "width": 10, "height": 10})).unwrap();
    let r = s.execute("document.export", &json!({"format": "webp"})).unwrap();
    let file = b64(&r);
    assert_eq!((&file[..4], &file[8..12], &file[12..16]), (&b"RIFF"[..], &b"WEBP"[..], &b"VP8L"[..]), "lossless bitstream");
    assert_eq!(r["warnings"], json!([]));
    let r = s.execute("document.export", &json!({"format": "webp", "lossless": false, "quality": 60, "background": "black"})).unwrap();
    assert_eq!(&b64(&r)[12..16], b"VP8L");
    assert!(r["warnings"][0].as_str().unwrap().contains("lossless"), "{r}");
    assert_eq!(image::load_from_memory(&b64(&r)).unwrap().to_rgba8().get_pixel(29, 19).0, [0, 0, 0, 255]);
}
