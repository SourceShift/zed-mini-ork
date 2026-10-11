use std::{fmt::Debug, num::NonZeroU32, ops::Range, sync::Arc};

use collections::HashMap;
use gpui::{Font, HighlightStyle, LineLayout, Pixels, TextAlign, WindowTextSystem};
use multi_buffer::{Anchor, MultiBufferOffset, MultiBufferRow, MultiBufferSnapshot, ToOffset};
use parking_lot::Mutex;
use project::InlayId;
use sum_tree::{Bias, TreeMap};
use text::BufferId;
use util::debug_panic;

use crate::{
    EditorStyle, MAX_LINE_LEN,
    hover_links::InlayHighlight,
    scroll::{ScrollOffset, ScrollPixelOffset},
};

use super::{
    DisplayMap, DisplayPoint, DisplayRow, DisplaySnapshot, FoldPoint, HighlightKey,
    HighlightStyleInterner,
    block_map::BlockSnapshot,
    fold_map::{FoldEdit, FoldSnapshot},
    inlay_map::has_chunk_renderer,
    row_ruler::{
        MAX_IDLE_REFRESHES, MappedRow, RowRuler, RowRulerCache, RulerCacheVersion, RulerShaper,
    },
    tab_map::{TabEdit, TabMap, TabPoint, TabSnapshot},
};

const MAX_TRACKED_RULER_DIRT: usize = 64;
const MAX_PENDING_TAB_EDITS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GridCell {
    pub(crate) width: Pixels,
    pub(crate) monospace: bool,
}

impl GridCell {
    const FIT_TOLERANCE: ScrollPixelOffset = 0.5;

    pub(crate) fn measure(
        text_system: &WindowTextSystem,
        style: &EditorStyle,
        highlight_styles: impl Iterator<Item = HighlightStyle>,
        font_size: Pixels,
    ) -> Self {
        let base = style.text.font();
        let font_id = text_system.resolve_font(&base);
        let width = text_system.em_layout_width(font_id, font_size);
        let mut variants = vec![(base.weight, base.style)];
        let styles = style
            .syntax
            .highlights()
            .copied()
            .chain([
                style.inlay_hints_style,
                style.edit_prediction_styles.insertion,
                style.edit_prediction_styles.whitespace,
            ])
            .chain(highlight_styles);
        for highlight in styles {
            let variant = (
                highlight.font_weight.unwrap_or(base.weight),
                highlight.font_style.unwrap_or(base.style),
            );
            if !variants.contains(&variant) {
                variants.push(variant);
            }
        }
        let monospace = width > Pixels::ZERO
            && variants.into_iter().all(|(weight, style)| {
                let font = Font {
                    weight,
                    style,
                    ..base.clone()
                };
                font_is_grid_exact(text_system, &font, font_size, width)
            });
        Self { width, monospace }
    }

