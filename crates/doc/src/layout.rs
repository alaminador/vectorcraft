//! Auto layout, as Figma has it (public documentation:
//! https://help.figma.com/hc/en-us/articles/360040451373): a frame (a group carrying an
//! [`AutoLayout`]) places its children in a row or a column, with a gap between them and padding
//! inside its edges, aligned on a 3 × 3 grid; it hugs its contents or keeps a fixed size; a row
//! can wrap. Each child ([`LayoutItem`]) keeps its own size or fills the frame along either axis,
//! or is positioned absolutely (left where it is, out of the flow).
//!
//! The frame's bounds are [`AutoLayout::frame`]; its fill, stroke and corner radius are those of
//! its background child (the bottom-most, marked [`LayoutItem::background`]), a rectangle kept to
//! the frame's bounds. [`arrange`] is the geometry; the engine applies it after every edit.

use logocraft_geom::{Affine, Rect};
use serde::{Deserialize, Serialize};

/// The axis a frame lays its children along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Horizontal,
    Vertical,
}

/// Where children sit along an axis when there is room to spare.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

impl Align {
    /// The share of the spare room before the children (0, ½, 1).
    pub fn factor(self) -> f64 {
        match self {
            Align::Start => 0.0,
            Align::Center => 0.5,
            Align::End => 1.0,
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "start" | "left" | "top" | "min" => Some(Align::Start),
            "center" | "centre" | "middle" => Some(Align::Center),
            "end" | "right" | "bottom" | "max" => Some(Align::End),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Align::Start => "start",
            Align::Center => "center",
            Align::End => "end",
        }
    }
}

/// How a frame sizes along an axis: around its contents (Hug), or keeping its size (Fixed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sizing {
    #[default]
    Hug,
    Fixed,
}

/// How a child sizes along an axis of its frame: its own size (Fixed, or a frame's own Hug or
/// Fixed), or the room the frame leaves it (Fill container).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemSizing {
    #[default]
    Auto,
    Fill,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// A frame's auto layout settings and bounds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoLayout {
    #[serde(default)]
    pub direction: Direction,
    /// Space between children, pt (may be negative: they overlap).
    #[serde(default)]
    pub gap: f64,
    /// Gap Auto (space between): the children spread over a fixed main axis.
    #[serde(default, skip_serializing_if = "crate::skip::is_default")]
    pub gap_auto: bool,
    /// Padding: top, right, bottom, left, pt.
    #[serde(default)]
    pub padding: [f64; 4],
    /// Alignment along the direction, and across it.
    #[serde(default)]
    pub align_main: Align,
    #[serde(default)]
    pub align_cross: Align,
    #[serde(default)]
    pub width: Sizing,
    #[serde(default)]
    pub height: Sizing,
    /// Wrap (horizontal frames of fixed width): children flow onto new rows.
    #[serde(default, skip_serializing_if = "crate::skip::is_default")]
    pub wrap: bool,
    /// Space between wrapped rows, pt.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cross_gap: f64,
    /// The frame's bounds (document space).
    pub frame: Rect,
    /// Clip content: children are cut off at the frame's bounds (its background clips them).
    #[serde(default, skip_serializing_if = "crate::skip::is_default")]
    pub clip: bool,
}

impl AutoLayout {
    /// Settings for a new frame at `frame`.
    pub fn new(direction: Direction, gap: f64, frame: Rect) -> Self {
        Self {
            direction,
            gap,
            gap_auto: false,
            padding: [0.0; 4],
            align_main: Align::Start,
            align_cross: Align::Start,
            width: Sizing::Hug,
            height: Sizing::Hug,
            wrap: false,
            cross_gap: 0.0,
            frame,
            clip: false,
        }
    }
    /// Move the bounds with the frame.
    pub fn transform(&mut self, a: Affine) {
        let r = a.transform_rect_bbox(self.frame);
        if r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite() {
            self.frame = r;
        }
    }
    /// These settings apart from the bounds.
    pub fn same_settings(&self, o: &AutoLayout) -> bool {
        AutoLayout { frame: o.frame, ..self.clone() } == *o
    }
    /// The sizing along the main axis and across it.
    fn sizings(&self) -> (Sizing, Sizing) {
        match self.direction {
            Direction::Horizontal => (self.width, self.height),
            Direction::Vertical => (self.height, self.width),
        }
    }
}

