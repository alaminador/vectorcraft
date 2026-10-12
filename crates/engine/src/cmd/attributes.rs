//! Window → Attributes: overprint, centre point display, image map, URL and note
//! (`attributes.set`), the fill rule (`path.setFillRule`) and, through `path.reverse`, the path
//! direction. The panel reads everything back with `attributes.info` ([`super::overprint`]).

use std::collections::BTreeSet;

use logocraft_doc::{Document, Node, NodeKind};
use logocraft_geom::FillRule;
use serde_json::{Value, json};

use super::overprint::{AttributesInfo, edit_counted};
use super::path::paths_in;
use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "path.setFillRule",
        "Fill Rule",
        [],
        None,
        "{rule: \"nonZero\"|\"evenOdd\", ids?} the fill rule of the paths and compound paths in ids or the selection (groups: those inside), as one undo step → {changed}",
        has_doc,
        set_fill_rule
    )]
}

fn parse_rule(s: &str) -> Option<FillRule> {
    match s.to_ascii_lowercase().replace(['-', ' ', '_'], "").as_str() {
        "nonzero" | "nonzerowinding" => Some(FillRule::NonZero),
        "evenodd" => Some(FillRule::EvenOdd),
        _ => None,
    }
}

/// The one value every item of `it` shares, if any.
fn same<T: PartialEq>(mut it: impl Iterator<Item = T>) -> Option<T> {
    let first = it.next()?;
    it.all(|v| v == first).then_some(first)
}

/// The paths and compound paths in `n`'s subtree that have a fill rule of their own with it (not a
/// compound path's members, which fill with its rule).
fn ruled<'a>(n: &'a Node, out: &mut Vec<(&'a Node, FillRule)>) {
    match &n.kind {
        NodeKind::Path { rule, guide: false, .. } | NodeKind::Compound { rule, .. } => out.push((n, *rule)),
        _ => n.children().into_iter().flatten().for_each(|c| ruled(c, out)),
    }
}

/// A directly selected compound member fills with its compound's rule. Its other attributes
/// and path direction still belong to the member itself.
fn fill_rule_owner(d: &Document, id: NodeId) -> Option<&Node> {
    let n = d.node(id)?;
    match d.parent_of(id).and_then(|parent| d.node(parent)) {
        Some(parent) if matches!(parent.kind, NodeKind::Compound { .. }) => Some(parent),
        _ => Some(n),
    }
}

/// The Attributes panel's values of `info.ids` beyond overprinting.
pub(super) fn fill_info(d: &Document, info: &mut AttributesInfo) {
    let nodes: Vec<&Node> = info.ids.iter().filter_map(|id| d.node(*id)).collect();
    let attrs = || nodes.iter().map(|n| n.extras().attrs.as_deref().cloned().unwrap_or_default());
    info.show_center = same(nodes.iter().map(|n| n.shows_center()));
    info.image_map = same(attrs().map(|a| a.image_map));
    info.url = same(attrs().map(|a| a.url));
    info.note = same(attrs().map(|a| a.note));
    let mut all = vec![];
    nodes.iter().filter_map(|n| fill_rule_owner(d, n.id)).for_each(|n| ruled(n, &mut all));
    info.fill_rule = same(all.into_iter().map(|(_, r)| r));
    let paths = paths_in(d, &info.ids);
    let subpaths = paths.iter().filter_map(|id| d.node(*id)?.path_data()).flat_map(|p| &p.subpaths);
    info.reversed = same(subpaths.map(|sp| sp.signed_area() < 0.0));
}

fn set_fill_rule(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "path.setFillRule";
    let rule = str_param(p, "rule").and_then(parse_rule).ok_or_else(|| bad(C, "`rule` must be nonZero or evenOdd"))?;
    let ids = targets(s, p)?;
    let changed = edit_counted(s, "Fill Rule", |d| {
        let mut all = vec![];
        ids.iter().filter_map(|id| fill_rule_owner(d, *id)).for_each(|n| ruled(n, &mut all));
        let todo: BTreeSet<_> = all.into_iter().filter(|(_, r)| *r != rule).map(|(n, _)| n.id).collect();
        for id in &todo {
            if let Some(NodeKind::Path { rule: r, .. } | NodeKind::Compound { rule: r, .. }) = d.node_mut(*id).map(|n| &mut n.kind) {
                *r = rule;
            }
        }
        Ok(todo.len())
    })?;
    Ok(json!({ "changed": changed }))
}
