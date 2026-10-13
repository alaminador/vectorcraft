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
        cmd!(
            "component.combineVariants",
            "Combine as Variants",
            ["Object"],
            None,
            "{ids?, name?} put the selected main components in a component set (an auto layout frame): each becomes a variant with properties (Property 1 = Default, Variant2…) its instances can switch between → {id, name}",
            has_selection,
            combine_variants
        ),
        cmd!(
            "component.addVariant",
            "Add Variant",
            [],
            None,
            "{id? (a variant, or a component set: its last variant)} a copy of the variant as a new variant beside it, its first property given a new value → {id, name}",
            has_selection,
            add_variant
        ),
        cmd!(
            "component.setVariant",
            "Variant Properties",
            [],
            None,
            "{ids? (variants; default: the selection), props: {name: value}} set the variants' property values (a new property name adds it to every variant of the set) → {ids}",
            has_selection,
            set_variant
        ),
        cmd!(
            "component.renameProperty",
            "Rename Variant Property",
            [],
            None,
            "{id? (a component set or one of its variants; default: the selection), from, to} rename a property in every variant of the set → {set}",
            has_selection,
            rename_property
        ),
        cmd!(
            "component.setInstanceVariant",
            "Switch Variant",
            [],
            None,
            "{ids? (instances; default: the selection), props: {name: value}} switch instances to the variant of their set with these values (the rest kept, or the closest variant) → {ids, symbols}",
            has_selection,
            set_instance_variant
        ),
        cmd!(query "component.variants", "Variants", [], None, "{id? (a set, a variant or an instance of one; default: the selection)} → {set: id, name, properties: [{name, values}], variants: [{id, symbol, props}]}", has_doc, variants_query),
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

// ---------- variants ----------

/// A variant's properties, in order (`State=Hover`).
pub(crate) type Props = Vec<(String, String)>;
/// A variant of a set: its main component, symbol and properties.
pub(crate) type Variant = (NodeId, String, Props);

/// The component set holding main component `id`: its frame and name.
pub(crate) fn set_of(d: &Document, id: NodeId) -> Option<(NodeId, String)> {
    let n = d.node(id)?;
    if let Some(name) = &n.extras().component_set {
        return Some((id, name.clone()));
    }
    let p = d.parent_of(id)?;
    d.node(p)?.extras().component_set.clone().map(|name| (p, name))
}

/// The variants of set `set`: (main id, symbol, properties), in order.
pub(crate) fn variants_of(d: &Document, set: NodeId) -> Vec<Variant> {
    let Some(ch) = d.node(set).and_then(|n| n.children()) else { return vec![] };
    ch.iter().filter_map(|c| Some((c.id, c.extras().component.clone()?, c.extras().variant.clone().unwrap_or_default()))).collect()
}

/// The set a node belongs to: a set, a variant, or an instance of a variant.
fn set_for(d: &Document, id: NodeId) -> Option<(NodeId, String)> {
    if let Some(found) = set_of(d, id) {
        return Some(found);
    }
    let NodeKind::SymbolInstance { symbol, .. } = &d.node(id)?.kind else { return None };
    set_of(d, main_of(d, symbol)?)
}

/// The properties of a set in order, each with its values in order.
pub(crate) fn properties(vs: &[Variant]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    for (_, _, props) in vs {
        for (k, v) in props {
            match out.iter_mut().find(|(n, _)| n == k) {
                Some((_, vals)) => {
                    if !vals.contains(v) {
                        vals.push(v.clone());
                    }
                }
                None => out.push((k.clone(), vec![v.clone()])),
            }
        }
    }
    out
}

/// The variant of `vs` for `want`: an exact match, else the one matching every `must` key and
/// the most of the others.
pub(crate) fn pick_variant<'a>(vs: &'a [Variant], want: &[(String, String)], must: &[String]) -> Option<&'a Variant> {
    let value = |props: &[(String, String)], k: &str| props.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    vs.iter()
        .filter(|(_, _, props)| must.iter().all(|k| value(props, k) == value(want, k)))
        .max_by_key(|(_, _, props)| want.iter().filter(|(k, v)| value(props, k).as_deref() == Some(v.as_str())).count())
}

/// A symbol name for a variant: `Set / State=Hover, Size=L`, unique.
fn variant_symbol_name(d: &Document, set: &str, props: &[(String, String)]) -> String {
    let props = props.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ");
    unique_symbol_name(d, &format!("{set} / {props}"))
}

