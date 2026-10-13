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
        cmd!(
            "component.addProperty",
            "Add Component Property",
            [],
            None,
            "{id? (the main component, or a layer in it; default: the selection), name, type: boolean|text|instanceSwap, default?: (the type's: true, the layer's text, the nested instance's component), layer?: id (a layer of the main to bind: its visibility, text or component)} add a component property; with a layer (or a selected inner layer) it drives that layer → {component, name}",
            has_selection,
            add_property
        ),
        cmd!(
            "component.editProperty",
            "Edit Component Property",
            [],
            None,
            "{id?, name, newName?, default?} rename a component property (instances keep their values) or change its default → {component, name}",
            has_selection,
            edit_property
        ),
        cmd!(
            "component.deleteProperty",
            "Delete Component Property",
            [],
            None,
            "{id?, name} remove a component property, its bindings and the instances' values → {component}",
            has_selection,
            delete_property
        ),
        cmd!(
            "component.bindProperty",
            "Bind Component Property",
            [],
            None,
            "{layer? (default: the selected layer inside a main component), property: name | null (unbind), field?: visible|characters|mainComponent (default: the property's)} make a property drive a layer of the main component → {layer}",
            has_selection,
            bind_property
        ),
        cmd!(
            "instance.setProperties",
            "Instance Properties",
            [],
            None,
            "{ids? (instances; default: the selection), properties: {name: value}} set instances' component property values (boolean, text, or a component name for an instance swap) → {ids}",
            has_selection,
            set_properties
        ),
        cmd!(
            "instance.setOverride",
            "Override Layer",
            [],
            None,
            "{ids? (instances), layer: id (a layer of the main component), fill?: colour|null, stroke?: colour|null, visible?: bool|null, characters?: text|null (null: back to the main's)} override one layer of instances → {ids}",
            has_selection,
            set_override
        ),
        cmd!(
            "component.resetOverrides",
            "Reset Overrides",
            [],
            None,
            "{ids? (instances), layer?: id (only that layer's overrides; omitted: every override and property value)} back to the main component's look → {ids}",
            has_selection,
            reset_overrides
        ),
        cmd!(query "component.properties", "Component Properties", [], None, "{id? (a main component, a layer in it or an instance; default: the selection)} → {component, main, props: [{name, type, default, value?}], layers: [{id, name, kind, fill?, visible, characters?, prop?, override?}]}", has_doc, properties_query),
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
    sync_mains(before, d);
    refresh_instances(before, d);
}