    pub(crate) fn fits(&self, columns: &Range<u32>, shaped_width: Pixels) -> bool {
        let grid_width = ScrollPixelOffset::from(self.width) * columns.len() as ScrollPixelOffset;
        (ScrollPixelOffset::from(shaped_width) - grid_width).abs() <= Self::FIT_TOLERANCE
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HorizontalViewport {
    pub(crate) scroll_columns: ScrollOffset,
    pub(crate) visible_columns: ScrollOffset,
    pub(crate) text_align: TextAlign,
    pub(crate) content_width: Pixels,
}

impl HorizontalViewport {
    pub(crate) fn aligned(&self, line_width: ScrollPixelOffset, cell: GridCell) -> Self {
        let content_width = ScrollPixelOffset::from(self.content_width);
        let alignment_offset = match self.text_align {
            TextAlign::Left => 0.,
            TextAlign::Center => (content_width - line_width) / 2.,
            TextAlign::Right => content_width - line_width,
        };
        if alignment_offset == 0. || cell.width <= Pixels::ZERO {
            return *self;
        }
        Self {
            scroll_columns: self.scroll_columns
                - alignment_offset / ScrollPixelOffset::from(cell.width),
            ..*self
        }
    }

    fn first_column(&self) -> usize {
        self.scroll_columns.max(0.).floor() as usize
    }

    fn column_count(&self) -> usize {
        (self.visible_columns.max(0.).ceil() as usize).max(1)
    }

    pub(crate) fn shaping_window(&self, row_len: u32) -> Range<u32> {
        let visible = self.column_count();
        let leading = visible / 2;
        let total = leading + visible * 2;
        let row_len = row_len as usize;
        let start = (self.first_column().saturating_sub(leading) / visible * visible)
            .min(row_len.saturating_sub(total));
        let end = (start + total).min(row_len);
        column_from_usize(start)..column_from_usize(end)
    }
}

#[derive(Clone)]
pub(crate) struct RuledRow {
    snapshot: Arc<DisplaySnapshot>,
    row: DisplayRow,
    pub(crate) ruler: Arc<RowRuler>,
    shaper: RulerShaper,
}

impl Debug for RuledRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuledRow")
            .field("row", &self.row)
            .field("ruler", &self.ruler)
            .finish()
    }
}

impl RuledRow {
    pub(crate) fn columns_for_viewport(
        &self,
        viewport: &HorizontalViewport,
        cell: GridCell,
    ) -> Range<u32> {
        let viewport = viewport.aligned(self.ruler.width(), cell);
        let cell_width = ScrollPixelOffset::from(cell.width);
        let visible_width = viewport.visible_columns.max(1.) * cell_width;
        let left = viewport.scroll_columns.max(0.) * cell_width;
        self.ruler
            .columns_for_x_range(left - visible_width / 2.0..left + visible_width * 1.5)
    }

    fn x_for_column(&self, column: u32) -> ScrollPixelOffset {
        self.ruler
            .x_for_column(column, &self.snapshot, self.row, &self.shaper)
    }

    fn column_for_x(&self, x: ScrollPixelOffset) -> u32 {
        self.ruler
            .column_for_x(x, &self.snapshot, self.row, &self.shaper)
    }
}

#[derive(Clone, Debug)]
pub struct WindowedRowGeometry {
    row_len: u32,
    cell: GridCell,
    window: Range<u32>,
    ruled: Option<RuledRow>,
}

impl WindowedRowGeometry {
    pub(crate) fn new(row_len: u32, cell: GridCell, window: Range<u32>) -> Self {
        Self {
            row_len,
            cell,
            window,
            ruled: None,
        }
    }

    pub(crate) fn ruled(ruled: RuledRow, row_len: u32, cell: GridCell, window: Range<u32>) -> Self {
        Self {
            row_len,
            cell,
            window,
            ruled: Some(ruled),
        }
    }

    pub(crate) fn is_ruled(&self) -> bool {
        self.ruled.is_some()
    }

    pub(crate) fn row_len(&self) -> u32 {
        self.row_len
    }

    pub(crate) fn window(&self) -> &Range<u32> {
        &self.window
    }

    pub(crate) fn cell(&self) -> GridCell {
        self.cell
    }

    pub(crate) fn start_x(&self) -> ScrollPixelOffset {
        match &self.ruled {
            Some(ruled) => ruled.ruler.x_range_for_columns(self.window.clone()).start,
            None => self.column_x(self.window.start),
        }
    }

    pub(crate) fn end_x(&self) -> ScrollPixelOffset {
        match &self.ruled {
            Some(ruled) => ruled.ruler.x_range_for_columns(self.window.clone()).end,
            None => self.column_x(self.window.end),
        }
    }

    pub(crate) fn width(&self, shaped_width: Pixels) -> ScrollPixelOffset {
        let row_width = self.column_x(self.row_len);
        if self.ruled.is_some() {
            return row_width;
        }
        row_width.max(self.start_x() + ScrollPixelOffset::from(shaped_width))
    }

