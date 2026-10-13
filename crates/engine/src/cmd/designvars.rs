//! Design variables (Figma's; [`logocraft_doc::design_vars`]), modelled on FigCraft's: collections
//! with modes, colour / number / text / boolean variables and aliases, layer properties bound to
//! them, and modes picked per container. Bound values are written into the layers after every edit
//! ([`apply`]), each layer resolving in the modes its containers pick, so rendering and export read
//! plain values. A binding is not an override: an instance's bound layers resolve in the
//! instance's modes.

use std::collections::BTreeMap;
use std::sync::Arc;

use logocraft_color::Paint;
use logocraft_doc::design_vars::{Collection, Mode, Variable, VariableType, VariableValue};
use logocraft_doc::{Document, LiveShape, Node, NodeId, NodeKind};
use serde_json::{Value, json};

use super::*;
use crate::EngineError;

/// The fields a layer can bind, with the variable type each takes.
pub const FIELDS: [(&str, VariableType); 9] = [
    ("fill", VariableType::Color),
    ("stroke", VariableType::Color),
    ("strokeWeight", VariableType::Number),
    ("opacity", VariableType::Number),
    ("cornerRadius", VariableType::Number),
    ("gap", VariableType::Number),
    ("padding", VariableType::Number),
    ("characters", VariableType::String),
    ("visible", VariableType::Boolean),
];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "variable.createCollection",
            "New Variable Collection",
            [],
            None,
            "{name, modes?: [names] (default: [\"Mode 1\"])} a collection of design variables → {id}",
            has_doc,
            create_collection
        ),
        cmd!("variable.renameCollection", "Rename Collection", [], None, "{collection, name}", has_doc, rename_collection),
        cmd!(
            "variable.deleteCollection",
            "Delete Collection",
            [],
            None,
            "{collection} (its variables go; bound layers keep their values)",
            has_doc,
            delete_collection
        ),
        cmd!(
            "variable.addMode",
            "Add Mode",
            [],
            None,
            "{collection, name} a mode whose values start as the default mode's → {id}",
            has_doc,
            add_mode
        ),
        cmd!("variable.renameMode", "Rename Mode", [], None, "{collection, mode, name}", has_doc, rename_mode),
        cmd!("variable.removeMode", "Remove Mode", [], None, "{collection, mode} (layers on it go back to the default)", has_doc, remove_mode),
        cmd!(
            "variable.create",
            "New Variable",
            [],
            None,
            "{collection (id or name; made when missing), name, type: color|number|string|boolean, value? (every mode), values?: {mode name or id: value}} → {id}",
            has_doc,
            create
        ),
        cmd!(
            "variable.set",
            "Variable Value",
            [],
            None,
            "{variable (id or name), mode? (id or name; default: the default mode), value? | alias?: variable} set a variable's value in one mode, or make it an alias",
            has_doc,
            set
        ),
        cmd!("variable.rename", "Rename Variable", [], None, "{variable, name}", has_doc, rename),
        cmd!(
            "variable.delete",
            "Delete Variable",
            [],
            None,
            "{variable} (bound layers keep their last value; aliases to it take its value)",
            has_doc,
            delete
        ),
        cmd!(
            "variable.bind",
            "Bind Variable",
            [],
            None,
            "{ids?, field: fill|stroke|strokeWeight|opacity|cornerRadius|gap|padding|characters|visible, variable: id or name | null (unbind; the value stays)} bind a property of the selected layers to a variable of its type",
            has_selection,
            bind
        ),
        cmd!(
            "variable.setMode",
            "Variable Mode",
            [],
            None,
            "{ids? (containers, instances; default: the selection), collection, mode: id or name | null (Auto: the parent's)} pick the mode of a collection for everything inside",
            has_selection,
            set_mode
        ),
        cmd!(query "variable.list", "Variables", [], None, "{} → {collections: [{id, name, modes: [{id, name}]}], variables: [{id, name, collection, type, values: {mode: value}}]}", has_doc, list),
    ]
}

// ---------- resolving ----------

