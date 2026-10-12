//! Creating art: shapes, paths, text.

use serde_json::{Value, json};
use vectorcraft_doc::{Appearance, CharStyle, LiveShape, Node, NodeKind, TextObject};
use vectorcraft_geom::{Affine, Anchor, AnchorKind, FillRule, PathData, Point, Rect, SubPath, shapes};

use super::newart::NewArt;
use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "shape.rectangle",
            "Rectangle",
            [],
            None,
            "{x, y, width, height, radius?: pt, rotation?: deg (about centre)} → {id}",
            has_doc,
            rectangle
        ),
        cmd!("shape.ellipse", "Ellipse", [], None, "{x, y, width, height, rotation?: deg (about centre)} → {id}", has_doc, ellipse),
        cmd!("shape.polygon", "Polygon", [], None, "{cx, cy, radius, sides=6, rotation?: deg} → {id}", has_doc, polygon),
        cmd!("shape.star", "Star", [], None, "{cx, cy, radius1, radius2, points=5, rotation?: deg} → {id}", has_doc, star),
        cmd!("shape.line", "Line Segment", [], None, "{x1, y1, x2, y2} → {id}", has_doc, line),
        cmd!("shape.spiral", "Spiral", [], None, "{cx, cy, radius, decay=80 (%), segments=10, clockwise?} → {id}", has_doc, spiral),
        cmd!("shape.arc", "Arc", [], None, "{x1, y1, x2, y2, closed?} → {id}", has_doc, arc),
        cmd!(
            "path.create",
            "Create Path",
            [],
            None,
            "{anchors: [{x, y, in?: [x,y], out?: [x,y]}], closed?: bool, d?: SVG path data} → {id}",
            has_doc,
            path_create
        ),
        cmd!(
            "view.drawMode",
            "Drawing Mode",
            [],
            Some("Shift+D"),
            "{mode?: normal|behind|inside} (no param cycles; inside needs one selected path)",
            has_doc,
            draw_mode
        ),
        cmd!(
            "text.create",
            "Create Text",
            [],
            None,
            "{x, y, text, vertical?: bool = false, size?: pt, font?: family, style?, color?, area?: {width, height, fit?: none|autoHeight|shrinkText, fitMinPercent?}, placeholder?: bool (fill it with placeholder text instead, as the Type tools do with prefs placeholderText), leadingModel?: \"romanBaseline\"|\"emBoxTop\", charAlign?: \"romanBaseline\"|\"emBoxTop\"|\"emBoxCenter\"|\"emBoxBottom\"|\"icfTop\"|\"icfBottom\" (default: emBoxTop and emBoxCenter while the interface is in Japanese, else romanBaseline)} → {id}; new area type gets Auto Size (fit autoHeight) when the autoSizeAreaType preference is on",
            has_doc,
            text_create
        ),
    ]
}

/// Insert a new object at the top of the insertion parent with the current paint and new-art
/// template; select it.
pub(crate) fn add_art(s: &mut Session, label: &str, kind: NodeKind, name: Option<String>) -> Result<Value> {
    let look = s.new_art_look(s.paint.fill.clone(), s.paint.stroke.clone(), s.paint.stroke_width);
    add_look(s, label, kind, look, name)
}

/// [`add_look`] with just `appearance`.
pub(crate) fn add_node(s: &mut Session, label: &str, kind: NodeKind, appearance: Appearance, name: Option<String>) -> Result<Value> {
    add_look(s, label, kind, NewArt::plain(appearance), name)
}