    pub(crate) fn column_x(&self, column: u32) -> ScrollPixelOffset {
        if let Some(ruled) = &self.ruled {
            return ruled.x_for_column(column.min(self.row_len));
        }
        ScrollPixelOffset::from(self.cell.width) * column as ScrollPixelOffset
    }

    pub(crate) fn column_for_x(&self, x: ScrollPixelOffset) -> u32 {
        if let Some(ruled) = &self.ruled {
            return ruled.column_for_x(x).min(self.row_len);
        }
        if self.cell.width <= Pixels::ZERO {
            return 0;
        }
        let column = (x / ScrollPixelOffset::from(self.cell.width))
            .round()
            .max(0.);
        column_from_usize(column as usize).min(self.row_len)
    }

    pub(crate) fn reaches_left_edge(&self) -> bool {
        self.window.start == 0
    }

    pub(crate) fn reaches_right_edge(&self) -> bool {
        self.window.end >= self.row_len
    }

    pub(crate) fn column_left_of_window_for_x(&self, x: ScrollPixelOffset) -> u32 {
        self.column_for_x(x).min(self.window.start)
    }

    pub(crate) fn column_right_of_window_for_x(&self, x: ScrollPixelOffset) -> u32 {
        self.column_for_x(x).max(self.window.end)
    }
}

pub enum RowLayout {
    Shaped(Arc<LineLayout>),
    Windowed {
        geometry: WindowedRowGeometry,
        shaped: Arc<LineLayout>,
    },
}

impl RowLayout {
    pub fn width(&self) -> ScrollPixelOffset {
        match self {
            Self::Shaped(layout) => ScrollPixelOffset::from(layout.width),
            Self::Windowed { geometry, shaped } => geometry.width(shaped.width),
        }
    }

    pub(crate) fn x_for_index(&self, index: usize) -> ScrollPixelOffset {
        match self {
            Self::Shaped(layout) => ScrollPixelOffset::from(layout.x_for_index(index)),
            Self::Windowed { geometry, shaped } => {
                let window = geometry.window();
                let column = column_from_usize(index);
                if !window.is_empty() && column >= window.start && column <= window.end {
                    geometry.start_x()
                        + ScrollPixelOffset::from(
                            shaped.x_for_index((column - window.start) as usize),
                        )
                } else {
                    geometry.column_x(column)
                }
            }
        }
    }

    pub fn closest_index_for_x(&self, x: ScrollPixelOffset) -> usize {
        match self {
            Self::Shaped(layout) => layout.closest_index_for_x(Pixels::from(x)),
            Self::Windowed { geometry, shaped } => {
                let start_x = geometry.start_x();
                let window = geometry.window();
                if x < start_x {
                    geometry.column_left_of_window_for_x(x) as usize
                } else if x <= start_x + ScrollPixelOffset::from(shaped.width) {
                    window.start as usize + shaped.closest_index_for_x(Pixels::from(x - start_x))
                } else {
                    geometry.column_right_of_window_for_x(x) as usize
                }
            }
        }
    }
}

impl DisplaySnapshot {
    #[cfg(test)]
    pub(crate) fn is_windowed_row(&self, display_row: DisplayRow, cell: GridCell) -> bool {
        self.is_long_unwrapped_row(display_row) && self.row_has_exact_grid(display_row, cell)
    }

    pub(crate) fn is_long_unwrapped_row(&self, display_row: DisplayRow) -> bool {
        self.long_unwrapped_row_len(display_row).is_some()
    }

    pub(crate) fn long_unwrapped_row_len(&self, display_row: DisplayRow) -> Option<u32> {
        if self.masked
            || self.has_soft_wraps()
            || display_row > self.max_point().row()
            || self.is_block_line(display_row)
        {
            return None;
        }
        let row_len = self.line_len(display_row);
        (row_len as usize > MAX_LINE_LEN).then_some(row_len)
    }