/// The modes in effect for node `id`: its containers' picks, the nearest winning.
pub fn modes_for(d: &Document, id: NodeId) -> BTreeMap<String, String> {
    let mut chain = vec![id];
    let mut cur = d.parent_of(id);
    while let Some(c) = cur {
        chain.push(c);
        cur = d.parent_of(c);
    }
    let mut out = BTreeMap::new();
    for c in chain.into_iter().rev() {
        if let Some(m) = d.node(c).and_then(|n| n.extras().var_modes.clone()) {
            out.extend(m);
        }
    }
    out
}

/// Write the values bound to `n`'s fields (resolved in `modes`) into it. → whether it changed.
fn apply_node(vars: &logocraft_doc::design_vars::DesignVariables, n: &mut Node, modes: &BTreeMap<String, String>) -> bool {
    let Some(bound) = n.extras().bound.clone() else { return false };
    let before = n.clone();
    for (field, var) in bound {
        let Some(v) = vars.resolve(&var, modes) else { continue };
        match (field.as_str(), v) {
            ("fill", VariableValue::Color(c)) => set_paint(n, Paint::solid(c), false),
            ("stroke", VariableValue::Color(c)) => set_paint(n, Paint::solid(c), true),
            ("strokeWeight", VariableValue::Number(w)) if w.is_finite() => {
                if let Some(s) = n.appearance.stroke_mut() {
                    s.width = w.max(0.0);
                }
            }
            ("opacity", VariableValue::Number(o)) if o.is_finite() => n.opacity = (o / 100.0).clamp(0.0, 1.0) as f32,
            ("cornerRadius", VariableValue::Number(r)) if r.is_finite() => {
                if let NodeKind::Path { path, live: Some(l), .. } = &mut n.kind
                    && let LiveShape::Rectangle { radii, .. } = &mut **l
                {
                    *radii = [r.max(0.0); 4];
                    *path = l.to_path();
                }
            }
            ("gap", VariableValue::Number(g)) if g.is_finite() => {
                if let Some(l) = n.extras_if_any().and_then(|e| e.layout.as_mut()) {
                    l.gap = g;
                }
            }
            ("padding", VariableValue::Number(p)) if p.is_finite() => {
                if let Some(l) = n.extras_if_any().and_then(|e| e.layout.as_mut()) {
                    l.padding = [p.max(0.0); 4];
                }
            }
            ("characters", v) => {
                let text = match v {
                    VariableValue::String(s) => s,
                    VariableValue::Number(x) => trim_num(x),
                    _ => continue,
                };
                if let NodeKind::Text(t) = &mut n.kind {
                    let cur: String = t.runs.iter().map(|r| r.text.as_str()).collect();
                    if cur != text {
                        super::typecmd::replace_text(t, &text);
                    }
                }
            }
            ("visible", VariableValue::Boolean(b)) => n.visible = b,
            _ => {}
        }
    }
    *n != before
}

