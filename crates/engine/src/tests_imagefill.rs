//! Image fills (`paint.imageFill`): a picture inside a shape, as its own pattern swatch, that
//! moves with the shape and survives a save.

use logocraft_color::Paint;
use logocraft_doc::{NodeId, NodeKind};
use serde_json::json;

use super::*;

fn png(w: u32, h: u32) -> String {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    logocraft_format::base64_encode(&out.into_inner())
}

fn fill_of(s: &Session, id: NodeId) -> Paint {
    s.doc().unwrap().doc.node(id).unwrap().appearance.fill_paint()
}

#[test]
fn an_image_fill_paints_the_picture_and_moves_with_the_shape() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let id = NodeId(s.execute("shape.rectangle", &json!({"x": 100, "y": 50, "width": 200, "height": 100})).unwrap()["id"].as_u64().unwrap());
    let r = s.execute("paint.imageFill", &json!({"dataBase64": png(100, 100), "mode": "fill"})).unwrap();
    assert_eq!(r["patterns"][0], "Image Fill 1");
    let Paint::Pattern { pattern, xf } = fill_of(&s, id) else { panic!("not a pattern fill") };
    assert_eq!(pattern, "Image Fill 1");
    let d = &s.doc().unwrap().doc;
    let def = d.pattern("Image Fill 1").unwrap();
    assert!(matches!(def.art[0].kind, NodeKind::Image(_)));
    assert_eq!((def.tile.width(), def.tile.height()), (200.0, 200.0), "covers the 200 × 100 shape");
    // Moving the shape moves the picture with it, Transform Patterns or not.
    s.execute("object.move", &json!({"dx": 10, "dy": 20})).unwrap();
    let Paint::Pattern { xf: moved, .. } = fill_of(&s, id) else { panic!() };
    assert_eq!(moved.as_coeffs()[4] - xf.as_coeffs()[4], 10.0);
    assert_eq!(moved.as_coeffs()[5] - xf.as_coeffs()[5], 20.0);
    // A second fill makes its own pattern; bad input is refused.
    s.execute("paint.imageFill", &json!({"dataBase64": png(10, 20), "mode": "tile", "scale": 50})).unwrap();
    assert!(s.doc().unwrap().doc.pattern("Image Fill 2").is_some());
    assert!(s.execute("paint.imageFill", &json!({"dataBase64": png(10, 10), "mode": "stretch"})).is_err());
    assert!(s.execute("paint.imageFill", &json!({"dataBase64": "!!"})).is_err());
    assert!(s.execute("paint.imageFill", &json!({"dataBase64": png(10, 10), "mode": "tile", "scale": 0})).is_err());
    // Saved and opened again with the picture.
    let doc = logocraft_format::load(&logocraft_format::save(&s.doc().unwrap().doc, false)).unwrap();
    let def = doc.pattern("Image Fill 2").unwrap();
    let NodeKind::Image(im) = &def.art[0].kind else { panic!() };
    assert!(doc.images.contains_key(&im.key));
}

#[test]
fn another_mode_lays_the_same_picture_out_again() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let id = NodeId(s.execute("shape.rectangle", &json!({"x": 100, "y": 50, "width": 200, "height": 100})).unwrap()["id"].as_u64().unwrap());
    // Nothing to lay out yet.
    assert!(s.execute("paint.imageFill", &json!({"mode": "fit"})).is_err());
    s.execute("paint.imageFill", &json!({"dataBase64": png(100, 100)})).unwrap();
    let r = s.execute("paint.imageFill", &json!({"mode": "fit"})).unwrap();
    assert_eq!(r["patterns"][0], "Image Fill 1", "the same pattern, not a new one");
    let d = &s.doc().unwrap().doc;
    assert_eq!(d.patterns.iter().filter(|p| logocraft_doc::pattern::is_image_fill(&p.name)).count(), 1);
    let def = d.pattern("Image Fill 1").unwrap();
    assert_eq!((def.tile.width(), def.tile.height()), (200.0, 100.0), "fit: the tile is the shape's bounds");
    assert!(matches!(fill_of(&s, id), Paint::Pattern { .. }));
}