    pub(crate) fn grid_window(
        &self,
        display_row: DisplayRow,
        row_len: u32,
        viewport: &HorizontalViewport,
        cell: GridCell,
        shaper: &RulerShaper,
    ) -> Option<(Range<u32>, Arc<LineLayout>)> {
        if !self.row_has_exact_grid(display_row, cell) {
            return None;
        }
        let row_width = ScrollPixelOffset::from(cell.width) * row_len as ScrollPixelOffset;
        let window = viewport.aligned(row_width, cell).shaping_window(row_len);
        let shaped = shaper.layout_columns(self, display_row, window.clone());
        if !cell.fits(&window, shaped.width) {
            debug_panic!(
                "grid-exact fonts must shape {} columns to {} px, got {:?}",
                window.len(),
                f64::from(cell.width) * window.len() as f64,
                shaped.width
            );
            return None;
        }
        Some((window, shaped))
    }

    pub(crate) fn ruled_row(&self, display_row: DisplayRow, shaper: RulerShaper) -> RuledRow {
        let tab_row = self
            .unwrapped_tab_point(DisplayPoint::new(display_row, 0), Bias::Left)
            .row();
        let ruler = self
            .row_rulers
            .get_or_build(tab_row, &shaper, |previous, renderer_widths| {
                RowRuler::new(self, display_row, &shaper, previous, renderer_widths)
            });
        RuledRow {
            snapshot: self
                .ruled_row_snapshot
                .get_or_init(|| Arc::new(self.clone()))
                .clone(),
            row: display_row,
            ruler,
            shaper,
        }
    }

    pub(crate) fn row_has_exact_grid(&self, display_row: DisplayRow, cell: GridCell) -> bool {
        if !cell.monospace {
            return false;
        }
        let fold_snapshot = self.fold_snapshot();
        let fold_row = self
            .display_point_to_fold_point(DisplayPoint::new(display_row, 0), Bias::Left)
            .row();
        let fold_range =
            FoldPoint::new(fold_row, 0)..FoldPoint::new(fold_row, fold_snapshot.line_len(fold_row));
        let summary = fold_snapshot.text_summary_for_range(fold_range.clone());
        if summary.chars != summary.len.0 {
            return false;
        }
        let inlay_range = fold_range.start.to_inlay_point(fold_snapshot)
            ..fold_range.end.to_inlay_point(fold_snapshot);
        let inlay_snapshot = self.inlay_snapshot();
        let buffer_range = inlay_snapshot.to_buffer_point(inlay_range.start)
            ..inlay_snapshot.to_buffer_point(inlay_range.end);
        let buffer = self.buffer_snapshot();
        let buffer_offsets =
            buffer.point_to_offset(buffer_range.start)..buffer.point_to_offset(buffer_range.end);
        fold_snapshot.folds_in_range(buffer_range).next().is_none()
            && !inlay_snapshot.has_inlays_matching(inlay_range, |inlay, text_range| {
                has_chunk_renderer(inlay)
                    || !inlay
                        .text()
                        .chunks_in_range(text_range)
                        .all(|chunk| all_grid_bytes(chunk.as_bytes()))
            })
            && !self.control_rows.contains_control(buffer, buffer_offsets)
    }
}

pub(super) struct RecordedTabMap {
    tab_map: TabMap,
    pending: Option<Vec<Vec<TabEdit>>>,
    pending_len: usize,
}

impl RecordedTabMap {
    pub(super) fn new(tab_map: TabMap) -> Self {
        Self {
            tab_map,
            pending: Some(Vec::new()),
            pending_len: 0,
        }
    }