fn trim_num(x: f64) -> String {
    let s = format!("{x:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Set a node's fill (type: every character's) or stroke paint, when it differs.
fn set_paint(n: &mut Node, paint: Paint, stroke: bool) {
    if let NodeKind::Text(t) = &mut n.kind {
        for r in &mut t.runs {
            let slot = if stroke { &mut r.style.stroke } else { &mut r.style.fill };
            if *slot != paint {
                *slot = paint.clone();
            }
        }
        return;
    }
    let cur = if stroke { n.appearance.stroke_paint() } else { n.appearance.fill_paint() };
    if cur != paint {
        if stroke {
            n.appearance.set_stroke(paint);
        } else {
            n.appearance.set_fill(paint);
        }
    }
}

/// Write the bound values of `n` and everything inside it, resolved in `modes` plus the modes its
/// containers inside pick (an instance's art, before it becomes the instance's symbol).
pub(crate) fn apply_tree(vars: &logocraft_doc::design_vars::DesignVariables, n: &mut Node, modes: &BTreeMap<String, String>) {
    let mut modes = modes.clone();
    if let Some(m) = n.extras().var_modes.clone() {
        modes.extend(m);
    }
    apply_node(vars, n, &modes);
    if let Some(ch) = n.children_mut() {
        for c in ch {
            apply_tree(vars, Arc::make_mut(c), &modes);
        }
    }
}

/// Does `n` or anything inside it bind a variable?
pub(crate) fn binds_any(n: &Node) -> bool {
    let mut any = false;
    n.walk(&mut |l: &Node| any |= l.extras().bound.is_some());
    any
}

/// After an edit: write every bound value, each layer in its own modes. Free when the document
/// has no variables.
pub(crate) fn apply(d: &mut Document) {
    if d.design_variables.variables.is_empty() {
        return;
    }
    let mut ids = vec![];
    d.walk(|n| {
        if n.extras().bound.is_some() {
            ids.push(n.id);
        }
    });
    let vars = d.design_variables.clone();
    for id in ids {
        let modes = modes_for(d, id);
        // Only through `node_mut` when something changes: unchanged layers stay shared.
        let Some(n) = d.node(id) else { continue };
        let mut probe = n.clone();
        if apply_node(&vars, &mut probe, &modes)
            && let Some(m) = d.node_mut(id)
        {
            *m = probe;
        }
    }
}

// ---------- commands ----------

fn vars_mut(d: &mut Document) -> &mut logocraft_doc::design_vars::DesignVariables {
    &mut d.design_variables
}

/// A fresh id with `prefix`.
fn fresh(taken: &dyn Fn(&str) -> bool, prefix: &str) -> String {
    (1..).map(|i| format!("{prefix}{i}")).find(|c| !taken(c)).unwrap_or_else(|| prefix.to_string())
}

fn name_param(p: &Value, key: &str, cmd: &str) -> Result<String> {
    str_param(p, key).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).ok_or_else(|| bad(cmd, format!("give `{key}`")))
}

fn collection_id(d: &Document, p: &Value, cmd: &str) -> Result<String> {
    let key = name_param(p, "collection", cmd)?;
    d.design_variables.find_collection(&key).map(|c| c.id.clone()).ok_or_else(|| EngineError::Other(format!("no collection `{key}`")))
}

fn variable_id(d: &Document, key: &str) -> Result<String> {
    d.design_variables.find(key).map(|v| v.id.clone()).ok_or_else(|| EngineError::Other(format!("no variable `{key}`")))
}

fn mode_id(c: &Collection, key: &str) -> Option<String> {
    c.modes.iter().find(|m| m.id == key || m.name == key).map(|m| m.id.clone())
}

/// A JSON value as a value of type `kind`.
fn value_of(kind: VariableType, v: &Value, cmd: &str) -> Result<VariableValue> {
    Ok(match kind {
        VariableType::Color => VariableValue::Color(super::color_value(v).ok_or_else(|| bad(cmd, "a colour variable takes a colour"))?),
        VariableType::Number => {
            VariableValue::Number(v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(cmd, "a number variable takes a number"))?)
        }
        VariableType::String => VariableValue::String(match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(bad(cmd, "a string variable takes text")),
        }),
        VariableType::Boolean => VariableValue::Boolean(v.as_bool().ok_or_else(|| bad(cmd, "a boolean variable takes true or false"))?),
    })
}

/// A value as JSON (a colour as `#rrggbb`, or `{c,m,y,k}` for CMYK).
pub fn value_json(v: &VariableValue) -> Value {
    match v {
        VariableValue::Color(c @ logocraft_color::Color::Cmyk { .. }) => logocraft_tools::params::color_json(c),
        VariableValue::Color(c) => json!(c.to_hex()),
        VariableValue::Number(x) => json!(x),
        VariableValue::String(s) => json!(s),
        VariableValue::Boolean(b) => json!(b),
        VariableValue::Alias(id) => json!({ "alias": id }),
    }
}

fn create_collection(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.createCollection";
    let name = name_param(p, "name", C)?;
    let modes: Vec<String> =
        p.get("modes").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let modes = if modes.is_empty() { vec!["Mode 1".to_string()] } else { modes };
    let id = s.edit("New Variable Collection", |d, _| {
        let v = vars_mut(d);
        let taken = |c: &str| v.collections.iter().any(|x| x.id == c);
        let id = fresh(&taken, "collection");
        let modes = modes.iter().enumerate().map(|(i, m)| Mode { id: format!("{id}/mode{}", i + 1), name: m.clone() }).collect();
        v.collections.push(Collection { id: id.clone(), name: name.clone(), modes });
        Ok(id)
    })?;
    Ok(json!({ "id": id }))
}