fn combine_variants(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = roots(s, p)?;
    let d = &s.doc()?.doc;
    let mains: Vec<NodeId> = ids.iter().copied().filter(|id| d.node(*id).is_some_and(|n| n.extras().component.is_some())).collect();
    if mains.is_empty() || mains.len() != ids.len() {
        return Err(EngineError::Other("select main components only (Create Component first)".into()));
    }
    if mains.iter().any(|id| set_of(d, *id).is_some_and(|(set, _)| set != *id)) {
        return Err(EngineError::Other("a selected component is in a component set already".into()));
    }
    let first_name = d.node(mains[0]).and_then(|n| n.extras().component.clone()).unwrap_or_else(|| "Component".into());
    let name = str_param(p, "name").filter(|n| !n.trim().is_empty()).map(str::to_string).unwrap_or(first_name);
    let bounds: Vec<logocraft_geom::Rect> = mains.iter().filter_map(|id| d.node(*id).and_then(|n| n.geometric_bounds())).collect();
    let union = bounds.iter().skip(1).fold(bounds.first().copied().unwrap_or(logocraft_geom::Rect::ZERO), |a, b| a.union(*b));
    let top = *mains.last().ok_or_else(|| EngineError::Other("nothing selected".into()))?;
    let id = s.edit("Combine as Variants", |d, sel| {
        let mut layout = logocraft_doc::layout::AutoLayout::new(logocraft_doc::layout::Direction::Horizontal, 20.0, union);
        layout.padding = [20.0; 4];
        let set = super::autolayout::wrap_in_frame(d, &mains, top, layout, &name)?;
        if let Some(n) = d.node_mut(set) {
            n.extras_mut().component_set = Some(name.clone());
        }
        let mut k = 1;
        for id in &mains {
            let Some(n) = d.node_mut(*id) else { continue };
            if n.extras().variant.is_none() {
                let value = if k == 1 { "Default".to_string() } else { format!("Variant{k}") };
                n.extras_mut().variant = Some(vec![("Property 1".into(), value)]);
            }
            k += 1;
        }
        sel.set([set]);
        Ok(set)
    })?;
    Ok(json!({ "id": id.0, "name": name }))
}

/// The variant a command on `id` means: a variant itself, or a set's last variant.
fn variant_target(d: &Document, id: NodeId) -> Option<NodeId> {
    let n = d.node(id)?;
    if n.extras().component.is_some() && set_of(d, id).is_some() {
        return Some(id);
    }
    n.extras().component_set.as_ref()?;
    variants_of(d, id).last().map(|(v, _, _)| *v)
}

fn add_variant(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = roots(s, p)?;
    let d = &s.doc()?.doc;
    let src = ids.first().and_then(|id| variant_target(d, *id)).ok_or_else(|| EngineError::Other("select a variant or a component set".into()))?;
    let (set, set_name) = set_of(d, src).ok_or_else(|| EngineError::Other("not in a component set".into()))?;
    let vs = variants_of(d, set);
    let mut props = d.node(src).and_then(|n| n.extras().variant.clone()).unwrap_or_default();
    if props.is_empty() {
        props.push(("Property 1".into(), "Default".into()));
    }
    // A new value for the first property: Variant2, Variant3… not yet used.
    let used: Vec<String> = vs.iter().filter_map(|(_, _, pr)| pr.first().map(|(_, v)| v.clone())).collect();
    let value = (2..).map(|k| format!("Variant{k}")).find(|v| !used.contains(v)).unwrap_or_else(|| "Variant".into());
    if let Some(first) = props.first_mut() {
        first.1 = value;
    }
    let name = variant_symbol_name(d, &set_name, &props);
    let (id, name) = s.edit("Add Variant", |d, sel| {
        let node = d.node(src).cloned().ok_or(EngineError::NoNode(src))?;
        let mut copy = d.reid(&node);
        copy.extras_mut().component = Some(name.clone());
        copy.extras_mut().variant = Some(props.clone());
        copy.name = Some(name.clone());
        let (art, size, _) = normalize(symbol_art(&copy))?;
        let art = d.reid(&art);
        d.symbols.push(Symbol { name: name.clone(), art: Arc::new(art) });
        set_natural_size(d, &name, Some(size));
        let id = copy.id;
        let (_, idx, _) = d.position(src).ok_or(EngineError::NoNode(src))?;
        d.insert(Some(set), idx + 1, copy)?;
        sel.set([id]);
        Ok((id, name.clone()))
    })?;
    Ok(json!({ "id": id.0, "name": name }))
}

/// `{name: value}` params as ordered pairs.
fn props_param(p: &Value, cmd: &str) -> Result<Vec<(String, String)>> {
    let obj = p.get("props").and_then(Value::as_object).ok_or_else(|| bad(cmd, "give props: {name: value}"))?;
    let mut out = vec![];
    for (k, v) in obj {
        let v = match v {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            _ => return Err(bad(cmd, format!("the value of {k} must be text"))),
        };
        let k = k.trim();
        if k.is_empty() || k.contains(['=', ',']) || v.contains([',', '=']) {
            return Err(bad(cmd, "property names and values can't be empty or hold = or ,"));
        }
        out.push((k.to_string(), v.trim().to_string()));
    }
    Ok(out)
}

