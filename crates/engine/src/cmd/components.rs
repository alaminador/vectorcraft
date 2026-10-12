//! Components, the Figma way, on top of symbols (`Document::symbols`):
//!
//! - **Create Component** (Cmd+Alt+K) turns the selection into the *main component*: it stays on
//!   the canvas where it was, editable like any art, marked with [`NodeExtras::component`]. A
//!   symbol of the same name holds a copy of its art, which instances draw.
//! - After every edit ([`sync`]), a main component that changed refreshes its symbol, so every
//!   instance follows at once; when it grew or shrank, the instances grow or shrink with it
//!   (about their centres).
//! - A copy of a main component (duplicate, copy and paste, Alt-drag) is an *instance*, not a
//!   second main.
//! - **Go to Main Component** selects an instance's main; **Detach Instance** (Cmd+Alt+B) turns
//!   instances into plain art.
//!
//! [`NodeExtras::component`]: logocraft_doc::NodeExtras::component

use std::collections::BTreeMap;
use std::sync::Arc;

use logocraft_doc::{Document, Node, NodeId, NodeKind, Symbol};
use logocraft_geom::Affine;
use serde_json::{Value, json};

use super::brushsym::{
    break_links, instances_of, natural_size, normalize, place_xf, roots, selected_instances, set_natural_size, unique_symbol_name,
};
use super::*;
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "component.create",
            "Create Component",
            ["Object"],
            Some("Cmd+Alt+K"),
            "{name?, ids?} make the selection (several objects are grouped first) the main component of a new component: it stays where it is, editable, and its instances follow it → {name, id}",
            has_selection,
            create
        ),
        cmd!(
            "component.detach",
            "Detach Instance",
            ["Object"],
            Some("Cmd+Alt+B"),
            "{ids?} turn the selected instances into plain art → {ids}",
            has_selection,
            detach
        ),
        cmd!(
            "component.goToMain",
            "Go to Main Component",
            ["Object"],
            None,
            "{id?} select the main component of the selected instance → {id, name}",
            has_selection,
            go_to_main
        ),
        cmd!(query "component.list", "Components", [], None, "{} → {components: [{name, main: id|null, instances}]}", has_doc, list),
    ]
}

/// The main components of `d`: (symbol name, node id), in document order.
fn mains(d: &Document) -> Vec<(String, NodeId)> {
    let mut v = vec![];
    d.walk(|n| {
        if let Some(name) = &n.extras().component {
            v.push((name.clone(), n.id));
        }
    });
    v
}

/// The main component of symbol `name`, if it is still in the document.
pub(crate) fn main_of(d: &Document, name: &str) -> Option<NodeId> {
    mains(d).into_iter().find(|(n, _)| n == name).map(|(_, id)| id)
}

/// The art a main component gives its symbol: itself without the main-component mark.
fn symbol_art(n: &Node) -> Node {
    let mut art = n.clone();
    art.extras_mut().component = None;
    art
}

/// Keep components consistent after an edit (`before`: the document before it):
/// copies of a main component become instances, and a changed main refreshes its symbol and
/// resizes its instances with it.
pub(crate) fn sync(before: &Document, d: &mut Document) {
    if d.symbols.is_empty() {
        return;
    }
    let found = mains(d);
    if found.is_empty() {
        return;
    }
    let mut by_name: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
    for (name, id) in found {
        by_name.entry(name).or_default().push(id);
    }
    for (name, ids) in by_name {
        // A main whose symbol is gone (deleted) stops being a main.
        if !d.symbols.iter().any(|s| s.name == name) {
            for id in ids {
                if let Some(n) = d.node_mut(id) {
                    n.extras_mut().component = None;
                }
            }
            continue;
        }
        // The main is the one that was a main before; the others are copies of it.
        let was_main = |id: &NodeId| before.node(*id).is_some_and(|n| n.extras().component.as_deref() == Some(name.as_str()));
        let main = ids.iter().copied().find(was_main).or_else(|| ids.first().copied());
        let Some(main) = main else { continue };
        for copy in ids.iter().copied().filter(|id| *id != main) {
            // Failing to convert a copy leaves it a plain object marked as a main; it is unmarked
            // so it can't fight the real main for the symbol.
            if to_instance(d, copy, &name).is_err()
                && let Some(n) = d.node_mut(copy)
            {
                n.extras_mut().component = None;
            }
        }
        let Some(node) = d.node(main).cloned() else { continue };
        if before.node(main).is_some_and(|b| *b == node) {
            continue;
        }
        let Ok((art, size, _)) = normalize(symbol_art(&node)) else { continue };
        let old = natural_size(d, &name);
        let art = d.reid(&art);
        if let Some(sym) = d.symbols.iter_mut().find(|s| s.name == name) {
            sym.art = Arc::new(art);
        }
        set_natural_size(d, &name, Some(size));
        // Instances grow and shrink with the main (about their centres).
        let (sx, sy) = (size.0 / old.0.max(1e-9), size.1 / old.1.max(1e-9));
        if (sx - 1.0).abs() > 1e-9 || (sy - 1.0).abs() > 1e-9 {
            for id in instances_of(d, Some(&name)) {
                if let Some(NodeKind::SymbolInstance { xf, .. }) = d.node_mut(id).map(|n| &mut n.kind) {
                    *xf *= Affine::scale_non_uniform(sx, sy);
                }
            }
        }
    }
}

