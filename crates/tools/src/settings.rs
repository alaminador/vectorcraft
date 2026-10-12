//! Tool options that persist: a tool keeps them when another tool is chosen and they are saved
//! with the preferences, as the reference app keeps each tool's options. Options that are the
//! state of an interaction (pins, a reference point, the text being edited, the files a place
//! cursor holds) aren't listed: they start afresh.
//!
//! Each row names the tools it covers, the store their values live in and the option keys. A row
//! with a shared store gives every tool it lists one set of values: the Liquify tools share their
//! Global Brush Dimensions, the Symbolism tools their brush. Otherwise each tool has its own store,
//! named after its id.

/// (tools, the store they share (None: each tool's own), option keys).
type Row = (&'static [&'static str], Option<&'static str>, &'static [&'static str]);

const ROWS: &[Row] = &[
    (&["arc"], None, &["closed"]),
    (&["pencil"], None, &["fidelity", "fill", "editWithin", "closeWithin"]),
    (&["smooth"], None, &["fidelity"]),
    (&["polygon"], None, &["sides"]),
    (&["star"], None, &["points"]),
    (&["freeTransform"], None, &["constrain"]),
    (&["artboard"], None, &["moveArt", "scaleArt"]),
];

/// The stores `tool`'s persistent options live in, each with the option keys it holds.
pub fn stores(tool: &str) -> impl Iterator<Item = (&'static str, &'static [&'static str])> + '_ {
    ROWS.iter().filter_map(move |(tools, shared, keys)| {
        let id = tools.iter().find(|t| **t == tool)?;
        Some((shared.unwrap_or(id), *keys))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_option_is_one_the_tool_reports() {
        for (tools, _, keys) in ROWS {
            for id in *tools {
                assert!(crate::tool_info(id).is_some(), "{id} is a tool");
                let opts = crate::create(id).options();
                for k in *keys {
                    assert!(opts.get(*k).is_some(), "{id} reports `{k}` in {opts}");
                }
            }
        }
    }
}