fn rename_collection(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.renameCollection";
    let id = collection_id(&s.doc()?.doc, p, C)?;
    let name = name_param(p, "name", C)?;
    s.edit("Rename Collection", |d, _| {
        if let Some(c) = vars_mut(d).collections.iter_mut().find(|c| c.id == id) {
            c.name = name.clone();
        }
        Ok(())
    })?;
    Ok(json!({ "id": id }))
}

/// Remove variables `gone`: aliases to them take their value; bindings go (values stay).
fn forget(d: &mut Document, gone: &[String]) {
    let snapshot = d.design_variables.clone();
    for v in &mut d.design_variables.variables {
        for (mode, val) in v.values.iter_mut() {
            if let VariableValue::Alias(t) = val
                && gone.contains(t)
            {
                let modes: BTreeMap<String, String> = [(v.collection.clone(), mode.clone())].into();
                if let Some(x) = snapshot.resolve(t, &modes) {
                    *val = x;
                }
            }
        }
    }
    d.design_variables.variables.retain(|v| !gone.contains(&v.id));
    let mut ids = vec![];
    d.walk(|n| {
        if n.extras().bound.as_ref().is_some_and(|b| b.values().any(|v| gone.contains(v))) {
            ids.push(n.id);
        }
    });
    for id in ids {
        if let Some(n) = d.node_mut(id) {
            let mut b = n.extras().bound.clone().unwrap_or_default();
            b.retain(|_, v| !gone.contains(v));
            n.extras_mut().bound = (!b.is_empty()).then_some(b);
        }
    }
}

fn delete_collection(s: &mut Session, p: &Value) -> Result<Value> {
    let id = collection_id(&s.doc()?.doc, p, "variable.deleteCollection")?;
    s.edit("Delete Collection", |d, _| {
        let gone: Vec<String> = d.design_variables.variables.iter().filter(|v| v.collection == id).map(|v| v.id.clone()).collect();
        forget(d, &gone);
        d.design_variables.collections.retain(|c| c.id != id);
        Ok(())
    })?;
    Ok(json!({ "id": id }))
}

fn add_mode(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.addMode";
    let id = collection_id(&s.doc()?.doc, p, C)?;
    let name = name_param(p, "name", C)?;
    let mode = s.edit("Add Mode", |d, _| {
        let v = vars_mut(d);
        let Some(c) = v.collections.iter_mut().find(|c| c.id == id) else { return Err(EngineError::Other("no collection".into())) };
        let taken = |m: &str| c.modes.iter().any(|x| x.id == m);
        let mode = fresh(&taken, &format!("{id}/mode"));
        let default = c.default_mode().cloned();
        c.modes.push(Mode { id: mode.clone(), name: name.clone() });
        for var in v.variables.iter_mut().filter(|x| x.collection == id) {
            if let Some(val) = default.as_ref().and_then(|dm| var.values.get(dm)).cloned() {
                var.values.insert(mode.clone(), val);
            }
        }
        Ok(mode)
    })?;
    Ok(json!({ "id": mode }))
}

fn rename_mode(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.renameMode";
    let id = collection_id(&s.doc()?.doc, p, C)?;
    let key = name_param(p, "mode", C)?;
    let name = name_param(p, "name", C)?;
    s.edit("Rename Mode", |d, _| {
        if let Some(c) = vars_mut(d).collections.iter_mut().find(|c| c.id == id)
            && let Some(m) = c.modes.iter_mut().find(|m| m.id == key || m.name == key)
        {
            m.name = name.clone();
        }
        Ok(())
    })?;
    Ok(json!({ "id": id }))
}