/// Insert a new object looking like `look` (drawing modes apply); select it.
pub(crate) fn add_look(s: &mut Session, label: &str, kind: NodeKind, look: NewArt, name: Option<String>) -> Result<Value> {
    let parent = s.doc()?.target_parent()?;
    let mode = s.draw_mode;
    let inside = s.draw_inside;
    let behind_of = s.doc()?.selection.in_paint_order(&s.doc()?.doc).first().copied();
    let id = s.edit(label, |d, sel| {
        let id = d.alloc_id();
        let mut n = Node::new(id, kind);
        look.apply(d, &mut n);
        n.name = name;
        match (mode, inside) {
            (crate::DrawMode::Inside, Some(target)) if d.node(target).is_some() => {
                // Wrap the target once in a clip group [mask copy, target, …new art].
                let group = match d.parent_of(target).and_then(|p| d.node(p).map(|pn| (p, pn.kind.clone()))) {
                    Some((p, NodeKind::Group { clip: true, children })) if children.get(1).is_some_and(|c| c.id == target) => p,
                    _ => {
                        let (par, idx, _) = d.position(target).ok_or(crate::EngineError::NoNode(target))?;
                        let mut mask = d.node(target).cloned().ok_or(crate::EngineError::NoNode(target))?;
                        mask.id = d.alloc_id();
                        mask.appearance = Appearance::basic(vectorcraft_color::Paint::None, vectorcraft_color::Paint::None, 0.0).into();
                        if let NodeKind::Path { clipping, .. } = &mut mask.kind {
                            *clipping = true;
                        }
                        let gid = d.alloc_id();
                        d.insert(par, idx + 1, Node::new(gid, NodeKind::Group { children: vec![std::sync::Arc::new(mask)], clip: true }))?;
                        d.move_node(target, Some(gid), usize::MAX)?;
                        gid
                    }
                };
                d.insert(Some(group), usize::MAX, n)?;
            }
            (crate::DrawMode::Behind, _) => match behind_of.and_then(|b| d.position(b)) {
                Some((par, idx, _)) => {
                    d.insert(par, idx, n)?;
                }
                None => {
                    d.insert(parent, 0, n)?;
                }
            },
            _ => {
                d.insert(parent, usize::MAX, n)?;
            }
        }
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}

fn draw_mode(s: &mut Session, p: &Value) -> Result<Value> {
    let mode = match str_param(p, "mode") {
        Some("behind") => crate::DrawMode::Behind,
        Some("inside") => crate::DrawMode::Inside,
        Some("normal") => crate::DrawMode::Normal,
        None => match s.draw_mode {
            crate::DrawMode::Normal => crate::DrawMode::Behind,
            crate::DrawMode::Behind if s.doc()?.selection.len() == 1 => crate::DrawMode::Inside,
            _ => crate::DrawMode::Normal,
        },
        Some(o) => return Err(bad("view.drawMode", format!("unknown mode `{o}`"))),
    };
    if mode == crate::DrawMode::Inside {
        let st = s.doc()?;
        let target =
            st.selection.objects.first().copied().filter(|id| {
                matches!(st.doc.node(*id).map(|n| &n.kind), Some(NodeKind::Path { .. } | NodeKind::Compound { .. } | NodeKind::Text(_)))
            });
        match target {
            Some(t) if st.selection.len() == 1 => s.draw_inside = Some(t),
            _ => return Err(bad("view.drawMode", "Draw Inside needs exactly one selected path, compound path or text")),
        }
    } else {
        s.draw_inside = None;
    }
    s.draw_mode = mode;
    Ok(json!({ "mode": mode }))
}

fn path_kind(path: PathData, live: Option<LiveShape>) -> NodeKind {
    NodeKind::Path { path, rule: FillRule::NonZero, live: live.map(Box::new), clipping: false, guide: false }
}

fn rect_of(p: &Value, cmd: &str) -> Result<Rect> {
    let x = f64_req(p, "x", cmd)?;
    let y = f64_req(p, "y", cmd)?;
    let w = f64_req(p, "width", cmd)?;
    let h = f64_req(p, "height", cmd)?;
    if !(w.is_finite() && h.is_finite()) {
        return Err(bad(cmd, "invalid size"));
    }
    Ok(Rect::new(x, y, x + w, y + h).abs())
}

fn rect_transform(r: Rect, p: &Value, cmd: &str) -> Result<Affine> {
    let rotation = if p.get("rotation").is_some() { f64_req(p, "rotation", cmd)? } else { 0.0 };
    if !rotation.is_finite() {
        return Err(bad(cmd, "rotation must be finite"));
    }
    if rotation == 0.0 {
        return Ok(Affine::translate(r.origin().to_vec2()));
    }
    Ok(Affine::translate(r.center().to_vec2()) * Affine::rotate(rotation.to_radians()) * Affine::translate((-r.width() / 2.0, -r.height() / 2.0)))
}

fn rectangle(s: &mut Session, p: &Value) -> Result<Value> {
    let r = rect_of(p, "shape.rectangle")?;
    let radius = f64_or(p, "radius", 0.0).max(0.0);
    let live = LiveShape::Rectangle {
        w: r.width(),
        h: r.height(),
        radii: [radius; 4],
        kinds: Default::default(),
        xf: rect_transform(r, p, "shape.rectangle")?,
    };
    let label = if radius > 0.0 { "Rounded Rectangle" } else { "Rectangle" };
    add_art(s, label, path_kind(live.to_path(), Some(live)), None)
}

fn ellipse(s: &mut Session, p: &Value) -> Result<Value> {
    let r = rect_of(p, "shape.ellipse")?;
    let live = LiveShape::Ellipse { w: r.width(), h: r.height(), pie: (0.0, 360.0), xf: rect_transform(r, p, "shape.ellipse")? };
    add_art(s, "Ellipse", path_kind(live.to_path(), Some(live)), None)
}

fn polygon(s: &mut Session, p: &Value) -> Result<Value> {
    let c = Point::new(f64_req(p, "cx", "shape.polygon")?, f64_req(p, "cy", "shape.polygon")?);
    let r = f64_req(p, "radius", "shape.polygon")?.abs();
    let sides = p.get("sides").and_then(Value::as_u64).unwrap_or(6).clamp(3, 1000) as u32;
    let rot = f64_or(p, "rotation", 0.0);
    let xf = Affine::translate(c.to_vec2()) * Affine::rotate(rot.to_radians());
    let live = LiveShape::Polygon { radius: r, sides, xf, radii: vec![], kinds: vec![] };
    add_art(s, "Polygon", path_kind(live.to_path(), Some(live)), None)
}

fn star(s: &mut Session, p: &Value) -> Result<Value> {
    let c = Point::new(f64_req(p, "cx", "shape.star")?, f64_req(p, "cy", "shape.star")?);
    let r1 = f64_req(p, "radius1", "shape.star")?.abs();
    let r2 = f64_or(p, "radius2", r1 / 2.0).abs();
    let n = p.get("points").and_then(Value::as_u64).unwrap_or(5).clamp(2, 1000) as u32;
    let path = shapes::star(c, r1, r2, n, f64_or(p, "rotation", 0.0));
    add_art(s, "Star", path_kind(path, None), None)
}

fn line(s: &mut Session, p: &Value) -> Result<Value> {
    let a = Point::new(f64_req(p, "x1", "shape.line")?, f64_req(p, "y1", "shape.line")?);
    let b = Point::new(f64_req(p, "x2", "shape.line")?, f64_req(p, "y2", "shape.line")?);
    let live = LiveShape::Line { a, b };
    // Lines are stroked, never filled (Illustrator ignores the fill for new open lines).
    let look = s.new_art_look(
        vectorcraft_color::Paint::None,
        if s.paint.stroke.is_none() { vectorcraft_color::Paint::solid(vectorcraft_color::Color::BLACK) } else { s.paint.stroke.clone() },
        s.paint.stroke_width.max(0.1),
    );
    add_look(s, "Line", path_kind(live.to_path(), Some(live)), look, None)
}

fn spiral(s: &mut Session, p: &Value) -> Result<Value> {
    let c = Point::new(f64_req(p, "cx", "shape.spiral")?, f64_req(p, "cy", "shape.spiral")?);
    let path = shapes::spiral(
        c,
        f64_req(p, "radius", "shape.spiral")?,
        f64_or(p, "decay", 80.0),
        p.get("segments").and_then(Value::as_u64).unwrap_or(10).clamp(2, 1000) as u32,
        bool_or(p, "clockwise", true),
    );
    add_art(s, "Spiral", path_kind(path, None), None)
}

fn arc(s: &mut Session, p: &Value) -> Result<Value> {
    let a = Point::new(f64_req(p, "x1", "shape.arc")?, f64_req(p, "y1", "shape.arc")?);
    let b = Point::new(f64_req(p, "x2", "shape.arc")?, f64_req(p, "y2", "shape.arc")?);
    add_art(s, "Arc", path_kind(shapes::arc(a, b, 0.0, bool_or(p, "closed", false)), None), None)
}

/// An anchor `{x, y, in?, out?, smooth?}` given to `cmd` (handles `[x, y]`, at the anchor when
/// absent or null): anything malformed fails rather than being left out or replaced by a default,
/// which would change the path's shape.
pub(crate) fn anchor_from_json(v: &Value, cmd: &str) -> Result<Anchor> {
    let coord =
        |key: &str| v.get(key).and_then(Value::as_f64).filter(|n| n.is_finite()).ok_or_else(|| bad(cmd, format!("anchor `{key}` must be a number")));
    let p = Point::new(coord("x")?, coord("y")?);
    let handle = |key: &str| match v.get(key).filter(|h| !h.is_null()) {
        None => Ok(p),
        Some(h) => finite_numbers(h).map(|[x, y]| Point::new(x, y)).ok_or_else(|| bad(cmd, format!("anchor `{key}` must be [x, y]"))),
    };
    let mut anchor = Anchor::with_handles(p, handle("in")?, handle("out")?);
    match v.get("smooth").filter(|b| !b.is_null()) {
        None | Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => anchor.kind = AnchorKind::Smooth,
        Some(_) => return Err(bad(cmd, "anchor `smooth` must be true or false")),
    }
    Ok(anchor)
}

fn path_create(s: &mut Session, p: &Value) -> Result<Value> {
    let path = if let Some(d) = str_param(p, "d") {
        let bp = vectorcraft_geom::BezPath::from_svg(d).map_err(|e| bad("path.create", format!("bad path data: {e}")))?;
        PathData::from_bezpath(&bp)
    } else {
        let anchors: Vec<Anchor> = p
            .get("anchors")
            .and_then(Value::as_array)
            .ok_or_else(|| bad("path.create", "missing anchors"))?
            .iter()
            .map(|a| anchor_from_json(a, "path.create"))
            .collect::<Result<_>>()?;
        if anchors.is_empty() {
            return Err(bad("path.create", "need at least one anchor"));
        }
        PathData::single(SubPath::new(anchors, bool_or(p, "closed", false)))
    };
    let mut look = s.new_art_look(s.paint.fill.clone(), s.paint.stroke.clone(), s.paint.stroke_width);
    // Open paths drawn with a stroke of None would be invisible: mimic Illustrator and keep what the user set.
    let ap = &mut look.appearance;
    if !path.is_closed() && ap.stroke_paint().is_none() && ap.fill_paint().is_none() {
        ap.set_stroke(vectorcraft_color::Paint::solid(vectorcraft_color::Color::BLACK));
    }
    add_look(s, "Pen", path_kind(path, None), look, None)
}

fn text_create(s: &mut Session, p: &Value) -> Result<Value> {
    let x = f64_req(p, "x", "text.create")?;
    let y = f64_req(p, "y", "text.create")?;
    let text = str_param(p, "text").unwrap_or("");
    let mut t = TextObject::point(Point::new(x, y), text, new_type_style(s, p));
    t.vertical = p.get("vertical").and_then(Value::as_bool).unwrap_or(false);
    t.para = new_type_para();
    new_type_alignment(s, p, "text.create", &mut t)?;
    if let Some(a) = p.get("area") {
        let w = f64_or(a, "width", 200.0);
        let h = f64_or(a, "height", 100.0);
        t.kind = vectorcraft_doc::TextKind::Area { frame: shapes::rectangle(Rect::new(0.0, 0.0, w, h)) };
        t.area.fit = new_area_fit(s, a, "text.create")?;
    }
    if bool_or(p, "placeholder", false) {
        super::typemenu::fill_with_placeholder(&mut t);
    } else {
        // Lays the text out (and sizes an Auto Size frame).
        super::typecmd::refresh_bounds(&mut t);
    }
    add_node(s, "Type", NodeKind::Text(Box::new(t)), Appearance::default(), None)
}

/// The paragraph attributes new type gets (the Type tools, `text.create`): aligned to the start of
/// each paragraph's direction (left, right for Hebrew and Arabic), Japanese punctuation composed
/// with Line-end Punctuation Half Width and Standard burasagari. Imported text keeps its own.
pub(crate) fn new_type_para() -> vectorcraft_doc::ParaStyle {
    vectorcraft_doc::ParaStyle {
        justify: vectorcraft_doc::Justify::Auto,
        mojikumi: vectorcraft_doc::Mojikumi::LineEndHalf,
        burasagari: vectorcraft_doc::Burasagari::Standard,
        ..Default::default()
    }
}

/// The leading model and character alignment new type starts with (`t`'s paragraph and runs):
/// `leadingModel` and `charAlign` from `p`, else em box top to top and em box centre while the
/// interface is in Japanese (#432), else the Roman baseline for both. Noted in the journal, so a
/// replay sets the same whatever the interface.
pub(crate) fn new_type_alignment(s: &mut Session, p: &Value, cmd: &str, t: &mut TextObject) -> Result<()> {
    use vectorcraft_doc::{CharAlign, LeadingModel};
    let japanese = s.japanese_interface();
    let leading = match p.get("leadingModel").filter(|v| !v.is_null()) {
        Some(v) => match v.as_str() {
            Some("romanBaseline") => LeadingModel::RomanBaseline,
            Some("emBoxTop") => LeadingModel::EmBoxTop,
            _ => return Err(bad(cmd, "`leadingModel` must be \"romanBaseline\" or \"emBoxTop\"")),
        },
        None => {
            let l = if japanese { LeadingModel::EmBoxTop } else { LeadingModel::RomanBaseline };
            s.note_journal("leadingModel", json!(l));
            l
        }
    };
    let align = match super::textedit::char_align_param(p, cmd)? {
        Some(a) => a,
        None => {
            let a = if japanese { CharAlign::EmBoxCenter } else { CharAlign::RomanBaseline };
            s.note_journal("charAlign", json!(a));
            a
        }
    };
    t.para.leading_model = leading;
    for r in &mut t.runs {
        r.style.char_align = align;
    }
    Ok(())
}

/// The fit new area type gets: `fit`/`fitMinPercent` from `p` (as `text.areaOptions` takes
/// them), else Auto Size when the "Auto Size New Area Type" preference is on.
pub(crate) fn new_area_fit(s: &Session, p: &Value, c: &str) -> Result<vectorcraft_doc::AreaFit> {
    let default = if s.prefs.auto_size_area_type { vectorcraft_doc::AreaFit::AutoHeight } else { vectorcraft_doc::AreaFit::None };
    Ok(super::typecmd::fit_param(p, default, c)?.unwrap_or(default))
}

/// The character style new type gets: `size`, `font`, `style` and `color` from `p`, else the
/// defaults, filled with the current fill unless that is None or white (black then).
pub(crate) fn new_type_style(s: &Session, p: &Value) -> CharStyle {
    let mut style = CharStyle::default();
    if let Some(sz) = p.get("size").and_then(Value::as_f64) {
        style.size = sz.clamp(0.1, 1296.0);
    }
    if let Some(f) = str_param(p, "font") {
        style.font_family = f.to_string();
    }
    if let Some(f) = str_param(p, "style") {
        style.font_style = f.to_string();
    }
    style.fill = match p.get("color").and_then(color_value) {
        Some(c) => vectorcraft_color::Paint::solid(c),
        None if !s.paint.fill.is_none() && s.paint.fill != vectorcraft_color::Paint::solid(vectorcraft_color::Color::WHITE) => s.paint.fill.clone(),
        None => vectorcraft_color::Paint::solid(vectorcraft_color::Color::BLACK),
    };
    style
}
