//! Paintbrush: the freehand gesture, painting with the current brush. Its `path.freehand`
//! previews become `brush.freehand` when a brush is current (tool option `brush`, else the
//! document's current brush); without one it draws like the Pencil, smoothed.

use logocraft_geom::Point;
use serde_json::{Value, json};

use super::GestureTool;
use crate::{Action, Cursor, Mods, Overlay, PointerEvent, Tool, ToolContext, ToolKey};

pub struct BrushPaintTool {
    inner: GestureTool,
    /// Brush override (None = the document's current brush, if any).
    pub brush: Option<String>,
}

impl Default for BrushPaintTool {
    fn default() -> Self {
        Self { inner: GestureTool::new("paintbrush"), brush: None }
    }
}

impl BrushPaintTool {
    fn brush_name(&self, cx: &ToolContext) -> Option<String> {
        self.brush.clone().or_else(|| cx.doc.unknown.get("currentBrush").and_then(Value::as_str).map(str::to_string))
    }

    fn map(&self, cx: &ToolContext, acts: Vec<Action>) -> Vec<Action> {
        let Some(b) = self.brush_name(cx) else { return acts };
        acts.into_iter()
            .map(|a| match a {
                Action::Preview(c, mut v) if c == "path.freehand" => {
                    v["brush"] = json!(b);
                    Action::Preview("brush.freehand".into(), v)
                }
                o => o,
            })
            .collect()
    }
}

impl Tool for BrushPaintTool {
    fn id(&self) -> &'static str {
        "paintbrush"
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let acts = self.inner.pointer(cx, ev);
        self.map(cx, acts)
    }
    fn key(&mut self, cx: &ToolContext, key: ToolKey, m: Mods) -> Vec<Action> {
        let acts = self.inner.key(cx, key, m);
        self.map(cx, acts)
    }
    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        self.inner.overlays(cx)
    }
    fn cursor(&self, cx: &ToolContext, p: Point, m: Mods) -> Cursor {
        self.inner.cursor(cx, p, m)
    }
    fn options(&self) -> Value {
        let mut v = self.inner.options();
        v["brush"] = json!(self.brush);
        v
    }
    fn set_option(&mut self, key: &str, v: &Value) {
        if key == "brush" {
            self.brush = v.as_str().map(str::to_string);
        } else {
            self.inner.set_option(key, v);
        }
    }
    fn busy(&self) -> bool {
        self.inner.busy()
    }
    fn deactivate(&mut self, cx: &ToolContext) -> Vec<Action> {
        let acts = self.inner.deactivate(cx);
        self.map(cx, acts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PointerKind;
    use crate::testutil::*;
    use logocraft_doc::Selection;

    #[test]
    fn paintbrush_rewrites_freehand_to_brush_freehand() {
        let (mut d, _) = doc_with_rect();
        let (s, p) = (Selection::default(), paint());
        {
            let cx = cx(&d, &s, &p);
            let mut t = BrushPaintTool::default();
            t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 0.0, 0.0));
            let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 30.0, 0.0));
            assert!(matches!(&a[1], Action::Preview(c, _) if c == "path.freehand"), "no brush: plain freehand");
        }
        d.unknown.insert("currentBrush".into(), json!("Charcoal"));
        let cx = cx(&d, &s, &p);
        let mut t = BrushPaintTool::default();
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 0.0, 0.0));
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 30.0, 0.0));
        assert!(matches!(&a[1], Action::Preview(c, v) if c == "brush.freehand" && v["brush"] == "Charcoal"));
        t.set_option("brush", &json!("Dots"));
        assert_eq!(t.options()["brush"], "Dots");
    }
}