/// The main-component half of [`sync`].
fn sync_mains(before: &Document, d: &mut Document) {
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
    let Some(name) = d.node(first).and_then(base_of) else {
        return Err(EngineError::Other("select an instance".into()));
    };
    let main = main_of(d, &name).ok_or_else(|| EngineError::Other(format!("the main component of `{name}` isn't in the document")))?;
    s.select(|_, sel| sel.set([main]))?;
    Ok(json!({ "id": main.0, "name": name }))
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let components: Vec<Value> = d
        .symbols
        .iter()
        .filter(|sym| !is_derived(&sym.name))
        .map(|sym| json!({ "name": sym.name, "main": main_of(d, &sym.name).map(|i| i.0), "instances": instances_of_component(d, &sym.name).len() }))
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
    let base = base_of(d.node(id)?)?;
    set_of(d, main_of(d, &base)?)
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
        let Some(base) = d.node(*id).and_then(base_of) else { continue };
        let Some(main) = main_of(d, &base) else { continue };
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

// ---------- component properties and overrides ----------

/// Symbols made for instances with overrides start with this (hidden from the Components panel).
pub const DERIVED_PREFIX: &str = "\u{2042} ";

/// Is symbol `name` one made for an instance's overrides?
pub fn is_derived(name: &str) -> bool {
    name.starts_with(DERIVED_PREFIX)
}

/// The symbol made for instance `id` of component `base`.
fn derived_name(base: &str, id: NodeId) -> String {
    format!("{DERIVED_PREFIX}{base} #{}", id.0)
}

/// The component (its symbol) instance `n` is of.
pub fn base_of(n: &Node) -> Option<String> {
    let NodeKind::SymbolInstance { symbol, .. } = &n.kind else { return None };
    match n.extras().instance.as_deref() {
        Some(i) if is_derived(symbol) || *symbol == i.component => Some(i.component.clone()),
        _ => Some(symbol.clone()),
    }
}

/// The instances of component `name`, with or without overrides.
pub(crate) fn instances_of_component(d: &Document, name: &str) -> Vec<NodeId> {
    let mut v = vec![];
    d.walk(|n| {
        if base_of(n).as_deref() == Some(name) {
            v.push(n.id);
        }
    });
    v
}

/// `n` and its descendants, mutably.
fn each_mut(n: &mut Node, f: &mut dyn FnMut(&mut Node)) {
    f(n);
    if let Some(ch) = n.children_mut() {
        for c in ch {
            each_mut(Arc::make_mut(c), f);
        }
    }
}

/// The main component holding layer `id` (itself, or its nearest ancestor that is one).
fn owning_main(d: &Document, id: NodeId) -> Option<NodeId> {
    let mut cur = Some(id);
    while let Some(c) = cur {
        if d.node(c)?.extras().component.is_some() {
            return Some(c);
        }
        cur = d.parent_of(c);
    }
    None
}

/// The main component a command is about: the selected (or given) main, the main holding a
/// selected layer, or the main of a selected instance.
fn target_main(s: &Session, p: &Value) -> Result<NodeId> {
    let ids = match id_param(p, "id") {
        Some(i) => vec![i],
        None => s.doc()?.selection.objects.clone(),
    };
    let d = &s.doc()?.doc;
    ids.iter()
        .find_map(|id| owning_main(d, *id).or_else(|| d.node(*id).and_then(base_of).and_then(|b| main_of(d, &b))))
        .ok_or_else(|| EngineError::Other("select a main component (or a layer in it)".into()))
}

/// Text of a type node.
fn plain_text(n: &Node) -> Option<String> {
    match &n.kind {
        NodeKind::Text(t) => Some(t.runs.iter().map(|r| r.text.as_str()).collect()),
        _ => None,
    }
}

/// Set a node's fill (type: every character's) or stroke paint.
fn set_paint(n: &mut Node, paint: &logocraft_color::Paint, stroke: bool) {
    if let NodeKind::Text(t) = &mut n.kind {
        for r in &mut t.runs {
            if stroke {
                r.style.stroke = paint.clone();
            } else {
                r.style.fill = paint.clone();
            }
        }
        return;
    }
    if stroke {
        n.appearance.set_stroke(paint.clone());
    } else {
        n.appearance.set_fill(paint.clone());
    }
}

/// The main component's art with instance data `data` applied: property values through the
/// layers' bindings, then per-layer overrides.
fn apply_instance(d: &Document, main: &Node, data: &logocraft_doc::component::InstanceData) -> Node {
    let defs = main.extras().component_props.clone().unwrap_or_default();
    let value = |prop: &str| data.props.get(prop).cloned().or_else(|| defs.get(prop).map(|p| p.default_value()));
    let mut art = main.clone();
    each_mut(&mut art, &mut |n| {
        if let Some(refs) = n.extras().prop_refs.clone() {
            for (field, prop) in refs {
                let Some(v) = value(&prop) else { continue };
                match field.as_str() {
                    "visible" => {
                        if let Some(b) = v.as_bool() {
                            n.visible = b;
                        }
                    }
                    "characters" => {
                        if let (NodeKind::Text(t), Some(text)) = (&mut n.kind, v.as_str()) {
                            super::typecmd::replace_text(t, text);
                        }
                    }
                    "mainComponent" => {
                        if let (NodeKind::SymbolInstance { symbol, .. }, Some(name)) = (&mut n.kind, v.as_str())
                            && d.symbols.iter().any(|x| x.name == name)
                        {
                            *symbol = name.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(o) = data.overrides.get(&n.id.0) {
            if let Some(f) = &o.fill {
                set_paint(n, f, false);
            }
            if let Some(f) = &o.stroke {
                set_paint(n, f, true);
            }
            if let Some(v) = o.visible {
                n.visible = v;
            }
            if let (Some(text), NodeKind::Text(t)) = (&o.characters, &mut n.kind) {
                super::typecmd::replace_text(t, text);
            }
        }
    });
    art
}

/// Does `data` change anything from the main's look?
fn changes_anything(main: &Node, data: &logocraft_doc::component::InstanceData) -> bool {
    let defs = main.extras().component_props.clone().unwrap_or_default();
    let props = data.props.iter().any(|(k, v)| defs.get(k).is_some_and(|p| p.default_value() != *v));
    props || data.overrides.values().any(|o| !o.is_empty())
}

/// Keep instances with property values or overrides showing them: each gets a symbol of its own
/// made from its main component (rebuilt when the main, the instance's data or the symbol
/// changes), framed like the main's so it sits where the component's would; instances back to the
/// main's look show the component's symbol again. A symbol an instance was swapped to makes it
/// an instance of that component. Symbols no instance shows any more go.
fn refresh_instances(before: &Document, d: &mut Document) {
    let mut ids = vec![];
    d.walk(|n| {
        if matches!(n.kind, NodeKind::SymbolInstance { .. }) && (n.extras().instance.is_some() || n.extras().component.is_none()) {
            ids.push(n.id);
        }
    });
    let mut used: Vec<String> = vec![];
    for id in ids {
        let Some(n) = d.node(id) else { continue };
        let NodeKind::SymbolInstance { symbol, .. } = &n.kind else { continue };
        let symbol = symbol.clone();
        let Some(mut data) = n.extras().instance.as_deref().cloned() else {
            continue;
        };
        // Swapped (Swap Instance, a variant switch): an instance of what it shows now, its
        // property values kept by name, its layer overrides (another component's layers) gone.
        if !is_derived(&symbol) && symbol != data.component {
            data.component = symbol.clone();
            data.overrides.clear();
            if let Some(n) = d.node_mut(id) {
                n.extras_mut().instance = Some(Box::new(data.clone()));
            }
        }
        let base = data.component.clone();
        let Some(main) = main_of(d, &base).and_then(|m| d.node(m)).cloned() else {
            used.push(symbol);
            continue;
        };
        let derived = derived_name(&base, id);
        if !changes_anything(&main, &data) {
            if symbol != base
                && let Some(NodeKind::SymbolInstance { symbol: s, .. }) = d.node_mut(id).map(|n| &mut n.kind)
            {
                *s = base.clone();
            }
            continue;
        }
        used.push(derived.clone());
        let main_changed = before.node(main.id) != Some(&main);
        let data_changed = before.node(id).and_then(|b| b.extras().instance.as_deref().cloned()) != Some(data.clone());
        let missing = !d.symbols.iter().any(|x| x.name == derived);
        if main_changed || data_changed || missing {
            let Some(b) = main.geometric_bounds() else { continue };
            let mut art = symbol_art(&apply_instance(d, &main, &data));
            each_mut(&mut art, &mut |n| {
                n.extras_mut().component = None;
                n.extras_mut().prop_refs = None;
            });
            // Framed by the main's box: the instance's transform places it as the main's.
            let (w, h) = (b.width().max(1.0), b.height().max(1.0));
            let half = logocraft_doc::SYMBOL_HALF;
            art.transform(Affine::scale_non_uniform(2.0 * half / w, 2.0 * half / h) * Affine::translate(-b.center().to_vec2()), false);
            let art = Arc::new(d.reid(&art));
            match d.symbols.iter_mut().find(|x| x.name == derived) {
                Some(sym) => sym.art = art,
                None => d.symbols.push(Symbol { name: derived.clone(), art }),
            }
            let size = natural_size(d, &base);
            set_natural_size(d, &derived, Some(size));
        }
        if symbol != derived
            && let Some(NodeKind::SymbolInstance { symbol: s, .. }) = d.node_mut(id).map(|n| &mut n.kind)
        {
            *s = derived;
        }
    }
    // Derived symbols nothing shows any more.
    let mut shown: Vec<String> = used;
    d.walk(|n| {
        if let NodeKind::SymbolInstance { symbol, .. } = &n.kind {
            shown.push(symbol.clone());
        }
    });
    let gone: Vec<String> = d.symbols.iter().filter(|x| is_derived(&x.name) && !shown.contains(&x.name)).map(|x| x.name.clone()).collect();
    if !gone.is_empty() {
        d.symbols.retain(|x| !gone.contains(&x.name));
        for g in gone {
            set_natural_size(d, &g, None);
        }
    }
}

fn prop_name(p: &Value, key: &str, cmd: &str) -> Result<String> {
    str_param(p, key).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).ok_or_else(|| bad(cmd, format!("give `{key}`")))
}

fn add_property(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "component.addProperty";
    let main = target_main(s, p)?;
    let name = prop_name(p, "name", C)?;
    let d = &s.doc()?.doc;
    // The layer to bind: given, or a selected layer inside the main.
    let layer = id_param(p, "layer").or_else(|| {
        let sel = &s.doc().ok()?.selection.objects;
        sel.iter().copied().find(|id| *id != main && owning_main(d, *id) == Some(main))
    });
    let layer_node = layer.and_then(|l| d.node(l)).cloned();
    if let Some(l) = layer
        && owning_main(d, l) != Some(main)
    {
        return Err(bad(C, "the layer isn't in that main component"));
    }
    let kind = str_param(p, "type").map(str::to_string).unwrap_or_else(|| match layer_node.as_ref().map(|n| &n.kind) {
        Some(NodeKind::Text(_)) => "text".into(),
        Some(NodeKind::SymbolInstance { .. }) => "instanceSwap".into(),
        _ => "boolean".into(),
    });
    let def = match kind.as_str() {
        "boolean" => logocraft_doc::component::PropDef::Boolean { default: p.get("default").and_then(Value::as_bool).unwrap_or(true) },
        "text" => logocraft_doc::component::PropDef::Text {
            default: str_param(p, "default").map(str::to_string).or_else(|| layer_node.as_ref().and_then(plain_text)).unwrap_or_else(|| name.clone()),
        },
        "instanceSwap" => {
            let default = str_param(p, "default").map(str::to_string).or_else(|| match layer_node.as_ref().map(|n| &n.kind) {
                Some(NodeKind::SymbolInstance { symbol, .. }) => Some(symbol.clone()),
                _ => None,
            });
            let default = default.ok_or_else(|| bad(C, "an instance swap property needs a nested instance or a default component"))?;
            logocraft_doc::component::PropDef::InstanceSwap { default }
        }
        o => return Err(bad(C, format!("type must be boolean, text or instanceSwap, not `{o}`"))),
    };
    if let Some(n) = &layer_node {
        let fits = match &def {
            logocraft_doc::component::PropDef::Text { .. } => matches!(n.kind, NodeKind::Text(_)),
            logocraft_doc::component::PropDef::InstanceSwap { .. } => matches!(n.kind, NodeKind::SymbolInstance { .. }),
            logocraft_doc::component::PropDef::Boolean { .. } => true,
        };
        if !fits {
            return Err(bad(C, format!("a {} property can't drive that layer", def.type_name())));
        }
    }
    if d.node(main).and_then(|n| n.extras().component_props.as_ref()).is_some_and(|m| m.contains_key(&name)) {
        return Err(EngineError::Other(format!("the component has a property `{name}` already")));
    }
    let comp = d.node(main).and_then(|n| n.extras().component.clone()).unwrap_or_default();
    s.edit("Add Component Property", |d, _| {
        let field = def.field().to_string();
        if let Some(n) = d.node_mut(main) {
            let mut m = n.extras().component_props.clone().unwrap_or_default();
            m.insert(name.clone(), def.clone());
            n.extras_mut().component_props = Some(m);
        }
        if let Some(l) = layer
            && let Some(n) = d.node_mut(l)
        {
            let mut refs = n.extras().prop_refs.clone().unwrap_or_default();
            refs.insert(field, name.clone());
            n.extras_mut().prop_refs = Some(refs);
        }
        Ok(())
    })?;
    Ok(json!({ "component": comp, "name": name }))
}

fn edit_property(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "component.editProperty";
    let main = target_main(s, p)?;
    let name = prop_name(p, "name", C)?;
    let new_name = str_param(p, "newName").map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    let d = &s.doc()?.doc;
    let defs = d.node(main).and_then(|n| n.extras().component_props.clone()).unwrap_or_default();
    let mut def = defs.get(&name).cloned().ok_or_else(|| EngineError::Other(format!("no property `{name}`")))?;
    if let Some(v) = p.get("default") {
        match &mut def {
            logocraft_doc::component::PropDef::Boolean { default } => {
                *default = v.as_bool().ok_or_else(|| bad(C, "default must be true or false"))?
            }
            logocraft_doc::component::PropDef::Text { default } | logocraft_doc::component::PropDef::InstanceSwap { default } => {
                *default = v.as_str().ok_or_else(|| bad(C, "default must be text"))?.to_string()
            }
        }
    }
    if let Some(n) = &new_name
        && *n != name
        && defs.contains_key(n)
    {
        return Err(EngineError::Other(format!("the component has a property `{n}` already")));
    }
    let comp = d.node(main).and_then(|n| n.extras().component.clone()).unwrap_or_default();
    let final_name = new_name.clone().unwrap_or_else(|| name.clone());
    s.edit("Edit Component Property", |d, _| {
        if let Some(n) = d.node_mut(main) {
            let mut m = n.extras().component_props.clone().unwrap_or_default();
            m.remove(&name);
            m.insert(final_name.clone(), def.clone());
            n.extras_mut().component_props = Some(m);
        }
        if final_name != name {
            rename_refs(d, main, &comp, &name, Some(&final_name));
        }
        Ok(())
    })?;
    Ok(json!({ "component": comp, "name": final_name }))
}

/// Rename (or with `to` None remove) property `from` in main `main`'s bindings and its instances'
/// values.
fn rename_refs(d: &mut Document, main: NodeId, comp: &str, from: &str, to: Option<&str>) {
    if let Some(n) = d.node_mut(main) {
        each_mut(n, &mut |l| {
            if let Some(mut refs) = l.extras().prop_refs.clone() {
                let before = refs.clone();
                refs.retain(|_, p| p != from || to.is_some());
                for p in refs.values_mut() {
                    if p == from
                        && let Some(t) = to
                    {
                        *p = t.to_string();
                    }
                }
                if refs != before {
                    l.extras_mut().prop_refs = (!refs.is_empty()).then_some(refs);
                }
            }
        });
    }
    for id in instances_of_component(d, comp) {
        if let Some(n) = d.node_mut(id)
            && let Some(mut data) = n.extras().instance.as_deref().cloned()
            && let Some(v) = data.props.remove(from)
        {
            if let Some(t) = to {
                data.props.insert(t.to_string(), v);
            }
            n.extras_mut().instance = Some(Box::new(data));
        }
    }
}

fn delete_property(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "component.deleteProperty";
    let main = target_main(s, p)?;
    let name = prop_name(p, "name", C)?;
    let d = &s.doc()?.doc;
    if !d.node(main).and_then(|n| n.extras().component_props.as_ref()).is_some_and(|m| m.contains_key(&name)) {
        return Err(EngineError::Other(format!("no property `{name}`")));
    }
    let comp = d.node(main).and_then(|n| n.extras().component.clone()).unwrap_or_default();
    s.edit("Delete Component Property", |d, _| {
        if let Some(n) = d.node_mut(main) {
            let mut m = n.extras().component_props.clone().unwrap_or_default();
            m.remove(&name);
            n.extras_mut().component_props = (!m.is_empty()).then_some(m);
        }
        rename_refs(d, main, &comp, &name, None);
        Ok(())
    })?;
    Ok(json!({ "component": comp }))
}

fn bind_property(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "component.bindProperty";
    let d = &s.doc()?.doc;
    let layer = match id_param(p, "layer") {
        Some(l) => l,
        None => *s.doc()?.selection.objects.first().ok_or_else(|| bad(C, "select a layer inside a main component"))?,
    };
    let main = owning_main(d, layer).filter(|m| *m != layer).ok_or_else(|| bad(C, "the layer isn't inside a main component"))?;
    let defs = d.node(main).and_then(|n| n.extras().component_props.clone()).unwrap_or_default();
    let prop = match p.get("property") {
        Some(Value::Null) => None,
        _ => Some(prop_name(p, "property", C)?),
    };
    let field = match (str_param(p, "field"), &prop) {
        (Some(f), _) => f.to_string(),
        (None, Some(name)) => defs.get(name).map(|d| d.field().to_string()).ok_or_else(|| EngineError::Other(format!("no property `{name}`")))?,
        (None, None) => return Err(bad(C, "give `field` to unbind")),
    };
    if !matches!(field.as_str(), "visible" | "characters" | "mainComponent") {
        return Err(bad(C, "field must be visible, characters or mainComponent"));
    }
    if let Some(name) = &prop
        && defs.get(name).map(|d| d.field()) != Some(field.as_str())
    {
        return Err(bad(C, format!("`{name}` can't drive `{field}`")));
    }
    // Text drives type layers, an instance swap nested instances.
    let fits = match (field.as_str(), d.node(layer).map(|n| &n.kind)) {
        ("characters", Some(NodeKind::Text(_))) | ("mainComponent", Some(NodeKind::SymbolInstance { .. })) | ("visible", Some(_)) => true,
        _ => prop.is_none(),
    };
    if !fits {
        return Err(bad(C, format!("a layer like this has no `{field}` to drive")));
    }
    s.edit("Bind Component Property", |d, _| {
        if let Some(n) = d.node_mut(layer) {
            let mut refs = n.extras().prop_refs.clone().unwrap_or_default();
            match &prop {
                Some(name) => refs.insert(field.clone(), name.clone()),
                None => refs.remove(&field),
            };
            n.extras_mut().prop_refs = (!refs.is_empty()).then_some(refs);
        }
        Ok(())
    })?;
    Ok(json!({ "layer": layer.0 }))
}

/// The instances a command is about, each with its component and data (made when it has none).
fn instance_targets(s: &Session, p: &Value) -> Result<Vec<(NodeId, String, logocraft_doc::component::InstanceData)>> {
    let ids = selected_instances(s, p)?;
    let d = &s.doc()?.doc;
    let out: Vec<_> = ids
        .iter()
        .filter_map(|id| {
            let n = d.node(*id)?;
            let base = base_of(n)?;
            let data = n
                .extras()
                .instance
                .as_deref()
                .cloned()
                .unwrap_or_else(|| logocraft_doc::component::InstanceData { component: base.clone(), ..Default::default() });
            Some((*id, base, data))
        })
        .collect();
    if out.is_empty() {
        return Err(EngineError::Other("select an instance".into()));
    }
    Ok(out)
}

fn write_instances(s: &mut Session, label: &str, targets: Vec<(NodeId, String, logocraft_doc::component::InstanceData)>) -> Result<Value> {
    let ids: Vec<u64> = targets.iter().map(|(i, ..)| i.0).collect();
    s.edit(label, |d, _| {
        for (id, _, data) in targets {
            if let Some(n) = d.node_mut(id) {
                n.extras_mut().instance = Some(Box::new(data));
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids }))
}

fn set_properties(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "instance.setProperties";
    let props = p.get("properties").and_then(Value::as_object).ok_or_else(|| bad(C, "give properties: {name: value}"))?.clone();
    let mut targets = instance_targets(s, p)?;
    let d = &s.doc()?.doc;
    for (_, base, data) in &mut targets {
        let defs = main_of(d, base).and_then(|m| d.node(m)).and_then(|n| n.extras().component_props.clone()).unwrap_or_default();
        for (k, v) in &props {
            let def = defs.get(k).ok_or_else(|| EngineError::Other(format!("`{base}` has no property `{k}`")))?;
            let v = match def {
                logocraft_doc::component::PropDef::Boolean { .. } => {
                    Value::Bool(v.as_bool().ok_or_else(|| bad(C, format!("{k} takes true or false")))?)
                }
                logocraft_doc::component::PropDef::Text { .. } => Value::String(match v {
                    Value::String(t) => t.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => return Err(bad(C, format!("{k} takes text"))),
                }),
                logocraft_doc::component::PropDef::InstanceSwap { .. } => {
                    let name = v.as_str().ok_or_else(|| bad(C, format!("{k} takes a component name")))?;
                    if !d.symbols.iter().any(|x| x.name == name && !is_derived(&x.name)) {
                        return Err(EngineError::Other(format!("no component `{name}`")));
                    }
                    Value::String(name.to_string())
                }
            };
            if def.default_value() == v {
                data.props.remove(k);
            } else {
                data.props.insert(k.clone(), v);
            }
        }
    }
    write_instances(s, "Instance Properties", targets)
}

/// A colour override param: a colour, or null (back to the main's).
fn paint_or_reset(p: &Value, key: &str, cmd: &str) -> Result<Option<Option<logocraft_color::Paint>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) if s == "none" => Ok(Some(Some(logocraft_color::Paint::None))),
        Some(v) => super::color_value(v)
            .map(|c| Some(Some(logocraft_color::Paint::solid(c))))
            .ok_or_else(|| bad(cmd, format!("{key} must be a colour or null"))),
    }
}

fn set_override(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "instance.setOverride";
    let layer = id_param(p, "layer").ok_or_else(|| bad(C, "give `layer` (a layer of the main component)"))?;
    let fill = paint_or_reset(p, "fill", C)?;
    let stroke = paint_or_reset(p, "stroke", C)?;
    let visible = match p.get("visible") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(v.as_bool().ok_or_else(|| bad(C, "visible must be true, false or null"))?)),
    };
    let characters = match p.get("characters") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(v.as_str().ok_or_else(|| bad(C, "characters must be text or null"))?.to_string())),
    };
    let mut targets = instance_targets(s, p)?;
    let d = &s.doc()?.doc;
    for (_, base, data) in &mut targets {
        let main = main_of(d, base).ok_or_else(|| EngineError::Other(format!("the main component of `{base}` isn't in the document")))?;
        if owning_main(d, layer) != Some(main) {
            return Err(bad(C, "that layer isn't in the instance's main component"));
        }
        let o = data.overrides.entry(layer.0).or_default();
        if let Some(v) = fill.clone() {
            o.fill = v;
        }
        if let Some(v) = stroke.clone() {
            o.stroke = v;
        }
        if let Some(v) = visible {
            o.visible = v;
        }
        if let Some(v) = characters.clone() {
            o.characters = v;
        }
        if o.is_empty() {
            data.overrides.remove(&layer.0);
        }
    }
    write_instances(s, "Override Layer", targets)
}