fn remove_mode(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.removeMode";
    let d = &s.doc()?.doc;
    let id = collection_id(d, p, C)?;
    let key = name_param(p, "mode", C)?;
    let c = d.design_variables.collection(&id).ok_or_else(|| EngineError::Other("no collection".into()))?;
    let mode = mode_id(c, &key).ok_or_else(|| EngineError::Other(format!("no mode `{key}`")))?;
    if c.modes.len() < 2 {
        return Err(EngineError::Other("a collection keeps at least one mode".into()));
    }
    s.edit("Remove Mode", |d, _| {
        let v = vars_mut(d);
        if let Some(c) = v.collections.iter_mut().find(|c| c.id == id) {
            c.modes.retain(|m| m.id != mode);
        }
        for var in &mut v.variables {
            var.values.remove(&mode);
        }
        // Containers on it go back to Auto.
        let mut ids = vec![];
        d.walk(|n| {
            if n.extras().var_modes.as_ref().is_some_and(|m| m.get(&id) == Some(&mode)) {
                ids.push(n.id);
            }
        });
        for n in ids {
            if let Some(n) = d.node_mut(n) {
                let mut m = n.extras().var_modes.clone().unwrap_or_default();
                m.remove(&id);
                n.extras_mut().var_modes = (!m.is_empty()).then_some(m);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "id": id }))
}

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.create";
    let name = name_param(p, "name", C)?;
    let kind = str_param(p, "type").and_then(VariableType::parse).ok_or_else(|| bad(C, "type must be color, number, string or boolean"))?;
    let coll_key = name_param(p, "collection", C)?;
    let d = &s.doc()?.doc;
    let existing = d.design_variables.find_collection(&coll_key).cloned();
    if let Some(c) = &existing
        && d.design_variables.variables.iter().any(|v| v.collection == c.id && v.name == name)
    {
        return Err(EngineError::Other(format!("`{}` has a variable `{name}` already", c.name)));
    }
    let first = match p.get("value") {
        Some(v) => value_of(kind, v, C)?,
        None => kind.initial(),
    };
    let per_mode: Vec<(String, VariableValue)> = match p.get("values").and_then(Value::as_object) {
        Some(o) => o.iter().map(|(k, v)| Ok((k.clone(), value_of(kind, v, C)?))).collect::<Result<_>>()?,
        None => vec![],
    };
    let id = s.edit("New Variable", |d, _| {
        let v = vars_mut(d);
        let coll = match existing.clone() {
            Some(c) => c,
            None => {
                let taken = |c: &str| v.collections.iter().any(|x| x.id == c);
                let id = fresh(&taken, "collection");
                let c = Collection { id: id.clone(), name: coll_key.clone(), modes: vec![Mode { id: format!("{id}/mode1"), name: "Mode 1".into() }] };
                v.collections.push(c.clone());
                c
            }
        };
        let taken = |c: &str| v.variables.iter().any(|x| x.id == c);
        let id = fresh(&taken, "var");
        let mut values: BTreeMap<String, VariableValue> = coll.modes.iter().map(|m| (m.id.clone(), first.clone())).collect();
        for (k, val) in &per_mode {
            if let Some(m) = mode_id(&coll, k) {
                values.insert(m, val.clone());
            }
        }
        v.variables.push(Variable { id: id.clone(), name: name.clone(), collection: coll.id.clone(), kind, values });
        Ok(id)
    })?;
    Ok(json!({ "id": id }))
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.set";
    let d = &s.doc()?.doc;
    let id = variable_id(d, &name_param(p, "variable", C)?)?;
    let var = d.design_variables.get(&id).cloned().ok_or_else(|| EngineError::Other("no variable".into()))?;
    let coll = d.design_variables.collection(&var.collection).cloned().ok_or_else(|| EngineError::Other("no collection".into()))?;
    let mode = match str_param(p, "mode") {
        Some(k) => mode_id(&coll, k).ok_or_else(|| EngineError::Other(format!("no mode `{k}`")))?,
        None => coll.default_mode().cloned().ok_or_else(|| EngineError::Other("the collection has no mode".into()))?,
    };
    let value = match (p.get("alias"), p.get("value")) {
        (Some(a), _) => {
            let target = variable_id(d, a.as_str().ok_or_else(|| bad(C, "alias takes a variable"))?)?;
            let t = d.design_variables.get(&target).ok_or_else(|| EngineError::Other("no variable".into()))?;
            if t.kind != var.kind {
                return Err(bad(C, "an alias must be of the same type"));
            }
            if d.design_variables.would_loop(&id, &target) {
                return Err(bad(C, "that alias would make a loop"));
            }
            VariableValue::Alias(target)
        }
        (None, Some(v)) => value_of(var.kind, v, C)?,
        (None, None) => return Err(bad(C, "give `value` or `alias`")),
    };
    s.edit("Variable Value", |d, _| {
        if let Some(v) = vars_mut(d).get_mut(&id) {
            v.values.insert(mode.clone(), value.clone());
        }
        Ok(())
    })?;
    Ok(json!({ "id": id }))
}