    pub(super) fn sync(
        &mut self,
        fold_snapshot: FoldSnapshot,
        fold_edits: Vec<FoldEdit>,
        tab_size: NonZeroU32,
    ) -> (TabSnapshot, Vec<TabEdit>) {
        let (snapshot, edits) = self.tab_map.sync(fold_snapshot, fold_edits, tab_size);
        if !edits.is_empty()
            && let Some(pending) = &mut self.pending
        {
            self.pending_len += edits.len();
            if self.pending_len > MAX_PENDING_TAB_EDITS {
                self.pending = None;
            } else {
                pending.push(edits.clone());
            }
        }
        (snapshot, edits)
    }

    fn take_edits(&mut self) -> Option<Vec<Vec<TabEdit>>> {
        self.pending_len = 0;
        std::mem::replace(&mut self.pending, Some(Vec::new()))
    }
}

#[derive(Default)]
pub(super) struct RulerDirt {
    pub(super) styles: DirtyRanges,
    pub(super) buffer_edits: DirtyRanges,
}

#[derive(Default)]
pub(super) struct DirtyRanges {
    all: bool,
    ranges: Vec<Range<Anchor>>,
}

impl DirtyRanges {
    pub(super) fn mark(&mut self, ranges: impl IntoIterator<Item = Range<Anchor>>) {
        if self.all {
            return;
        }
        self.ranges.extend(ranges);
        if self.ranges.len() > MAX_TRACKED_RULER_DIRT {
            self.mark_all();
        }
    }

    pub(super) fn mark_all(&mut self) {
        self.all = true;
        self.ranges.clear();
    }
}

#[derive(Debug)]
pub(super) struct SyntaxStamp {
    versions: Vec<(BufferId, usize)>,
    unstyled: bool,
}

impl SyntaxStamp {
    pub(super) fn new(buffer: &MultiBufferSnapshot, range: Range<MultiBufferOffset>) -> Self {
        let ranges = buffer.range_to_buffer_ranges(range);
        Self {
            versions: ranges
                .iter()
                .map(|(buffer, ..)| (buffer.remote_id(), buffer.syntax_update_count()))
                .collect(),
            unstyled: ranges.iter().all(|(buffer, range, _)| {
                buffer
                    .syntax_layers_for_range(range.start.0..range.end.0, true)
                    .next()
                    .is_none()
            }),
        }
    }

    fn still_applies(&self, current: &Self) -> bool {
        let same_buffers = || {
            self.versions
                .iter()
                .map(|(buffer_id, _)| buffer_id)
                .eq(current.versions.iter().map(|(buffer_id, _)| buffer_id))
        };
        self.versions == current.versions || self.unstyled && current.unstyled && same_buffers()
    }
}

pub(super) type DiagnosticKey = (usize, usize, lsp::DiagnosticSeverity, bool, bool);

pub(super) fn diagnostic_keys(
    buffer: &MultiBufferSnapshot,
    range: Range<MultiBufferOffset>,
) -> Vec<DiagnosticKey> {
    buffer
        .diagnostics_in_range(range.clone())
        .map(|entry| {
            (
                entry.range.start.0.wrapping_sub(range.start.0),
                entry.range.end.0.wrapping_sub(range.start.0),
                entry.diagnostic.severity,
                entry.diagnostic.underline,
                entry.diagnostic.is_unnecessary,
            )
        })
        .collect()
}

#[derive(Default)]
pub(super) struct ControlRows(Mutex<HashMap<MultiBufferOffset, ControlRow>>);

struct ControlRow {
    range: Range<Anchor>,
    end: MultiBufferOffset,
    has_control: bool,
    idle_refreshes: u8,
}