fn reset_overrides(s: &mut Session, p: &Value) -> Result<Value> {
    let layer = id_param(p, "layer");
    let mut targets = instance_targets(s, p)?;
    for (_, _, data) in &mut targets {
        match layer {
            Some(l) => {
                data.overrides.remove(&l.0);
            }
            None => {
                data.overrides.clear();
                data.props.clear();
            }
        }
    }
    write_instances(s, "Reset Overrides", targets)
}

fn properties_query(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = match id_param(p, "id") {
        Some(i) => vec![i],
        None => s.doc()?.selection.objects.clone(),
    };
    let d = &s.doc()?.doc;
    let first = *ids.first().ok_or_else(|| EngineError::Other("select a component or an instance".into()))?;
    let n = d.node(first).ok_or(EngineError::NoNode(first))?;
    let data = n.extras().instance.as_deref().cloned();
    let main = match base_of(n) {
        Some(b) => main_of(d, &b),
        None => owning_main(d, first),
    }
    .ok_or_else(|| EngineError::Other("select a component (or a layer in it) or an instance".into()))?;
    let m = d.node(main).ok_or(EngineError::NoNode(main))?;
    let defs = m.extras().component_props.clone().unwrap_or_default();
    let props: Vec<Value> = defs
        .iter()
        .map(|(k, def)| {
            let mut v = json!({ "name": k, "type": def.type_name(), "default": def.default_value() });
            if let Some(data) = &data {
                v["value"] = data.props.get(k).cloned().unwrap_or_else(|| def.default_value());
            }
            v
        })
        .collect();
    let mut layers = vec![];
    m.walk(&mut |l: &Node| {
        if l.id == main || l.is_container() && !matches!(l.kind, NodeKind::Compound { .. }) {
            return;
        }
        let fill = match &l.kind {
            NodeKind::Text(t) => t.runs.first().map(|r| r.style.fill.clone()),
            _ => Some(l.appearance.fill_paint()),
        };
        let mut v = json!({
            "id": l.id.0,
            "name": l.name.clone().unwrap_or_else(|| l.kind_label().to_string()),
            "kind": l.kind_label(),
            "visible": l.visible,
        });
        if let Some(c) = fill.as_ref().and_then(|f| f.color()) {
            v["fill"] = json!(c.to_hex());
        }
        if let Some(t) = plain_text(l) {
            v["characters"] = json!(t);
        }
        if let Some(refs) = &l.extras().prop_refs {
            v["prop"] = json!(refs);
        }
        if let Some(o) = data.as_ref().and_then(|d| d.overrides.get(&l.id.0)) {
            v["override"] = serde_json::to_value(o).unwrap_or(Value::Null);
        }
        layers.push(v);
    });
    Ok(json!({ "component": m.extras().component, "main": main.0, "props": props, "layers": layers }))
}