fn rename(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.rename";
    let id = variable_id(&s.doc()?.doc, &name_param(p, "variable", C)?)?;
    let name = name_param(p, "name", C)?;
    s.edit("Rename Variable", |d, _| {
        if let Some(v) = vars_mut(d).get_mut(&id) {
            v.name = name.clone();
        }
        Ok(())
    })?;
    Ok(json!({ "id": id }))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let id = variable_id(&s.doc()?.doc, &name_param(p, "variable", "variable.delete")?)?;
    s.edit("Delete Variable", |d, _| {
        forget(d, std::slice::from_ref(&id));
        Ok(())
    })?;
    Ok(json!({ "id": id }))
}

fn bind(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.bind";
    let field = name_param(p, "field", C)?;
    let (_, kind) = FIELDS.iter().find(|(f, _)| *f == field).copied().ok_or_else(|| bad(C, format!("can't bind `{field}`")))?;
    let ids = super::brushsym::roots(s, p)?;
    let d = &s.doc()?.doc;
    let var = match p.get("variable") {
        Some(Value::Null) | None => None,
        Some(v) => {
            let id = variable_id(d, v.as_str().ok_or_else(|| bad(C, "variable takes an id or a name"))?)?;
            let t = d.design_variables.get(&id).map(|v| v.kind);
            let ok = t == Some(kind) || (field == "characters" && t == Some(VariableType::Number));
            if !ok {
                return Err(bad(C, format!("`{field}` takes a {} variable", kind.name())));
            }
            Some(id)
        }
    };
    s.edit("Bind Variable", |d, _| {
        for id in &ids {
            if let Some(n) = d.node_mut(*id) {
                let mut b = n.extras().bound.clone().unwrap_or_default();
                match &var {
                    Some(v) => b.insert(field.clone(), v.clone()),
                    None => b.remove(&field),
                };
                n.extras_mut().bound = (!b.is_empty()).then_some(b);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn set_mode(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "variable.setMode";
    let d = &s.doc()?.doc;
    let coll = collection_id(d, p, C)?;
    let c = d.design_variables.collection(&coll).cloned().ok_or_else(|| EngineError::Other("no collection".into()))?;
    let mode = match p.get("mode") {
        Some(Value::Null) | None => None,
        Some(v) => {
            let k = v.as_str().ok_or_else(|| bad(C, "mode takes an id or a name"))?;
            Some(mode_id(&c, k).ok_or_else(|| EngineError::Other(format!("no mode `{k}`")))?)
        }
    };
    let ids = super::brushsym::roots(s, p)?;
    s.edit("Variable Mode", |d, _| {
        for id in &ids {
            if let Some(n) = d.node_mut(*id) {
                let mut m = n.extras().var_modes.clone().unwrap_or_default();
                match &mode {
                    Some(v) => m.insert(coll.clone(), v.clone()),
                    None => m.remove(&coll),
                };
                n.extras_mut().var_modes = (!m.is_empty()).then_some(m);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let v = &s.doc()?.doc.design_variables;
    let collections: Vec<Value> = v
        .collections
        .iter()
        .map(|c| json!({ "id": c.id, "name": c.name, "modes": c.modes.iter().map(|m| json!({ "id": m.id, "name": m.name })).collect::<Vec<_>>() }))
        .collect();
    let variables: Vec<Value> = v
        .variables
        .iter()
        .map(|x| {
            let values: serde_json::Map<String, Value> = x.values.iter().map(|(k, val)| (k.clone(), value_json(val))).collect();
            json!({ "id": x.id, "name": x.name, "collection": x.collection, "type": x.kind.name(), "values": values })
        })
        .collect();
    Ok(json!({ "collections": collections, "variables": variables }))
}
