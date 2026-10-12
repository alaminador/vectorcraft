//! The Graphic Styles panel: new, apply (replace or add), link, redefine, break link, rename,
//! delete, duplicate, sort and list.
//!
//! Applying a style links the targeted objects to it ([`Node::graphic_style`]). An object stays
//! linked while it keeps the style's look; editing its appearance or transparency breaks the link
//! ([`in_sync`]), and Redefine Graphic Style updates only the objects still linked.
//!
//! Styles keep placed gradients relative to the unit box ([`GraphicStyle::unit_box`]): a new or
//! redefined style stores them relative to its source's bounds, and each object a style is applied
//! to gets them at the same place relative to its own.
//!
//! With Override Character Color on ([`crate::Prefs::override_char_color`]), type a style is applied
//! to loses its characters' own fill (stroke) when the style has fills (strokes), so the style's
//! paint shows instead of the character colour under it.

use serde_json::{Value, json};
use vectorcraft_color::Paint;
use vectorcraft_doc::{Appearance, Document, GraphicStyle, Node, NodeKind};
use vectorcraft_geom::Rect;

use super::*;
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!("graphicStyle.duplicate", "Duplicate Graphic Style", ["Window", "Graphic Styles"], None, "{name} → {name}", has_doc, style_duplicate)]
}

// ---------- links ----------

/// Does a style applied to `n` (`top`: `n` is the target itself) descend into its children? Groups
/// and layers style their contents, and so do other containers (blends…) inside them, while a
/// compound path takes it as a whole, as with paint commands ([`leaf_targets`]).
fn descends(n: &Node, top: bool) -> bool {
    match n.kind {
        NodeKind::Group { .. } | NodeKind::Layer { .. } => true,
        NodeKind::Compound { .. } => false,
        _ => !top && n.is_container(),
    }
}

/// The objects whose appearance a style applied to `n` sets: `n`, or its painted contents.
fn painted<'a>(n: &'a Node, top: bool, out: &mut Vec<&'a Node>) {
    match n.children() {
        Some(ch) if descends(n, top) => ch.iter().for_each(|c| painted(c, false, out)),
        _ => out.push(n),
    }
}

/// Does `n` still have style `g`'s look (its transparency, and the style's appearance as its own;
/// a group or layer with no appearance of its own: on every painted object inside it, the look a
/// style made from it captures)? Linked objects that don't are no longer linked.
pub(crate) fn in_sync(n: &Node, g: &GraphicStyle) -> bool {
    if !g.transparency_matches(n) {
        return false;
    }
    if g.appearance_on(n).approx_eq(&n.appearance) {
        return true;
    }
    let mut leaves = vec![];
    painted(n, true, &mut leaves);
    n.appearance == Appearance::default() && leaves.iter().all(|l| l.id != n.id && g.appearance_on(l).approx_eq(&l.appearance))
}

/// The style `n` is linked to (and still looks like).
pub(crate) fn linked_style<'a>(d: &'a Document, n: &Node) -> Option<&'a GraphicStyle> {
    d.graphic_style_by_id(n.extras().graphic_style?).filter(|g| in_sync(n, g))
}

/// `ap` with its placed gradients moved from box `from` (an object's bounds) to the unit box.
fn unitized(mut ap: Appearance, from: Option<Rect>) -> Appearance {
    if let Some(b) = from.filter(|_| ap.has_placed_gradient()) {
        ap.rebase_gradients(b, GraphicStyle::UNIT_BOX);
    }
    ap
}

/// The appearance a new or redefined style takes from `n`: its own. A group without fills or
/// strokes of its own lends those of its topmost painted object, and type its characters' fill
/// and stroke. Placed gradients come relative to the unit box (from the bounds of the object
/// they're on). The pen pressure of strokes stays with their own path.
pub(crate) fn captured(n: &Node) -> Appearance {
    let mut ap = (*n.appearance).clone().without_pressure();
    if !ap.items.is_empty() {
        return unitized(ap, n.geometric_bounds());
    }
    match &n.kind {
        NodeKind::Text(t) => {
            if let Some(r) = t.runs.first() {
                ap.items = r.style.basic_appearance().items;
                // Type paints its characters in text space.
                ap = unitized(ap, Some(t.local_bounds()));
            }
        }
        _ => {
            let mut leaves = vec![];
            painted(n, true, &mut leaves);
            if let Some(top) = leaves.last().filter(|l| l.id != n.id) {
                // The topmost object's own effects apply before the group's.
                let top = captured(top);
                ap.items = top.items;
                ap.effects.splice(0..0, top.effects);
            }
        }
    }
    ap
}

/// Override Character Color: type taking appearance `ap` loses its characters' fill when `ap` has
/// fills, and their stroke when it has strokes.
fn override_chars(n: &mut Node, ap: &Appearance) {
    let NodeKind::Text(t) = &mut n.kind else { return };
    let (fills, strokes) = (ap.items.iter().any(|i| i.is_fill()), ap.items.iter().any(|i| !i.is_fill()));
    for r in &mut t.runs {
        if fills {
            r.style.fill = Paint::None;
        }
        if strokes {
            r.style.stroke = Paint::None;
        }
    }
}

/// Give `n` style `g`: its appearance (a group or layer too: its own, above its contents; type its
/// characters' colour too with `chars`, Override Character Color) and transparency.
fn style_node(n: &mut Node, g: &GraphicStyle, chars: bool) {
    n.appearance = g.appearance_on(n).into_owned().into();
    if chars {
        override_chars(n, &g.appearance);
    }
    g.apply_transparency(n);
}

fn style_index(d: &Document, name: &str) -> Result<usize> {
    d.graphic_style_index(name).ok_or_else(|| EngineError::Other(format!("no graphic style `{name}`")))
}

impl Session {
    /// The graphic style of the first selected object: the one it was last linked to, and whether
    /// it is still linked (still has that style's look).
    pub fn selection_graphic_style(&self) -> Option<(&GraphicStyle, bool)> {
        let st = self.active()?;
        let n = st.doc.node(*st.selection.subjects().first()?)?;
        let g = st.doc.graphic_style_by_id(n.extras().graphic_style?)?;
        Some((g, in_sync(n, g)))
    }

    /// A copy of `n` with style `g` applied as `graphicStyle.apply` applies it (Override Character
    /// Color as set): the Graphic Styles panel's previews.
    pub fn styled(&self, n: &Node, g: &GraphicStyle) -> Node {
        let mut n = n.clone();
        style_node(&mut n, g, self.prefs.override_char_color);
        n
    }
}

// ---------- commands ----------

fn style_duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("graphicStyle.duplicate", "missing name"))?.to_string();
    let new = s.edit("Duplicate Graphic Style", |d, _| {
        let pos = style_index(d, &name)?;
        let nm = unique_name(&format!("{name} copy"), |n| d.graphic_style(n).is_some());
        let g = GraphicStyle { name: nm.clone(), id: d.next_graphic_style_id(), ..d.graphic_styles[pos].clone() };
        d.graphic_styles.insert(pos + 1, g);
        Ok(nm)
    })?;
    Ok(json!({"name": new}))
}