fn set_variant(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "component.setVariant";
    let props = props_param(p, C)?;
    let ids = roots(s, p)?;
    let d = &s.doc()?.doc;
    let targets: Vec<(NodeId, NodeId)> =
        ids.iter().filter_map(|id| variant_target(d, *id).and_then(|v| set_of(d, v).map(|(set, _)| (v, set)))).collect();
    if targets.is_empty() {
        return Err(EngineError::Other("select a variant".into()));
    }
    s.edit("Variant Properties", |d, _| {
        for (v, set) in &targets {
            for (k, val) in &props {
                // A new property reaches every variant of the set, with this value.
                let all = variants_of(d, *set);
                for (other, _, pr) in all {
                    if !pr.iter().any(|(n, _)| n == k)
                        && let Some(n) = d.node_mut(other)
                    {
                        let mut pr = pr.clone();
                        pr.push((k.clone(), val.clone()));
                        n.extras_mut().variant = Some(pr);
                    }
                }
                if let Some(n) = d.node_mut(*v) {
                    let mut pr = n.extras().variant.clone().unwrap_or_default();
                    match pr.iter_mut().find(|(n, _)| n == k) {
                        Some(slot) => slot.1 = val.clone(),
                        None => pr.push((k.clone(), val.clone())),
                    }
                    n.extras_mut().variant = Some(pr);
                }
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": targets.iter().map(|(v, _)| v.0).collect::<Vec<_>>() }))
}

fn rename_property(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "component.renameProperty";
    let from = str_param(p, "from").ok_or_else(|| bad(C, "give from"))?.to_string();
    let to = str_param(p, "to")
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.contains(['=', ',']))
        .ok_or_else(|| bad(C, "give to (no = or ,)"))?
        .to_string();
    let ids = roots(s, p)?;
    let d = &s.doc()?.doc;
    let (set, _) = ids.iter().find_map(|id| set_for(d, *id)).ok_or_else(|| EngineError::Other("select a component set or a variant".into()))?;
    if variants_of(d, set).iter().any(|(_, _, pr)| pr.iter().any(|(k, _)| *k == to)) && from != to {
        return Err(EngineError::Other(format!("the set has a property `{to}` already")));
    }
    s.edit("Rename Variant Property", |d, _| {
        for (v, _, pr) in variants_of(d, set) {
            let renamed: Vec<(String, String)> = pr.into_iter().map(|(k, val)| (if k == from { to.clone() } else { k }, val)).collect();
            if let Some(n) = d.node_mut(v) {
                n.extras_mut().variant = Some(renamed);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "set": set.0 }))
}

fn set_instance_variant(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "component.setInstanceVariant";
    let changes = props_param(p, C)?;
    let ids = selected_instances(s, p)?;
    let d = &s.doc()?.doc;
    let must: Vec<String> = changes.iter().map(|(k, _)| k.clone()).collect();
    let mut plan: Vec<(NodeId, String)> = vec![];
    for id in &ids {
        let Some(NodeKind::SymbolInstance { symbol, .. }) = d.node(*id).map(|n| &n.kind) else { continue };
        let Some(main) = main_of(d, symbol) else { continue };
        let Some((set, _)) = set_of(d, main) else { continue };
        let vs = variants_of(d, set);
        let mut want = d.node(main).and_then(|n| n.extras().variant.clone()).unwrap_or_default();
        for (k, v) in &changes {
            match want.iter_mut().find(|(n, _)| n == k) {
                Some(slot) => slot.1 = v.clone(),
                None => want.push((k.clone(), v.clone())),
            }
        }
        let Some((_, target, _)) = pick_variant(&vs, &want, &must) else {
            return Err(EngineError::Other(format!(
                "no variant has {}",
                changes.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")
            )));
        };
        plan.push((*id, target.clone()));
    }
    if plan.is_empty() {
        return Err(EngineError::Other("select instances of a variant".into()));
    }
    s.edit("Switch Variant", |d, _| {
        for (id, symbol) in &plan {
            let size = natural_size(d, symbol);
            let Some(n) = d.node_mut(*id) else { continue };
            let centre = n.geometric_bounds().map(|b| b.center());
            if let (NodeKind::SymbolInstance { symbol: sym, xf }, Some(c)) = (&mut n.kind, centre) {
                *sym = symbol.clone();
                *xf = place_xf(c, size);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": plan.iter().map(|(i, _)| i.0).collect::<Vec<_>>(), "symbols": plan.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>() }))
}

fn variants_query(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = match ids_param(p, "ids").or_else(|| id_param(p, "id").map(|i| vec![i])) {
        Some(v) => v,
        None => s.doc()?.selection.objects.clone(),
    };
    let d = &s.doc()?.doc;
    let (set, name) = ids
        .iter()
        .find_map(|id| set_for(d, *id))
        .ok_or_else(|| EngineError::Other("select a component set, a variant or an instance of one".into()))?;
    let vs = variants_of(d, set);
    let props: Vec<Value> = properties(&vs).into_iter().map(|(k, vals)| json!({ "name": k, "values": vals })).collect();
    let list: Vec<Value> = vs
        .iter()
        .map(|(id, sym, pr)| json!({ "id": id.0, "symbol": sym, "props": pr.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>() }))
        .collect();
    Ok(json!({ "set": set.0, "name": name, "properties": props, "variants": list }))
}
