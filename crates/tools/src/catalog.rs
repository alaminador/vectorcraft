//! The tool catalogue: ids, labels, shortcuts and toolbar groups (Illustrator's Advanced toolbar order).

use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ToolInfo {
    pub id: &'static str,
    pub label: &'static str,
    /// Single-key shortcut (Shift+key shown as "Shift+X").
    pub shortcut: Option<&'static str>,
    /// Icon name in the UI icon set.
    pub icon: &'static str,
}

const fn t(id: &'static str, label: &'static str, shortcut: Option<&'static str>, icon: &'static str) -> ToolInfo {
    ToolInfo { id, label, shortcut, icon }
}

/// Toolbar groups in order; the first tool of each group is the default visible one.
pub const TOOL_GROUPS: &[&[ToolInfo]] = &[
    &[t("selection", "Selection Tool", Some("V"), "tool-selection")],
    &[
        t("directSelection", "Direct Selection Tool", Some("A"), "tool-direct"),
        t("groupSelection", "Group Selection Tool", None, "tool-group-select"),
    ],
    &[t("lasso", "Lasso Tool", Some("Q"), "tool-lasso")],
    &[
        t("pen", "Pen Tool", Some("P"), "tool-pen"),
        t("addAnchor", "Add Anchor Point Tool", Some("+"), "tool-pen-add"),
        t("deleteAnchor", "Delete Anchor Point Tool", Some("-"), "tool-pen-delete"),
        t("anchorPoint", "Anchor Point Tool", Some("Shift+C"), "tool-anchor"),
    ],
    &[t("curvature", "Curvature Tool", Some("Shift+~"), "tool-curvature")],
    &[
        t("type", "Type Tool", Some("T"), "tool-type"),
        t("areaType", "Area Type Tool", None, "tool-type-area"),
        t("typeOnPath", "Type on a Path Tool", None, "tool-type-path"),
    ],
    &[t("lineSegment", "Line Segment Tool", Some("\\"), "tool-line"), t("arc", "Arc Tool", None, "tool-arc")],
    &[
        t("rectangle", "Rectangle Tool", Some("M"), "tool-rect"),
        t("roundedRectangle", "Rounded Rectangle Tool", None, "tool-rounded-rect"),
        t("ellipse", "Ellipse Tool", Some("L"), "tool-ellipse"),
        t("polygon", "Polygon Tool", None, "tool-polygon"),
        t("star", "Star Tool", None, "tool-star"),
    ],
    &[
        t("pencil", "Pencil Tool", Some("N"), "tool-pencil"),
        t("smooth", "Smooth Tool", None, "tool-smooth"),
        t("join", "Join Tool", None, "tool-join"),
    ],
    &[t("scissors", "Scissors Tool", Some("C"), "tool-scissors"), t("knife", "Knife", None, "tool-knife")],
    &[t("rotate", "Rotate Tool", Some("R"), "tool-rotate"), t("reflect", "Reflect Tool", Some("O"), "tool-reflect")],
    &[t("scale", "Scale Tool", Some("S"), "tool-scale"), t("shear", "Shear Tool", None, "tool-shear")],
    &[t("width", "Width Tool", Some("Shift+W"), "tool-width")],
    &[t("freeTransform", "Free Transform Tool", Some("E"), "tool-free-transform")],
    &[t("shapeBuilder", "Shape Builder Tool", Some("Shift+M"), "tool-shape-builder")],
    &[t("gradient", "Gradient Tool", Some("G"), "tool-gradient")],
    &[t("eyedropper", "Eyedropper Tool", Some("I"), "tool-eyedropper"), t("measure", "Measure Tool", None, "tool-measure")],
    &[t("artboard", "Artboard Tool", Some("Shift+O"), "tool-artboard")],
    &[t("hand", "Hand Tool", Some("H"), "tool-hand")],
    &[t("zoom", "Zoom Tool", Some("Z"), "tool-zoom")],
];

pub fn all_tools() -> impl Iterator<Item = &'static ToolInfo> {
    TOOL_GROUPS.iter().flat_map(|g| g.iter())
}

pub fn tool_info(id: &str) -> Option<&'static ToolInfo> {
    all_tools().find(|t| t.id == id)
}

/// Tool for a single-key shortcut such as "V" or "Shift+M".
pub fn tool_for_shortcut(s: &str) -> Option<&'static ToolInfo> {
    all_tools().find(|t| t.shortcut == Some(s))
}

/// Index of the group containing `id`.
pub fn group_of(id: &str) -> Option<usize> {
    TOOL_GROUPS.iter().position(|g| g.iter().any(|t| t.id == id))
}

/// Selection, Direct Selection and Group Selection: the tools Cmd held lends the others for a drag.
pub fn is_selection_tool(id: &str) -> bool {
    matches!(id, "selection" | "directSelection" | "groupSelection")
}

/// The tools that show a selected path's anchors and handles as Direct Selection does, and drag
/// them: a Cmd drag with one of them is a Direct Selection drag until a selection tool is chosen.
pub fn edits_anchors(id: &str) -> bool {
    matches!(id, "directSelection" | "pen" | "addAnchor" | "deleteAnchor" | "anchorPoint" | "curvature")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_ids_and_shortcuts() {
        let mut ids: Vec<&str> = all_tools().map(|t| t.id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate tool ids");
        let mut sc: Vec<&str> = all_tools().filter_map(|t| t.shortcut).collect();
        let n = sc.len();
        sc.sort();
        sc.dedup();
        assert_eq!(sc.len(), n, "duplicate shortcuts");
        assert!(n > 20);
    }

    #[test]
    fn lookups() {
        assert_eq!(tool_for_shortcut("V").unwrap().id, "selection");
        assert_eq!(tool_for_shortcut("Shift+M").unwrap().id, "shapeBuilder");
        assert_eq!(group_of("star"), group_of("rectangle"));
        assert!(is_selection_tool("groupSelection") && !is_selection_tool("pen"));
        assert!(edits_anchors("pen") && !edits_anchors("rectangle"));
    }
}