/// Minimum and maximum width and height, pt (Figma's min/max: a hugging or filling size stays
/// within them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f64>,
}

impl Limits {
    /// `v` within `min..max` (a max below the min gives way to the min).
    fn clamp(v: f64, min: Option<f64>, max: Option<f64>) -> f64 {
        let v = match max.filter(|m| m.is_finite()) {
            Some(m) => v.min(m),
            None => v,
        };
        match min.filter(|m| m.is_finite()) {
            Some(m) => v.max(m),
            None => v,
        }
    }
    pub fn width(&self, v: f64) -> f64 {
        Self::clamp(v, self.min_width, self.max_width)
    }
    pub fn height(&self, v: f64) -> f64 {
        Self::clamp(v, self.min_height, self.max_height)
    }
    pub fn is_empty(&self) -> bool {
        *self == Limits::default()
    }
}

/// A frame child's auto layout settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutItem {
    #[serde(default, skip_serializing_if = "crate::skip::is_default")]
    pub width: ItemSizing,
    #[serde(default, skip_serializing_if = "crate::skip::is_default")]
    pub height: ItemSizing,
    /// Absolute position: out of the flow, left where it is.
    #[serde(default, skip_serializing_if = "crate::skip::is_default")]
    pub absolute: bool,
    /// The frame's background (its fill, stroke and corners), kept to its bounds.
    #[serde(default, skip_serializing_if = "crate::skip::is_default")]
    pub background: bool,
    /// Min and max width and height (of a frame's own size too).
    #[serde(default, flatten)]
    pub limits: Limits,
}

impl LayoutItem {
    /// In the flow (placed by the frame)?
    pub fn flows(&self) -> bool {
        !self.absolute && !self.background
    }
}

/// A child to place: its size and whether it fills the frame along each axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub w: f64,
    pub h: f64,
    pub fill_w: bool,
    pub fill_h: bool,
    /// Its min and max sizes (what Fill container gives it stays within them).
    pub limits: Limits,
}

/// [`arrange`]'s result: the frame's bounds and each child's.
#[derive(Clone, Debug, PartialEq)]
pub struct Arranged {
    pub frame: Rect,
    pub items: Vec<Rect>,
}