impl ControlRows {
    fn edited(&self, buffer: &MultiBufferSnapshot, dirty: &[Range<MultiBufferOffset>]) -> Self {
        let rows = self
            .0
            .lock()
            .values()
            .filter(|row| row.idle_refreshes < MAX_IDLE_REFRESHES)
            .filter_map(|row| {
                let start = row.range.start.to_offset(buffer);
                let end = row.range.end.to_offset(buffer);
                let start_point = buffer.offset_to_point(start);
                if start_point.column != 0
                    || start + buffer.line_len(MultiBufferRow(start_point.row)) as usize != end
                {
                    return None;
                }
                let mut touched = dirty
                    .iter()
                    .filter(|dirty| dirty.start <= end && start <= dirty.end)
                    .peekable();
                if row.has_control && touched.peek().is_some() {
                    return None;
                }
                let still_clean = touched.all(|dirty| {
                    buffer
                        .bytes_in_range(dirty.start.max(start)..dirty.end.min(end))
                        .all(all_grid_bytes)
                });
                still_clean.then(|| {
                    (
                        start,
                        ControlRow {
                            range: row.range.clone(),
                            end,
                            has_control: row.has_control,
                            idle_refreshes: row.idle_refreshes + 1,
                        },
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        Self(Mutex::new(rows))
    }

    fn contains_control(
        &self,
        buffer: &MultiBufferSnapshot,
        range: Range<MultiBufferOffset>,
    ) -> bool {
        if range.is_empty() {
            return false;
        }
        if let Some(row) = self.0.lock().get_mut(&range.start)
            && row.end == range.end
        {
            row.idle_refreshes = 0;
            return row.has_control;
        }
        let has_control = !buffer.bytes_in_range(range.clone()).all(all_grid_bytes);
        self.0.lock().insert(
            range.start,
            ControlRow {
                range: buffer.anchor_before(range.start)..buffer.anchor_after(range.end),
                end: range.end,
                has_control,
                idle_refreshes: 0,
            },
        );
        has_control
    }
}

pub(super) fn affects_shaping(style: &HighlightStyle) -> bool {
    style.color.is_some()
        || style.font_weight.is_some()
        || style.font_style.is_some()
        || style.underline.is_some()
        || style.strikethrough.is_some()
        || style.fade_out.is_some()
}

impl DisplayMap {
    pub(super) fn refresh_row_caches(&mut self, block_snapshot: &BlockSnapshot) {
        let tab_snapshot = &block_snapshot.wrap_snapshot.tab_snapshot;
        let buffer = &tab_snapshot.fold_snapshot.inlay_snapshot.buffer;
        let version = RulerCacheVersion {
            tabs: tab_snapshot.version,
            highlights: self.highlight_version,
            non_text_state: buffer.non_text_state_update_count(),
            diagnostics_max_severity: self.diagnostics_max_severity,
            tab_size: tab_snapshot.tab_size,
            masked: self.masked,
        };
        let tab_edits = self.tab_map.take_edits();
        let previous_version = self.row_rulers.version();
        if previous_version == version && tab_edits.as_ref().is_some_and(Vec::is_empty) {
            return;
        }
        let dirt = std::mem::take(&mut self.ruler_dirt);
        self.control_rows = if dirt.buffer_edits.all {
            Arc::default()
        } else {
            let edited = dirt
                .buffer_edits
                .ranges
                .iter()
                .map(|range| range.start.to_offset(buffer)..range.end.to_offset(buffer))
                .collect::<Vec<_>>();
            Arc::new(self.control_rows.edited(buffer, &edited))
        };
        let all_changed = dirt.styles.all
            || tab_edits.is_none()
            || previous_version.diagnostics_max_severity != version.diagnostics_max_severity
            || previous_version.tab_size != version.tab_size
            || previous_version.masked != version.masked;
        let tab_edits = tab_edits.unwrap_or_default();
        let diagnostics_changed = previous_version.non_text_state != version.non_text_state;
        let fold_snapshot = &tab_snapshot.fold_snapshot;
        let restyled_rows = dirt
            .styles
            .ranges
            .iter()
            .map(|range| {
                tab_row_for_offset(fold_snapshot, range.start.to_offset(buffer))
                    ..=tab_row_for_offset(fold_snapshot, range.end.to_offset(buffer))
            })
            .collect::<Vec<_>>();
        let remap = |row: u32, ruler: &RowRuler, recently_used: bool| {
            let mapped = map_row_through_tab_edits(row, ruler.len(), &tab_edits);
            let MappedRow::Unchanged(row) = mapped else {
                return mapped;
            };
            if !recently_used || all_changed || restyled_rows.iter().any(|rows| rows.contains(&row))
            {
                return MappedRow::Changed(row);
            }
            if diagnostics_changed || ruler.syntax().is_some() {
                let range = row_buffer_range(fold_snapshot, row);
                if diagnostics_changed
                    && ruler.diagnostics() != diagnostic_keys(buffer, range.clone())
                {
                    return MappedRow::Changed(row);
                }
                if ruler
                    .syntax()
                    .is_some_and(|stamp| !stamp.still_applies(&SyntaxStamp::new(buffer, range)))
                {
                    return MappedRow::Changed(row);
                }
            }
            mapped
        };
        self.row_rulers = Arc::new(RowRulerCache::new(version, Some(&self.row_rulers), remap));
    }

    pub(super) fn mark_all_rulers_dirty(&mut self) {
        self.ruler_dirt.styles.mark_all();
    }

    pub(super) fn mark_text_highlight_dirty(
        &mut self,
        key: HighlightKey,
        ranges: &[Range<Anchor>],
        style: HighlightStyle,
        merge: bool,
        buffer: &MultiBufferSnapshot,
    ) {
        let previous = self.text_highlights.get(&key);
        if !affects_shaping(&style)
            && !previous.is_some_and(|previous| affects_shaping(&previous.0))
        {
            return;
        }
        let previous_hull = previous
            .filter(|previous| !merge || previous.0 != style)
            .and_then(|previous| anchor_hull(&previous.1, buffer));
        let dirty = previous_hull
            .into_iter()
            .chain(anchor_hull(ranges, buffer))
            .collect::<Vec<_>>();
        self.highlight_version += 1;
        self.ruler_dirt.styles.mark(dirty);
    }

    pub(super) fn mark_inlay_highlight_dirty(
        &mut self,
        key: HighlightKey,
        highlights: &[InlayHighlight],
        style: HighlightStyle,
    ) {
        let previous_affects_shaping = self.inlay_highlights.get(&key).is_some_and(|highlights| {
            highlights
                .iter()
                .any(|(_, (style, _))| affects_shaping(style))
        });
        if !affects_shaping(&style) && !previous_affects_shaping {
            return;
        }
        self.highlight_version += 1;
        if highlights
            .iter()
            .any(|highlight| self.inlay_spans_rows(highlight.inlay))
        {
            self.mark_all_rulers_dirty();
            return;
        }
        let dirty = highlights
            .iter()
            .map(|highlight| highlight.inlay_position..highlight.inlay_position)
            .collect::<Vec<_>>();
        self.ruler_dirt.styles.mark(dirty);
    }

    pub(super) fn mark_removed_highlights_dirty(
        &mut self,
        text: Option<&(HighlightStyle, Vec<Range<Anchor>>)>,
        inlays: Option<&TreeMap<InlayId, (HighlightStyle, InlayHighlight)>>,
    ) {
        if inlays.is_some_and(|inlays| {
            inlays
                .iter()
                .any(|(id, (style, _))| affects_shaping(style) && self.inlay_spans_rows(*id))
        }) {
            self.highlight_version += 1;
            self.mark_all_rulers_dirty();
            return;
        }
        let dirty = text
            .filter(|(style, _)| affects_shaping(style))
            .into_iter()
            .flat_map(|(_, ranges)| ranges.iter().cloned())
            .chain(inlays.into_iter().flat_map(|inlays| {
                inlays
                    .iter()
                    .filter(|(_, (style, _))| affects_shaping(style))
                    .map(|(_, (_, highlight))| highlight.inlay_position..highlight.inlay_position)
            }))
            .collect::<Vec<_>>();
        if !dirty.is_empty() {
            self.highlight_version += 1;
            self.ruler_dirt.styles.mark(dirty);
        }
    }

    pub(super) fn mark_semantic_styles_dirty(&mut self, interner: &HighlightStyleInterner) {
        if interner.styles().any(affects_shaping) {
            self.highlight_version += 1;
            self.mark_all_rulers_dirty();
        }
    }

    fn inlay_spans_rows(&self, id: InlayId) -> bool {
        self.inlay_map
            .current_inlays()
            .any(|inlay| inlay.id == id && inlay.text().max_point().row > 0)
    }
}

fn is_grid_byte(byte: u8) -> bool {
    (byte >= 0x20 && byte != 0x7f) || byte == b'\t' || byte == b'\n'
}

fn all_grid_bytes(bytes: &[u8]) -> bool {
    let found_control = bytes.iter().fold(false, |found_control, byte| {
        found_control | !is_grid_byte(*byte)
    });
    !found_control
}

fn map_row_through_tab_edits(row: u32, len: u32, patches: &[Vec<TabEdit>]) -> MappedRow {
    let mut row = row;
    let mut kept = true;
    for edits in patches {
        let row_start = TabPoint::new(row, 0);
        let row_end = TabPoint::new(row, len);
        let old_row = row;
        let ix = edits.partition_point(|edit| edit.old.end <= row_start);
        if let Some(edit) = ix.checked_sub(1).and_then(|ix| edits.get(ix)) {
            kept &= edit.old.end < row_start || edit.new.end.column() == 0;
            row = row - edit.old.end.row() + edit.new.end.row();
        }
        if let Some(edit) = edits.get(ix)
            && edit.old.start <= row_end
        {
            kept = false;
            row = if edit.old.start.row() == old_row {
                edit.new.start.row()
            } else {
                edit.new.end.row()
            };
        }
    }
    if kept {
        MappedRow::Unchanged(row)
    } else {
        MappedRow::Changed(row)
    }
}

fn anchor_hull(ranges: &[Range<Anchor>], buffer: &MultiBufferSnapshot) -> Option<Range<Anchor>> {
    let start = ranges
        .iter()
        .map(|range| range.start)
        .min_by(|a, b| a.cmp(b, buffer))?;
    let end = ranges
        .iter()
        .map(|range| range.end)
        .max_by(|a, b| a.cmp(b, buffer))?;
    Some(start..end)
}

fn tab_row_for_offset(fold_snapshot: &FoldSnapshot, offset: MultiBufferOffset) -> u32 {
    let inlay_snapshot = &fold_snapshot.inlay_snapshot;
    let inlay_point = inlay_snapshot.to_point(inlay_snapshot.to_inlay_offset(offset));
    fold_snapshot.to_fold_point(inlay_point, Bias::Left).row()
}

pub(super) fn row_buffer_range(
    fold_snapshot: &FoldSnapshot,
    tab_row: u32,
) -> Range<MultiBufferOffset> {
    let inlay_snapshot = &fold_snapshot.inlay_snapshot;
    let to_buffer = |fold_point: FoldPoint| {
        let inlay_point = fold_point.to_inlay_point(fold_snapshot);
        inlay_snapshot.to_buffer_offset(inlay_snapshot.to_offset(inlay_point))
    };
    to_buffer(FoldPoint::new(tab_row, 0))
        ..to_buffer(FoldPoint::new(tab_row, fold_snapshot.line_len(tab_row)))
}

fn font_is_grid_exact(
    text_system: &WindowTextSystem,
    font: &Font,
    font_size: Pixels,
    cell_width: Pixels,
) -> bool {
    let font_id = text_system.resolve_font(font);
    text_system.ascii_shaping_preserves_advances(font)
        && (0x20u8..=0x7E)
            .all(|byte| text_system.layout_width(font_id, font_size, byte as char) == cell_width)
}

fn column_from_usize(column: usize) -> u32 {
    u32::try_from(column).unwrap_or(u32::MAX)
}