/// Replace the copy `id` of a main component by an instance of `name` in the same place.
fn to_instance(d: &mut Document, id: NodeId, name: &str) -> Result<()> {
    let n = d.node(id).cloned().ok_or(EngineError::NoNode(id))?;
    let b = n.geometric_bounds().ok_or_else(|| EngineError::Other("the copy has no bounds".into()))?;
    let (par, idx, _) = d.position(id).ok_or(EngineError::NoNode(id))?;
    let mut inst =
        Node::new(id, NodeKind::SymbolInstance { symbol: name.into(), xf: place_xf(b.center(), (b.width().max(1.0), b.height().max(1.0))) });
    inst.name = n.name.clone();
    d.remove(id)?;
    d.insert(par, idx, inst)?;
    Ok(())
}

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = roots(s, p)?;
    if ids.is_empty() {
        return Err(EngineError::Other("nothing selected".into()));
    }
    let d = &s.doc()?.doc;
    if ids.iter().any(|id| d.node(*id).is_some_and(|n| n.extras().component.is_some())) {
        return Err(EngineError::Other("the selection is a main component already".into()));
    }
    let base = str_param(p, "name").filter(|n| !n.trim().is_empty()).map(str::to_string).unwrap_or_else(|| {
        // "Component 1", "Component 2"… as Figma names them.
        (1..).map(|i| format!("Component {i}")).find(|n| !d.symbols.iter().any(|s| s.name == *n)).unwrap_or_else(|| "Component".into())
    });
    let name = unique_symbol_name(d, &base);
    let (name, id) = s.edit("Create Component", |d, sel| {
        // Several objects are grouped first, in place of the topmost.
        let id = if let [one] = ids.as_slice() {
            *one
        } else {
            let top = *ids.last().ok_or_else(|| EngineError::Other("nothing selected".into()))?;
            let (par, idx, _) = d.position(top).ok_or(EngineError::NoNode(top))?;
            let children: Vec<Arc<Node>> = ids.iter().filter_map(|i| d.node(*i).cloned().map(Arc::new)).collect();
            let gid = d.alloc_id();
            let group = Node::group(gid, children);
            d.insert(par, idx + 1, group)?;
            for r in &ids {
                d.remove(*r)?;
            }
            gid
        };
        let node = d.node(id).cloned().ok_or(EngineError::NoNode(id))?;
        let (art, size, _) = normalize(symbol_art(&node))?;
        let art = d.reid(&art);
        d.symbols.push(Symbol { name: name.clone(), art: Arc::new(art) });
        set_natural_size(d, &name, Some(size));
        if let Some(n) = d.node_mut(id) {
            n.extras_mut().component = Some(name.clone());
            if n.name.is_none() {
                n.name = Some(name.clone());
            }
        }
        sel.set([id]);
        Ok((name.clone(), id))
    })?;
    Ok(json!({ "name": name, "id": id.0 }))
}

fn detach(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = selected_instances(s, p)?;
    if ids.is_empty() {
        return Err(EngineError::Other("select an instance".into()));
    }
    let out = s.edit("Detach Instance", |d, sel| {
        let out = break_links(d, &ids)?;
        sel.set(out.iter().copied());
        Ok(out)
    })?;
    Ok(json!({ "ids": out.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn go_to_main(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = selected_instances(s, p)?;
    let first = *ids.first().ok_or_else(|| EngineError::Other("select an instance".into()))?;
    let d = &s.doc()?.doc;
    let Some(NodeKind::SymbolInstance { symbol, .. }) = d.node(first).map(|n| &n.kind) else {
        return Err(EngineError::Other("select an instance".into()));
    };
    let name = symbol.clone();
    let main = main_of(d, &name).ok_or_else(|| EngineError::Other(format!("the main component of `{name}` isn't in the document")))?;
    s.select(|_, sel| sel.set([main]))?;
    Ok(json!({ "id": main.0, "name": name }))
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let components: Vec<Value> = d
        .symbols
        .iter()
        .map(|sym| json!({ "name": sym.name, "main": main_of(d, &sym.name).map(|i| i.0), "instances": instances_of(d, Some(&sym.name)).len() }))
        .collect();
    Ok(json!({ "components": components }))
}
