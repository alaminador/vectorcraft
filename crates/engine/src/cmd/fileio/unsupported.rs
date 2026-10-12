//! Formats people ask for that LogoCraft doesn't read or write, each with what to use instead.

use serde_json::{Value, json};

pub struct Unsupported {
    pub id: &'static str,
    pub label: &'static str,
    /// Lower-case extensions without the dot.
    pub extensions: &'static [&'static str],
    pub hint: &'static str,
}

/// Formats LogoCraft doesn't read or write. Append-only.
pub const UNSUPPORTED: &[Unsupported] = &[
    Unsupported {
        id: "dxf",
        label: "DXF",
        extensions: &["dxf"],
        hint: "DXF is not supported in LogoCraft: export SVG or PDF instead, which CAD apps import",
    },
    Unsupported {
        id: "dwg",
        label: "DWG",
        extensions: &["dwg"],
        hint: "DWG is not supported in LogoCraft: export SVG or PDF instead, which CAD apps import",
    },
    Unsupported {
        id: "pict",
        label: "PICT",
        extensions: &["pict", "pct", "pic"],
        hint: "PICT is a retired format and is not supported: convert the file to PDF or PNG first",
    },
    Unsupported {
        id: "emf",
        label: "EMF",
        extensions: &["emf"],
        hint: "EMF is not supported in LogoCraft: use SVG or PDF for vector art, PNG for pictures",
    },
    Unsupported {
        id: "wmf",
        label: "WMF",
        extensions: &["wmf"],
        hint: "WMF is not supported in LogoCraft: use SVG or PDF for vector art, PNG for pictures",
    },
    Unsupported {
        id: "psd",
        label: "PSD",
        extensions: &["psd", "psb"],
        hint: "PSD files are not supported in LogoCraft: save a PNG or JPEG and place that",
    },
    Unsupported { id: "tiff", label: "TIFF", extensions: &["tif", "tiff"], hint: "TIFF is not supported in LogoCraft: use PNG, JPEG or WebP" },
    Unsupported { id: "bmp", label: "BMP", extensions: &["bmp"], hint: "BMP is not supported in LogoCraft: use PNG, JPEG or WebP" },
    Unsupported { id: "tga", label: "Targa", extensions: &["tga"], hint: "Targa is not supported in LogoCraft: use PNG, JPEG or WebP" },
    Unsupported {
        id: "gif",
        label: "GIF",
        extensions: &["gif"],
        hint: "GIF is not supported in LogoCraft: use PNG (PNG-8 for a small palette file), JPEG or WebP",
    },
    Unsupported {
        id: "txt",
        label: "Text",
        extensions: &["txt"],
        hint: "Plain-text import and export are not part of LogoCraft: paste the text into a type object",
    },
];

/// The unsupported format an id or extension (any case, leading dot allowed) names.
pub fn unsupported(id_or_ext: &str) -> Option<&'static Unsupported> {
    let k = id_or_ext.trim_start_matches('.').to_ascii_lowercase();
    UNSUPPORTED.iter().find(|u| u.id == k || u.extensions.contains(&k.as_str()))
}

impl Unsupported {
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "label": self.label, "extensions": self.extensions, "hint": self.hint })
    }
}