/// Lay `slots` out in frame `l` (whose own size stays within `own`): the frame's new bounds (its
/// top-left corner stays) and each child's bounds, in order.
pub fn arrange(l: &AutoLayout, own: &Limits, slots: &[Slot]) -> Arranged {
    let horizontal = l.direction == Direction::Horizontal;
    let [pt, pr, pb, pl] = l.padding.map(|p| if p.is_finite() { p } else { 0.0 });
    let (pm0, pm1, pc0, pc1) = if horizontal { (pl, pr, pt, pb) } else { (pt, pb, pl, pr) };
    let (main_sizing, cross_sizing) = l.sizings();
    let size = |s: &Slot| if horizontal { (s.w, s.h, s.fill_w, s.fill_h) } else { (s.h, s.w, s.fill_h, s.fill_w) };
    let clamp_main = |lim: &Limits, v: f64| if horizontal { lim.width(v) } else { lim.height(v) };
    let clamp_cross = |lim: &Limits, v: f64| if horizontal { lim.height(v) } else { lim.width(v) };
    let finite = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
    let (fw, fh) = (l.frame.width(), l.frame.height());
    let (frame_main, frame_cross) = if horizontal { (fw, fh) } else { (fh, fw) };
    let main_fixed = (main_sizing == Sizing::Fixed).then_some(frame_main);
    let cross_fixed = (cross_sizing == Sizing::Fixed).then_some(frame_cross);
    let inner_main = main_fixed.map(|m| (m - pm0 - pm1).max(0.0));
    let gap = if l.gap.is_finite() { l.gap } else { 0.0 };
    let cross_gap = if l.cross_gap.is_finite() { l.cross_gap } else { 0.0 };

    // Rows (one unless wrapping): indices into `slots`.
    let mut lines: Vec<Vec<usize>> = vec![];
    match inner_main.filter(|_| l.wrap && horizontal) {
        Some(inner) => {
            let mut cur: Vec<usize> = vec![];
            let mut used = 0.0;
            for (i, s) in slots.iter().enumerate() {
                let m = finite(size(s).0);
                let next = if cur.is_empty() { m } else { used + gap + m };
                if !cur.is_empty() && next > inner + 1e-9 {
                    lines.push(std::mem::take(&mut cur));
                    used = m;
                } else {
                    used = next;
                }
                cur.push(i);
            }
            if !cur.is_empty() {
                lines.push(cur);
            }
        }
        None => lines.push((0..slots.len()).collect()),
    }

    // Main sizes (Fill shares what the others leave) and each row's gap, length and thickness.
    let mut mains: Vec<f64> = slots.iter().map(|s| finite(size(s).0)).collect();
    let mut gaps = vec![gap; lines.len()];
    let mut lengths = vec![0.0; lines.len()];
    let mut thick = vec![0.0f64; lines.len()];
    for (li, line) in lines.iter().enumerate() {
        let n = line.len();
        if let Some(inner) = inner_main {
            if l.gap_auto {
                let sum: f64 = line.iter().filter_map(|i| mains.get(*i)).sum();
                gaps[li] = if n > 1 { ((inner - sum) / (n - 1) as f64).max(0.0) } else { 0.0 };
            } else {
                let fills: Vec<usize> = line.iter().copied().filter(|i| slots.get(*i).is_some_and(|s| size(s).2)).collect();
                if !fills.is_empty() {
                    let fixed: f64 = line.iter().filter(|i| !fills.contains(i)).filter_map(|i| mains.get(*i)).sum();
                    let room = inner - fixed - gap * n.saturating_sub(1) as f64;
                    // Share the room; a child held by its min or max keeps that, and the others
                    // share what is left (a few rounds settle it).
                    let mut open: Vec<usize> = fills.clone();
                    let mut left = room;
                    for _ in 0..fills.len() + 1 {
                        if open.is_empty() {
                            break;
                        }
                        let each = (left / open.len() as f64).max(1.0);
                        let held: Vec<usize> = open
                            .iter()
                            .copied()
                            .filter(|i| slots.get(*i).is_some_and(|s| (clamp_main(&s.limits, each) - each).abs() > 1e-9))
                            .collect();
                        if held.is_empty() {
                            for i in &open {
                                if let Some(m) = mains.get_mut(*i) {
                                    *m = each;
                                }
                            }
                            break;
                        }
                        for i in held {
                            let v = slots.get(i).map_or(each, |s| clamp_main(&s.limits, each));
                            if let Some(m) = mains.get_mut(i) {
                                *m = v;
                            }
                            left -= v;
                            open.retain(|o| *o != i);
                        }
                    }
                }
            }
        }
        lengths[li] = line.iter().filter_map(|i| mains.get(*i)).sum::<f64>() + gaps[li] * n.saturating_sub(1) as f64;
        let own = |fill_only: bool| {
            line.iter().filter_map(|i| slots.get(*i)).filter(|s| size(s).3 == fill_only).map(|s| finite(size(s).1)).fold(0.0, f64::max)
        };
        thick[li] = if line.iter().any(|i| slots.get(*i).is_some_and(|s| !size(s).3)) { own(false) } else { own(true) };
    }

    let total_main = clamp_main(own, main_fixed.unwrap_or_else(|| lengths.iter().copied().fold(0.0, f64::max) + pm0 + pm1));
    let content_cross = thick.iter().sum::<f64>() + cross_gap * lines.len().saturating_sub(1) as f64;
    let total_cross = clamp_cross(own, cross_fixed.unwrap_or(content_cross + pc0 + pc1));
    let inner_main = (total_main - pm0 - pm1).max(0.0);
    let inner_cross = (total_cross - pc0 - pc1).max(0.0);
    // One row spans the whole inner cross size (children fill or align in it); wrapped rows keep
    // their own thickness and align as a block.
    if lines.len() == 1 {
        thick[0] = inner_cross;
    }
    let block = if lines.len() == 1 { inner_cross } else { content_cross };
    let mut cross_at = pc0 + (inner_cross - block).max(0.0) * l.align_cross.factor();

    let mut items = vec![Rect::ZERO; slots.len()];
    for (li, line) in lines.iter().enumerate() {
        let spare = if l.gap_auto && main_fixed.is_some() { 0.0 } else { (inner_main - lengths[li]).max(0.0) };
        let mut main_at = pm0 + spare * l.align_main.factor();
        for i in line {
            let (Some(s), Some(m)) = (slots.get(*i), mains.get(*i)) else { continue };
            let (_, c, _, fill_c) = size(s);
            let c = if fill_c { clamp_cross(&s.limits, thick[li]) } else { finite(c) };
            let off = cross_at + (thick[li] - c).max(0.0) * if fill_c { 0.0 } else { l.align_cross.factor() };
            let (x, y, w, h) = if horizontal { (main_at, off, *m, c) } else { (off, main_at, c, *m) };
            if let Some(r) = items.get_mut(*i) {
                *r = Rect::new(l.frame.x0 + x, l.frame.y0 + y, l.frame.x0 + x + w, l.frame.y0 + y + h);
            }
            main_at += m + gaps[li];
        }
        cross_at += thick[li] + cross_gap;
    }
    let (w, h) = if horizontal { (total_main, total_cross) } else { (total_cross, total_main) };
    Arranged { frame: Rect::new(l.frame.x0, l.frame.y0, l.frame.x0 + w, l.frame.y0 + h), items }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(w: f64, h: f64) -> Slot {
        Slot { w, h, fill_w: false, fill_h: false, limits: Limits::default() }
    }

    fn row(gap: f64) -> AutoLayout {
        AutoLayout::new(Direction::Horizontal, gap, Rect::new(10.0, 20.0, 10.0, 20.0))
    }

    #[test]
    fn a_hugging_row_puts_children_side_by_side_inside_the_padding() {
        let mut l = row(10.0);
        l.padding = [5.0, 8.0, 5.0, 8.0];
        let a = arrange(&l, &Limits::default(), &[slot(40.0, 20.0), slot(30.0, 10.0)]);
        assert_eq!(a.frame, Rect::new(10.0, 20.0, 10.0 + 8.0 + 40.0 + 10.0 + 30.0 + 8.0, 20.0 + 5.0 + 20.0 + 5.0));
        assert_eq!(a.items[0], Rect::new(18.0, 25.0, 58.0, 45.0));
        assert_eq!(a.items[1], Rect::new(68.0, 25.0, 98.0, 35.0), "top aligned");
        l.align_cross = Align::Center;
        assert_eq!(arrange(&l, &Limits::default(), &[slot(40.0, 20.0), slot(30.0, 10.0)]).items[1].y0, 30.0);
    }

    #[test]
    fn a_column_stacks_and_aligns_across() {
        let mut l = AutoLayout::new(Direction::Vertical, 4.0, Rect::new(0.0, 0.0, 0.0, 0.0));
        l.align_cross = Align::End;
        let a = arrange(&l, &Limits::default(), &[slot(40.0, 20.0), slot(30.0, 10.0)]);
        assert_eq!(a.frame, Rect::new(0.0, 0.0, 40.0, 34.0));
        assert_eq!(a.items[1], Rect::new(10.0, 24.0, 40.0, 34.0));
    }

    #[test]
    fn fixed_frames_align_fill_and_spread() {
        let mut l = row(10.0);
        l.width = Sizing::Fixed;
        l.height = Sizing::Fixed;
        l.frame = Rect::new(0.0, 0.0, 200.0, 50.0);
        l.align_main = Align::Center;
        let a = arrange(&l, &Limits::default(), &[slot(40.0, 20.0), slot(40.0, 20.0)]);
        assert_eq!(a.frame, l.frame, "a fixed frame keeps its size");
        assert_eq!(a.items[0].x0, 55.0, "centred: (200 - 90) / 2");
        // Fill container takes what the others leave, across too.
        let fill = Slot { w: 1.0, h: 1.0, fill_w: true, fill_h: true, limits: Limits::default() };
        let a = arrange(&l, &Limits::default(), &[slot(40.0, 20.0), fill]);
        assert_eq!(a.items[1], Rect::new(50.0, 0.0, 200.0, 50.0));
        // Gap Auto spreads them edge to edge.
        l.gap_auto = true;
        let a = arrange(&l, &Limits::default(), &[slot(40.0, 20.0), slot(40.0, 20.0), slot(40.0, 20.0)]);
        assert_eq!((a.items[0].x0, a.items[1].x0, a.items[2].x1), (0.0, 80.0, 200.0));
    }

    #[test]
    fn a_wrapping_row_starts_new_rows() {
        let mut l = row(10.0);
        l.width = Sizing::Fixed;
        l.frame = Rect::new(0.0, 0.0, 100.0, 0.0);
        l.wrap = true;
        l.cross_gap = 6.0;
        let a = arrange(&l, &Limits::default(), &[slot(40.0, 20.0), slot(40.0, 20.0), slot(40.0, 30.0)]);
        assert_eq!(a.items[1].x0, 50.0);
        assert_eq!(a.items[2], Rect::new(0.0, 26.0, 40.0, 56.0), "third child on the second row");
        assert_eq!(a.frame.height(), 56.0, "hugs both rows");
    }

    #[test]
    fn min_and_max_hold_sizes() {
        let mut l = row(0.0);
        l.width = Sizing::Fixed;
        l.frame = Rect::new(0.0, 0.0, 300.0, 20.0);
        let capped = Slot { w: 1.0, h: 10.0, fill_w: true, fill_h: false, limits: Limits { max_width: Some(50.0), ..Default::default() } };
        let free = Slot { w: 1.0, h: 10.0, fill_w: true, fill_h: false, limits: Limits::default() };
        let a = arrange(&l, &Limits::default(), &[capped, free]);
        assert_eq!(a.items[0].width(), 50.0, "held by its max");
        assert_eq!(a.items[1].width(), 250.0, "the other takes the rest");
        // A hugging frame stays within its own min.
        let hug = row(0.0);
        let a = arrange(&hug, &Limits { min_width: Some(120.0), ..Default::default() }, &[slot(40.0, 10.0)]);
        assert_eq!(a.frame.width(), 120.0);
    }

    #[test]
    fn bad_numbers_do_not_spread() {
        let mut l = row(f64::NAN);
        l.padding = [f64::INFINITY, 0.0, 0.0, 0.0];
        let a = arrange(&l, &Limits::default(), &[slot(f64::NAN, 10.0), slot(10.0, -5.0)]);
        assert!(a.items.iter().all(|r| r.x0.is_finite() && r.y1.is_finite()));
        assert!(arrange(&row(0.0), &Limits::default(), &[]).frame.is_zero_area());
    }
}
