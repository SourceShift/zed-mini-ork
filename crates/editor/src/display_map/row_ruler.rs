use std::{
    hash::{Hash, Hasher},
    num::NonZeroU32,
    ops::Range,
    sync::Arc,
};

use collections::{FxHasher, HashMap};
use gpui::{
    Font, FontStyle, FontWeight, HighlightStyle, Hsla, LineLayout, Pixels, StrikethroughStyle,
    TextRun, UnderlineStyle, WindowTextSystem,
};
use language::LanguageAwareStyling;
use parking_lot::Mutex;
use text::Bias;

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::{
    EditorStyle,
    display_map::{
        ChunkRendererId, ChunkReplacement, DisplayPoint, DisplayRow, DisplaySnapshot,
        HighlightedChunk,
        long_rows::{DiagnosticKey, SyntaxStamp, diagnostic_keys, row_buffer_range},
    },
    scroll::ScrollPixelOffset,
};
use project::{InlayId, project_settings::DiagnosticSeverity};

const CHUNK_LEN: u32 = 2_048;
const MAX_CHUNK_LEN: u32 = 3 * CHUNK_LEN;
const FORCED_BOUNDARY_SLACK: u32 = 1_024;
const SHAPING_CONTEXT: u32 = 64;
const MAX_SHAPED_LEN: u32 = 10_240;
pub(super) const MAX_IDLE_REFRESHES: u8 = 3;

const _: () = assert!(
    CHUNK_LEN + MAX_CHUNK_LEN + FORCED_BOUNDARY_SLACK + 4 + 2 * SHAPING_CONTEXT <= MAX_SHAPED_LEN
);

#[derive(Clone)]
pub(crate) struct RulerShaper {
    pub(crate) text_system: Arc<WindowTextSystem>,
    pub(crate) style: EditorStyle,
    pub(crate) font_size: Pixels,
    pub(crate) language_aware: LanguageAwareStyling,
}

impl RulerShaper {
    pub(crate) fn layout_columns(
        &self,
        snapshot: &DisplaySnapshot,
        row: DisplayRow,
        columns: Range<u32>,
    ) -> Arc<LineLayout> {
        let mut text = String::new();
        let mut runs = Vec::new();
        for chunk in self.chunks(snapshot, row, columns) {
            text.push_str(chunk.text);
            runs.push(self.run(chunk.style, chunk.text.len()));
        }
        self.layout(&text, &runs)
    }

    pub(crate) fn compose_columns(
        &self,
        snapshot: &DisplaySnapshot,
        row: DisplayRow,
        columns: Range<u32>,
    ) -> ComposedLayout {
        let mut composed = ComposedLayout::default();
        let mut text = String::new();
        let mut runs = Vec::new();
        for chunk in self.chunks(snapshot, row, columns) {
            let Some(replacement) = &chunk.replacement else {
                text.push_str(chunk.text);
                runs.push(self.run(chunk.style, chunk.text.len()));
                continue;
            };
            if !text.is_empty() {
                composed.push_text(self.layout(&text, &runs));
                text.clear();
                runs.clear();
            }
            let width = match replacement {
                ChunkReplacement::Str(replacement) => {
                    self.layout(replacement, &[self.run(chunk.style, replacement.len())])
                        .width
                }
                ChunkReplacement::Renderer(renderer) => {
                    renderer.measured_width.unwrap_or_else(|| {
                        self.layout(chunk.text, &[self.run(chunk.style, chunk.text.len())])
                            .width
                    })
                }
            };
            composed.push_replacement(chunk.text.len(), width);
        }
        if !text.is_empty() {
            composed.push_text(self.layout(&text, &runs));
        }
        composed
    }

    fn chunks<'a>(
        &'a self,
        snapshot: &'a DisplaySnapshot,
        row: DisplayRow,
        columns: Range<u32>,
    ) -> impl Iterator<Item = HighlightedChunk<'a>> + 'a {
        snapshot.highlighted_chunks_in_range(
            DisplayPoint::new(row, columns.start)..DisplayPoint::new(row, columns.end),
            self.language_aware,
            &self.style,
        )
    }

    fn run(&self, style: Option<HighlightStyle>, len: usize) -> TextRun {
        match style {
            Some(style) => self.style.text.clone().highlight(style).to_run(len),
            None => self.style.text.to_run(len),
        }
    }

    fn layout(&self, text: &str, runs: &[TextRun]) -> Arc<LineLayout> {
        self.text_system
            .layout_line(text, self.font_size, runs, None)
    }

    fn renderer_metrics_key(&self) -> u64 {
        renderer_metrics_key(&self.style.text.font(), self.font_size)
    }

    fn metrics_fingerprint(&self) -> u64 {
        let mut hasher = FxHasher::default();
        self.style.text.font().hash(&mut hasher);
        hasher.write_u32(f32::from(self.font_size).to_bits());
        self.style.text.color.hash(&mut hasher);
        self.style.text.underline.hash(&mut hasher);
        self.style.text.strikethrough.hash(&mut hasher);
        for style in self.style.syntax.highlights().chain([
            &self.style.inlay_hints_style,
            &self.style.edit_prediction_styles.insertion,
            &self.style.edit_prediction_styles.whitespace,
        ]) {
            HighlightStyle {
                background_color: None,
                ..*style
            }
            .hash(&mut hasher);
        }
        hasher.write_u8(self.language_aware.tree_sitter as u8);
        hasher.write_u8(self.language_aware.diagnostics as u8);
        hasher.write_u8(self.style.show_underlines as u8);
        hasher.write_u32(self.style.unnecessary_code_fade.to_bits());
        hasher.finish()
    }

    fn matches(&self, ruler: &RowRuler) -> bool {
        ruler.metrics_fingerprint == self.metrics_fingerprint()
    }
}

pub(crate) fn renderer_metrics_key(font: &Font, font_size: Pixels) -> u64 {
    let mut hasher = FxHasher::default();
    font.hash(&mut hasher);
    hasher.write_u32(f32::from(font_size).to_bits());
    hasher.finish()
}

#[derive(Default)]
pub(crate) struct ComposedLayout {
    pieces: Vec<ComposedPiece>,
    len: usize,
    width: Pixels,
}

struct ComposedPiece {
    start: usize,
    len: usize,
    x: Pixels,
    kind: PieceKind,
}

enum PieceKind {
    Text(Arc<LineLayout>),
    Replacement(Pixels),
}

impl ComposedLayout {
    pub(crate) fn width(&self) -> Pixels {
        self.width
    }

    pub(crate) fn x_for_index(&self, index: usize) -> Pixels {
        let Some(piece) = self.piece_at(index) else {
            return Pixels::ZERO;
        };
        match &piece.kind {
            PieceKind::Text(layout) => piece.x + layout.x_for_index(index - piece.start),
            PieceKind::Replacement(width) if index > piece.start => piece.x + *width,
            PieceKind::Replacement(_) => piece.x,
        }
    }

    pub(crate) fn closest_index_for_x(&self, x: Pixels) -> usize {
        let ix = self
            .pieces
            .partition_point(|piece| piece.x <= x)
            .saturating_sub(1);
        let Some(piece) = self.pieces.get(ix) else {
            return 0;
        };
        match &piece.kind {
            PieceKind::Text(layout) => piece.start + layout.closest_index_for_x(x - piece.x),
            PieceKind::Replacement(width) if x - piece.x > *width / 2. => piece.start + piece.len,
            PieceKind::Replacement(_) => piece.start,
        }
    }

    fn is_glyph_boundary(&self, index: usize) -> bool {
        if index >= self.len {
            return true;
        }
        self.piece_at(index).is_none_or(|piece| match &piece.kind {
            PieceKind::Text(layout) => layout.is_glyph_boundary(index - piece.start),
            PieceKind::Replacement(_) => index == piece.start,
        })
    }

    fn piece_at(&self, index: usize) -> Option<&ComposedPiece> {
        let ix = self
            .pieces
            .partition_point(|piece| piece.start <= index)
            .saturating_sub(1);
        self.pieces.get(ix)
    }

    fn push_text(&mut self, layout: Arc<LineLayout>) {
        let (len, width) = (layout.len, layout.width);
        self.push(len, width, PieceKind::Text(layout));
    }

    fn push_replacement(&mut self, len: usize, width: Pixels) {
        self.push(len, width, PieceKind::Replacement(width));
    }

    fn push(&mut self, len: usize, width: Pixels, kind: PieceKind) {
        self.pieces.push(ComposedPiece {
            start: self.len,
            len,
            x: self.width,
            kind,
        });
        self.len += len;
        self.width += width;
    }
}

#[derive(Default)]
struct ReplacedChars(Vec<u64>);

impl ReplacedChars {
    fn insert(&mut self, offset: u32) {
        let word = offset as usize / 64;
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        self.0[word] |= 1 << (offset % 64);
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn contains(&self, offset: u32) -> bool {
        self.0
            .get(offset as usize / 64)
            .is_some_and(|word| word & (1 << (offset % 64)) != 0)
    }

    fn in_range(&self, range: Range<u32>) -> impl Iterator<Item = u32> + '_ {
        let end = (range.end as usize).min(self.0.len() * 64);
        let mut offset = range.start as usize;
        std::iter::from_fn(move || {
            while offset < end {
                let word = self.0[offset / 64] >> (offset % 64);
                if word == 0 {
                    offset = (offset / 64 + 1) * 64;
                    continue;
                }
                let found = offset + word.trailing_zeros() as usize;
                if found >= end {
                    offset = end;
                    return None;
                }
                offset = found + 1;
                return Some(found as u32);
            }
            None
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct FontOverrides {
    weight: Option<FontWeight>,
    style: Option<FontStyle>,
}

impl FontOverrides {
    fn from_style(style: Option<HighlightStyle>) -> Self {
        Self {
            weight: style.and_then(|style| style.font_weight),
            style: style.and_then(|style| style.font_style),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Decorations {
    color: Hsla,
    underline: Option<UnderlineStyle>,
    strikethrough: Option<StrikethroughStyle>,
}

impl Decorations {
    fn from_run(run: &TextRun) -> Self {
        Self {
            color: run.color,
            underline: run.underline,
            strikethrough: run.strikethrough,
        }
    }
}

#[derive(Clone, Debug)]
struct StyleSegment {
    end: u32,
    overrides: FontOverrides,
    decorations: Decorations,
    style: Option<HighlightStyle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RulerCacheVersion {
    pub(crate) tabs: usize,
    pub(crate) highlights: usize,
    pub(crate) non_text_state: usize,
    pub(crate) diagnostics_max_severity: DiagnosticSeverity,
    pub(crate) tab_size: NonZeroU32,
    pub(crate) masked: bool,
}

pub(crate) struct RowRulerCache {
    version: RulerCacheVersion,
    rulers: Mutex<HashMap<u32, CachedRuler>>,
    previous: Mutex<HashMap<u32, Arc<RowRuler>>>,
    renderer_widths: Mutex<(u64, HashMap<InlayId, Pixels>)>,
}

struct CachedRuler {
    ruler: Arc<RowRuler>,
    idle_refreshes: u8,
}

pub(crate) enum MappedRow {
    Unchanged(u32),
    Changed(u32),
}

impl RowRulerCache {
    pub(crate) fn new(
        version: RulerCacheVersion,
        previous: Option<&RowRulerCache>,
        mut remap: impl FnMut(u32, &RowRuler, bool) -> MappedRow,
    ) -> Self {
        let (previous_rulers, retained, renderer_widths) = previous
            .map(|previous| {
                let rulers = previous
                    .rulers
                    .lock()
                    .iter()
                    .map(|(row, cached)| (*row, cached.ruler.clone(), cached.idle_refreshes))
                    .collect::<Vec<_>>();
                let mut retained = HashMap::default();
                let mut reusable = HashMap::default();
                for (row, ruler, idle_refreshes) in rulers.iter() {
                    match remap(*row, ruler, *idle_refreshes < MAX_IDLE_REFRESHES) {
                        MappedRow::Unchanged(row) => {
                            retained.insert(
                                row,
                                CachedRuler {
                                    ruler: ruler.clone(),
                                    idle_refreshes: idle_refreshes + 1,
                                },
                            );
                            reusable.insert(row, ruler.clone());
                        }
                        MappedRow::Changed(row) => {
                            reusable.insert(row, ruler.clone());
                        }
                    }
                }
                if rulers.is_empty() {
                    for (row, ruler) in previous.previous.lock().iter() {
                        let (MappedRow::Unchanged(row) | MappedRow::Changed(row)) =
                            remap(*row, ruler, false);
                        reusable.insert(row, ruler.clone());
                    }
                }
                (reusable, retained, previous.renderer_widths.lock().clone())
            })
            .unwrap_or_default();
        Self {
            version,
            rulers: Mutex::new(retained),
            previous: Mutex::new(previous_rulers),
            renderer_widths: Mutex::new(renderer_widths),
        }
    }

    pub(crate) fn version(&self) -> RulerCacheVersion {
        self.version
    }

    pub(crate) fn update_renderer_widths(
        &self,
        widths: impl IntoIterator<Item = (ChunkRendererId, Pixels)>,
        metrics_key: u64,
    ) -> bool {
        let mut renderer_widths = self.renderer_widths.lock();
        let mut changed = false;
        if renderer_widths.0 != metrics_key {
            changed = !renderer_widths.1.is_empty();
            *renderer_widths = (metrics_key, HashMap::default());
        }
        let renderer_widths = &mut renderer_widths.1;
        for (id, width) in widths {
            if let ChunkRendererId::Inlay(id) = id
                && renderer_widths.insert(id, width) != Some(width)
            {
                changed = true;
            }
        }
        if changed {
            let mut rulers = self.rulers.lock();
            if !rulers.is_empty() {
                *self.previous.lock() = std::mem::take(&mut *rulers)
                    .into_iter()
                    .map(|(row, cached)| (row, cached.ruler))
                    .collect();
            }
        }
        changed
    }

    pub(crate) fn remove_renderer_widths(&self, inlays: &[InlayId]) {
        let mut renderer_widths = self.renderer_widths.lock();
        for inlay in inlays {
            renderer_widths.1.remove(inlay);
        }
    }

    pub(crate) fn get_or_build(
        &self,
        tab_row: u32,
        shaper: &RulerShaper,
        build: impl FnOnce(Option<&RowRuler>, &HashMap<InlayId, Pixels>) -> RowRuler,
    ) -> Arc<RowRuler> {
        if let Some(cached) = self.rulers.lock().get_mut(&tab_row)
            && shaper.matches(&cached.ruler)
        {
            cached.idle_refreshes = 0;
            return cached.ruler.clone();
        }
        let previous = self
            .previous
            .lock()
            .get(&tab_row)
            .filter(|previous| shaper.matches(previous))
            .cloned();
        let renderer_widths = self.renderer_widths.lock();
        let no_widths = HashMap::default();
        let widths = if renderer_widths.0 == shaper.renderer_metrics_key() {
            &renderer_widths.1
        } else {
            &no_widths
        };
        let ruler = Arc::new(build(previous.as_deref(), widths));
        drop(renderer_widths);
        self.rulers.lock().insert(
            tab_row,
            CachedRuler {
                ruler: ruler.clone(),
                idle_refreshes: 0,
            },
        );
        ruler
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RenderPiece {
    pub(crate) chunk: Range<u32>,
    pub(crate) context: Range<u32>,
    pub(crate) width: Pixels,
    pub(crate) x: ScrollPixelOffset,
}

#[derive(Clone, Debug)]
struct RulerChunk {
    len: u32,
    width: Pixels,
    hash: u64,
    fixed: bool,
    context_before: u32,
    context_after: u32,
}

#[derive(Debug)]
pub(crate) struct RowRuler {
    metrics_fingerprint: u64,
    syntax: Option<SyntaxStamp>,
    diagnostics: Vec<DiagnosticKey>,
    chunks: Vec<RulerChunk>,
    starts: Vec<u32>,
    xs: Vec<ScrollPixelOffset>,
}

struct RowText {
    text: String,
    fixed_spans: Vec<(Range<u32>, Pixels)>,
    replaced: ReplacedChars,
    replacement_widths: HashMap<(char, FontOverrides), Pixels>,
    segments: Vec<StyleSegment>,
    forced_boundaries: Vec<u32>,
}

impl RowText {
    fn read(
        snapshot: &DisplaySnapshot,
        row: DisplayRow,
        shaper: &RulerShaper,
        renderer_widths: &HashMap<InlayId, Pixels>,
    ) -> Self {
        let row_len = snapshot.line_len(row);
        let mut text = String::with_capacity(row_len as usize);
        let mut fixed_spans = Vec::<(Range<u32>, Pixels)>::new();
        let mut segments = Vec::<StyleSegment>::new();
        let mut replaced = ReplacedChars::default();
        let mut replacement_widths = HashMap::<(char, FontOverrides), Pixels>::default();
        for chunk in shaper.chunks(snapshot, row, 0..row_len) {
            let start = text.len() as u32;
            text.push_str(chunk.text);
            let end = text.len() as u32;
            let overrides = FontOverrides::from_style(chunk.style);
            match segments.last_mut() {
                Some(segment) if segment.style == chunk.style => segment.end = end,
                last => {
                    let decorations = Decorations::from_run(&shaper.run(chunk.style, 0));
                    match last {
                        Some(segment)
                            if segment.overrides == overrides
                                && segment.decorations == decorations =>
                        {
                            segment.end = end
                        }
                        _ => segments.push(StyleSegment {
                            end,
                            overrides,
                            decorations,
                            style: chunk.style,
                        }),
                    }
                }
            }
            let Some(replacement) = chunk.replacement else {
                continue;
            };
            let width = match replacement {
                ChunkReplacement::Str(replacement) => {
                    let mut characters = chunk.text.chars();
                    if let (Some(character), None) = (characters.next(), characters.next()) {
                        replaced.insert(start);
                        replacement_widths
                            .entry((character, overrides))
                            .or_insert_with(|| {
                                let run = shaper.run(chunk.style, replacement.len());
                                shaper.layout(&replacement, &[run]).width
                            });
                        continue;
                    }
                    let run = shaper.run(chunk.style, replacement.len());
                    shaper.layout(&replacement, &[run]).width
                }
                ChunkReplacement::Renderer(renderer) => renderer
                    .measured_width
                    .or_else(|| match renderer.id {
                        ChunkRendererId::Inlay(id) => renderer_widths.get(&id).copied(),
                        ChunkRendererId::Fold(_) => None,
                    })
                    .unwrap_or_else(|| {
                        let run = shaper.run(chunk.style, chunk.text.len());
                        shaper.layout(chunk.text, &[run]).width
                    }),
            };
            fixed_spans.push((start..end, width));
        }
        Self {
            text,
            fixed_spans,
            replaced,
            replacement_widths,
            segments,
            forced_boundaries: Vec::new(),
        }
    }

    fn is_replacement_edge(&self, offset: u32) -> bool {
        if self.replaced.is_empty() {
            return false;
        }
        self.replaced.contains(offset)
            || self.text[..offset as usize]
                .chars()
                .next_back()
                .is_some_and(|before| self.replaced.contains(offset - before.len_utf8() as u32))
    }

    fn max_chunk_end(&self, start: u32) -> u32 {
        start
            .saturating_add(MAX_CHUNK_LEN)
            .min(self.text.len() as u32)
    }

    fn exceeds_max_chunk_len(&self) -> bool {
        self.max_chunk_end(0) < self.text.len() as u32
    }

    fn forced_boundaries(&self, boundaries: &GraphemeBoundaries, shaper: &RulerShaper) -> Vec<u32> {
        let len = self.text.len() as u32;
        let mut forced = Vec::new();
        let mut previous = 0;
        while previous < len {
            let window_end = self.max_chunk_end(previous);
            if window_end == len {
                break;
            }
            if let Some(safe) = self.last_shaping_boundary_in(previous, window_end, boundaries) {
                previous = safe;
                continue;
            }
            let slack_end = window_end.saturating_add(FORCED_BOUNDARY_SLACK).min(len);
            let boundary = self
                .forced_boundary_in(previous, window_end, slack_end, boundaries, shaper)
                .unwrap_or_else(|| {
                    let mut offset = slack_end as usize;
                    while !self.text.is_char_boundary(offset) {
                        offset += 1;
                    }
                    offset as u32
                });
            if boundary >= len {
                break;
            }
            forced.push(boundary);
            previous = boundary;
        }
        forced
    }

    fn forced_boundary_in(
        &self,
        previous: u32,
        start: u32,
        end: u32,
        boundaries: &GraphemeBoundaries,
        shaper: &RulerShaper,
    ) -> Option<u32> {
        let mut context_start = start.saturating_sub(SHAPING_CONTEXT).max(previous) as usize;
        while !self.text.is_char_boundary(context_start) {
            context_start += 1;
        }
        let mut layout_end = end as usize;
        while !self.text.is_char_boundary(layout_end) {
            layout_end -= 1;
        }
        let layout = self.layout_range(context_start as u32..layout_end as u32, shaper);
        boundaries
            .next_matching(start, end, |offset| {
                offset < end && layout.is_glyph_boundary((offset - context_start as u32) as usize)
            })
            .or_else(|| boundaries.next_matching(start, end, |_| true))
    }

    fn last_shaping_boundary_in(
        &self,
        after: u32,
        end: u32,
        boundaries: &GraphemeBoundaries,
    ) -> Option<u32> {
        let bytes = self.text.as_bytes();
        let end = end.min(bytes.len() as u32);
        let fixed_edge = self.last_fixed_span_edge_in(after, end);
        let floor = fixed_edge.unwrap_or(after);
        let mut position = end;
        while position > floor {
            let offset = position as usize;
            let beside_isolated_byte = bytes[offset - 1..]
                .iter()
                .take(2)
                .any(|byte| may_be_shaping_isolated(*byte));
            if (beside_isolated_byte
                || self.text.is_char_boundary(offset) && self.is_replacement_edge(position))
                && self.is_shaping_boundary_between_graphemes(position)
                && boundaries.is_boundary(position)
            {
                return Some(position);
            }
            position -= 1;
        }
        fixed_edge
    }

    fn last_fixed_span_edge_in(&self, after: u32, end: u32) -> Option<u32> {
        let first_after_end = self
            .fixed_spans
            .partition_point(|(span, _)| span.start <= end);
        self.fixed_spans[..first_after_end]
            .iter()
            .rev()
            .take_while(|(span, _)| span.end > after)
            .flat_map(|(span, _)| [span.end, span.start])
            .find(|edge| after < *edge && *edge <= end)
    }

    fn is_forced_boundary(&self, offset: u32) -> bool {
        self.forced_boundaries.binary_search(&offset).is_ok()
    }

    fn next_forced_boundary_after(&self, offset: u32) -> u32 {
        let ix = self
            .forced_boundaries
            .partition_point(|forced| *forced <= offset);
        self.forced_boundaries
            .get(ix)
            .copied()
            .unwrap_or(self.text.len() as u32)
    }

    fn last_forced_boundary_at_or_before(&self, offset: u32) -> u32 {
        let ix = self
            .forced_boundaries
            .partition_point(|forced| *forced <= offset);
        ix.checked_sub(1).map_or(0, |ix| self.forced_boundaries[ix])
    }

    fn is_shaping_boundary(&self, offset: u32, boundaries: &GraphemeBoundaries) -> bool {
        if offset == 0 || offset == self.text.len() as u32 || self.is_forced_boundary(offset) {
            return true;
        }
        boundaries.is_boundary(offset) && self.is_shaping_boundary_between_graphemes(offset)
    }

    fn is_shaping_boundary_between_graphemes(&self, offset: u32) -> bool {
        if !self.text.is_char_boundary(offset as usize) {
            return false;
        }
        if self.is_replacement_edge(offset) {
            return true;
        }
        let offset = offset as usize;
        let span_edge = self
            .fixed_spans
            .partition_point(|(span, _)| span.end < offset as u32);
        if self.fixed_spans[span_edge..]
            .iter()
            .take_while(|(span, _)| span.start <= offset as u32)
            .any(|(span, _)| span.start == offset as u32 || span.end == offset as u32)
        {
            return true;
        }
        let (Some(before), Some(after)) = (
            self.text[..offset].chars().next_back(),
            self.text[offset..].chars().next(),
        ) else {
            return false;
        };
        is_shaping_isolated(before) || is_shaping_isolated(after)
    }

    fn context_of(&self, range: Range<u32>) -> Range<u32> {
        let stretch_start = self
            .fixed_spans
            .partition_point(|(span, _)| span.end <= range.start);
        let stretch = if stretch_start == 0 {
            0
        } else {
            self.fixed_spans[stretch_start - 1].0.end
        }
            ..self
                .fixed_spans
                .get(stretch_start)
                .map_or(self.text.len() as u32, |(span, _)| span.start);
        let mut start = range
            .start
            .saturating_sub(SHAPING_CONTEXT)
            .max(stretch.start)
            .max(self.last_forced_boundary_at_or_before(range.start))
            as usize;
        while !self.text.is_char_boundary(start) {
            start += 1;
        }
        let mut end = range
            .end
            .saturating_add(SHAPING_CONTEXT)
            .min(stretch.end)
            .min(self.next_forced_boundary_after(range.end.saturating_sub(1)))
            as usize;
        while !self.text.is_char_boundary(end) {
            end += 1;
        }
        start as u32..end as u32
    }

    fn segments_in(&self, range: Range<u32>) -> impl Iterator<Item = (Range<u32>, &StyleSegment)> {
        let first = self
            .segments
            .partition_point(|segment| segment.end <= range.start);
        let mut segments = self.segments[first..].iter();
        let mut start = if first == 0 {
            0
        } else {
            self.segments[first - 1].end
        };
        std::iter::from_fn(move || {
            if start >= range.end {
                return None;
            }
            let segment = segments.next()?;
            let segment_range = start.max(range.start)..segment.end.min(range.end);
            start = segment.end;
            Some((segment_range, segment))
        })
    }

    fn hash_chunk(&self, range: Range<u32>, fixed_width: Option<Pixels>) -> u64 {
        let mut hasher = FxHasher::default();
        let context = match fixed_width {
            Some(width) => {
                hasher.write_u8(1);
                hasher.write_u32(f32::from(width).to_bits());
                range
            }
            None => {
                let context = self.context_of(range.clone());
                hasher.write_u8(0);
                hasher.write_u32(range.start - context.start);
                hasher.write_u32(context.end - range.end);
                context
            }
        };
        hasher.write(&self.text.as_bytes()[context.start as usize..context.end as usize]);
        for replaced in self.replaced.in_range(context.clone()) {
            hasher.write_u32(replaced - context.start);
        }
        for (segment_range, segment) in self.segments_in(context) {
            hasher.write_u32(segment_range.end - segment_range.start);
            segment.overrides.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn layout_range(&self, range: Range<u32>, shaper: &RulerShaper) -> ComposedLayout {
        let mut composed = ComposedLayout::default();
        let mut text_start = range.start;
        for start in self.replaced.in_range(range.clone()) {
            let Some(character) = self.text[start as usize..].chars().next() else {
                break;
            };
            if text_start < start {
                composed.push_text(self.layout_text(text_start..start, shaper));
            }
            let overrides = self
                .segments_in(start..start + 1)
                .next()
                .map(|(_, segment)| segment.overrides)
                .unwrap_or_default();
            let width = self
                .replacement_widths
                .get(&(character, overrides))
                .copied()
                .unwrap_or_default();
            composed.push_replacement(character.len_utf8(), width);
            text_start = start + character.len_utf8() as u32;
        }
        if text_start < range.end {
            composed.push_text(self.layout_text(text_start..range.end, shaper));
        }
        composed
    }

    fn layout_text(&self, range: Range<u32>, shaper: &RulerShaper) -> Arc<LineLayout> {
        let runs = self
            .segments_in(range.clone())
            .map(|(segment_range, segment)| {
                shaper.run(
                    segment.style,
                    (segment_range.end - segment_range.start) as usize,
                )
            })
            .collect::<Vec<_>>();
        shaper.layout(&self.text[range.start as usize..range.end as usize], &runs)
    }

    fn layout_chunk(&self, range: Range<u32>, shaper: &RulerShaper) -> Pixels {
        let context = self.context_of(range.clone());
        let layout = self.layout_range(context.clone(), shaper);
        let start_x = if context.start < range.start {
            layout.x_for_index((range.start - context.start) as usize)
        } else {
            Pixels::ZERO
        };
        let end_x = if range.end < context.end {
            layout.x_for_index((range.end - context.start) as usize)
        } else {
            layout.width()
        };
        end_x - start_x
    }
}

fn may_be_shaping_isolated(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte >= 0x80
}

fn is_shaping_isolated(char: char) -> bool {
    char.is_whitespace()
        || matches!(
            u32::from(char),
            0x3000..=0x30FF
                | 0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xAC00..=0xD7AF
                | 0xF900..=0xFAFF
                | 0xFF00..=0xFFEF
                | 0x20000..=0x3134F
        )
}

impl RowRuler {
    pub(crate) fn new(
        snapshot: &DisplaySnapshot,
        row: DisplayRow,
        shaper: &RulerShaper,
        previous: Option<&RowRuler>,
        renderer_widths: &HashMap<InlayId, Pixels>,
    ) -> Self {
        let mut row_text = RowText::read(snapshot, row, shaper, renderer_widths);
        if row_text.exceeds_max_chunk_len() {
            let forced =
                row_text.forced_boundaries(&GraphemeBoundaries::new(&row_text.text), shaper);
            row_text.forced_boundaries = forced;
        }
        let boundaries = GraphemeBoundaries::new(&row_text.text);
        let (prefix, suffix) = previous.map_or((Vec::new(), Vec::new()), |previous| {
            previous.reusable_chunks(&row_text, &boundaries)
        });
        let middle_start = prefix.iter().map(|chunk| chunk.len).sum::<u32>();
        let middle_end =
            row_text.text.len() as u32 - suffix.iter().map(|chunk| chunk.len).sum::<u32>();
        let mut chunks = prefix;
        chunks.extend(chunk_row_text(
            &row_text,
            middle_start..middle_end,
            &boundaries,
            shaper,
        ));
        chunks.extend(suffix);

        let mut starts = Vec::with_capacity(chunks.len() + 1);
        let mut xs = Vec::with_capacity(chunks.len() + 1);
        starts.push(0);
        xs.push(0.);
        for chunk in &chunks {
            starts.push(starts.last().copied().unwrap_or(0) + chunk.len);
            xs.push(xs.last().copied().unwrap_or(0.) + ScrollPixelOffset::from(chunk.width));
        }
        let buffer = snapshot.buffer_snapshot();
        let tab_row = snapshot
            .unwrapped_tab_point(DisplayPoint::new(row, 0), Bias::Left)
            .row();
        let buffer_range = row_buffer_range(snapshot.fold_snapshot(), tab_row);
        Self {
            metrics_fingerprint: shaper.metrics_fingerprint(),
            syntax: shaper
                .language_aware
                .tree_sitter
                .then(|| SyntaxStamp::new(buffer, buffer_range.clone())),
            diagnostics: diagnostic_keys(buffer, buffer_range),
            chunks,
            starts,
            xs,
        }
    }

    pub(super) fn syntax(&self) -> Option<&SyntaxStamp> {
        self.syntax.as_ref()
    }

    pub(super) fn diagnostics(&self) -> &[DiagnosticKey] {
        &self.diagnostics
    }

    pub(crate) fn len(&self) -> u32 {
        self.starts.last().copied().unwrap_or(0)
    }

    pub(crate) fn width(&self) -> ScrollPixelOffset {
        self.xs.last().copied().unwrap_or(0.)
    }

    pub(crate) fn x_for_column(
        &self,
        column: u32,
        snapshot: &DisplaySnapshot,
        row: DisplayRow,
        shaper: &RulerShaper,
    ) -> ScrollPixelOffset {
        if column >= self.len() {
            return self.width();
        }
        let ix = self
            .starts
            .partition_point(|start| *start <= column)
            .saturating_sub(1);
        let start = self.starts[ix];
        let x = self.xs[ix];
        if self.chunks[ix].fixed || column == start {
            return x;
        }
        let context = self.chunk_context(ix);
        let layout = shaper.compose_columns(snapshot, row, context.clone());
        let origin = chunk_origin(&layout, &context, start);
        x + ScrollPixelOffset::from(layout.x_for_index((column - context.start) as usize) - origin)
    }

    pub(crate) fn column_for_x(
        &self,
        x: ScrollPixelOffset,
        snapshot: &DisplaySnapshot,
        row: DisplayRow,
        shaper: &RulerShaper,
    ) -> u32 {
        if self.chunks.is_empty() {
            return 0;
        }
        if x <= 0. {
            return 0;
        }
        if x >= self.width() {
            return self.len();
        }
        let ix = self
            .xs
            .partition_point(|chunk_x| *chunk_x <= x)
            .saturating_sub(1)
            .min(self.chunks.len() - 1);
        let start = self.starts[ix];
        let chunk_x = self.xs[ix];
        let chunk = &self.chunks[ix];
        if chunk.fixed {
            return if x - chunk_x > ScrollPixelOffset::from(chunk.width) / 2. {
                start + chunk.len
            } else {
                start
            };
        }
        let context = self.chunk_context(ix);
        let layout = shaper.compose_columns(snapshot, row, context.clone());
        let origin = chunk_origin(&layout, &context, start);
        let column =
            context.start + layout.closest_index_for_x(Pixels::from(x - chunk_x) + origin) as u32;
        column.clamp(start, start + chunk.len)
    }

    pub(crate) fn columns_for_x_range(&self, x: Range<ScrollPixelOffset>) -> Range<u32> {
        if self.chunks.is_empty() {
            return 0..0;
        }
        let last = self.chunks.len();
        let first = self
            .xs
            .partition_point(|chunk_x| *chunk_x <= x.start)
            .saturating_sub(1)
            .min(last - 1);
        let end = self
            .xs
            .partition_point(|chunk_x| *chunk_x < x.end)
            .clamp(first + 1, last);
        self.starts[first]..self.starts[end]
    }

    pub(crate) fn x_range_for_columns(&self, columns: Range<u32>) -> Range<ScrollPixelOffset> {
        let (first, end) = self.chunk_ixs_for_columns(columns);
        self.xs[first]..self.xs[end]
    }

    pub(crate) fn render_pieces(
        &self,
        columns: Range<u32>,
    ) -> impl Iterator<Item = RenderPiece> + '_ {
        let (first, end) = self.chunk_ixs_for_columns(columns);
        (first..end).map(|ix| RenderPiece {
            chunk: self.starts[ix]..self.starts[ix + 1],
            context: self.chunk_context(ix),
            width: self.chunks[ix].width,
            x: self.xs[ix],
        })
    }

    fn chunk_ixs_for_columns(&self, columns: Range<u32>) -> (usize, usize) {
        let first = self
            .starts
            .partition_point(|start| *start <= columns.start)
            .saturating_sub(1)
            .min(self.chunks.len());
        let end = self
            .starts
            .partition_point(|start| *start < columns.end)
            .clamp(first, self.chunks.len());
        (first, end)
    }

    fn chunk_context(&self, ix: usize) -> Range<u32> {
        let chunk = &self.chunks[ix];
        self.starts[ix] - chunk.context_before..self.starts[ix + 1] + chunk.context_after
    }

    fn reusable_chunks(
        &self,
        row_text: &RowText,
        boundaries: &GraphemeBoundaries,
    ) -> (Vec<RulerChunk>, Vec<RulerChunk>) {
        let new_len = row_text.text.len() as u32;
        let mut prefix_count = 0;
        let mut position = 0;
        for chunk in &self.chunks {
            let end = position + chunk.len;
            if end > new_len || chunk_hash(row_text, position..end, boundaries) != Some(chunk.hash)
            {
                break;
            }
            position = end;
            prefix_count += 1;
        }
        if prefix_count == self.chunks.len() && position == new_len {
            return (self.chunks.clone(), Vec::new());
        }
        let mismatch_start = position;

        let shift = i64::from(new_len) - i64::from(self.len());
        let mut suffix_count = 0;
        for ix in (prefix_count..self.chunks.len()).rev() {
            let old_start = i64::from(self.starts[ix]) + shift;
            let old_end = i64::from(self.starts[ix + 1]) + shift;
            if old_start < i64::from(mismatch_start) {
                break;
            }
            let range = old_start as u32..old_end as u32;
            if chunk_hash(row_text, range, boundaries) != Some(self.chunks[ix].hash) {
                break;
            }
            suffix_count += 1;
        }
        (
            self.chunks[..prefix_count].to_vec(),
            self.chunks[self.chunks.len() - suffix_count..].to_vec(),
        )
    }
}

fn chunk_origin(layout: &ComposedLayout, context: &Range<u32>, start: u32) -> Pixels {
    if context.start < start {
        layout.x_for_index((start - context.start) as usize)
    } else {
        Pixels::ZERO
    }
}

fn chunk_row_text(
    row_text: &RowText,
    range: Range<u32>,
    boundaries: &GraphemeBoundaries,
    shaper: &RulerShaper,
) -> Vec<RulerChunk> {
    let mut chunks = Vec::new();
    let mut position = range.start;
    let mut spans = row_text
        .fixed_spans
        .iter()
        .skip_while(|(span, _)| span.end <= range.start)
        .peekable();
    while position < range.end {
        if let Some((span, width)) = spans.peek()
            && span.start <= position
        {
            let end = span.end.min(range.end);
            chunks.push(RulerChunk {
                len: end - position,
                width: *width,
                hash: row_text.hash_chunk(position..end, Some(*width)),
                fixed: true,
                context_before: 0,
                context_after: 0,
            });
            position = end;
            spans.next();
            continue;
        }
        let stretch_end = spans
            .peek()
            .map_or(range.end, |(span, _)| span.start.min(range.end));
        let limit = stretch_end.min(row_text.next_forced_boundary_after(position));
        let target = position.saturating_add(CHUNK_LEN).min(limit);
        let end = if target >= limit {
            limit
        } else {
            boundaries
                .next_matching(target, limit, |offset| {
                    offset == limit || row_text.is_shaping_boundary_between_graphemes(offset)
                })
                .unwrap_or(limit)
        };
        let context = row_text.context_of(position..end);
        chunks.push(RulerChunk {
            len: end - position,
            width: row_text.layout_chunk(position..end, shaper),
            hash: row_text.hash_chunk(position..end, None),
            fixed: false,
            context_before: position - context.start,
            context_after: context.end - end,
        });
        position = end;
    }
    chunks
}

fn chunk_hash(
    row_text: &RowText,
    range: Range<u32>,
    boundaries: &GraphemeBoundaries,
) -> Option<u64> {
    if !row_text.is_shaping_boundary(range.start, boundaries)
        || !row_text.is_shaping_boundary(range.end, boundaries)
    {
        return None;
    }
    let fixed_spans = &row_text.fixed_spans;
    let first_overlapping = fixed_spans.partition_point(|(span, _)| span.end <= range.start);
    let overlapping = fixed_spans[first_overlapping..]
        .iter()
        .take_while(|(span, _)| span.start < range.end)
        .collect::<Vec<_>>();
    let fixed_width = match overlapping.as_slice() {
        [] => None,
        [(span, width)] if *span == range => Some(*width),
        _ => return None,
    };
    Some(row_text.hash_chunk(range, fixed_width))
}

const REGIONAL_INDICATOR_PREFIX: &[u8] = b"\xF0\x9F\x87";

enum GraphemeBoundaries<'a> {
    Ascii { len: u32 },
    Segmented { text: &'a str },
    Indexed { len: u32, bits: Vec<u64> },
}

impl<'a> GraphemeBoundaries<'a> {
    fn new(text: &'a str) -> Self {
        let len = text.len() as u32;
        if text.is_ascii() {
            return Self::Ascii { len };
        }
        let bytes = text.as_bytes();
        let has_regional_indicators = bytes.contains(&REGIONAL_INDICATOR_PREFIX[0])
            && bytes
                .windows(REGIONAL_INDICATOR_PREFIX.len())
                .any(|window| window == REGIONAL_INDICATOR_PREFIX);
        if !has_regional_indicators {
            return Self::Segmented { text };
        }
        let mut bits = vec![0u64; text.len() / 64 + 1];
        for (offset, _) in text.grapheme_indices(true) {
            bits[offset / 64] |= 1 << (offset % 64);
        }
        bits[text.len() / 64] |= 1 << (text.len() % 64);
        Self::Indexed { len, bits }
    }

    fn len(&self) -> u32 {
        match self {
            Self::Ascii { len } | Self::Indexed { len, .. } => *len,
            Self::Segmented { text } => text.len() as u32,
        }
    }

    fn is_boundary(&self, offset: u32) -> bool {
        if offset > self.len() {
            return false;
        }
        match self {
            Self::Ascii { .. } => true,
            Self::Segmented { text } => {
                let offset = offset as usize;
                text.is_char_boundary(offset)
                    && GraphemeCursor::new(offset, text.len(), true).is_boundary(text, 0)
                        == Ok(true)
            }
            Self::Indexed { bits, .. } => {
                let offset = offset as usize;
                bits[offset / 64] & (1 << (offset % 64)) != 0
            }
        }
    }

    fn next_matching(
        &self,
        offset: u32,
        limit: u32,
        mut accept: impl FnMut(u32) -> bool,
    ) -> Option<u32> {
        let limit = limit.min(self.len());
        let offset = offset.min(limit);
        match self {
            Self::Ascii { .. } => (offset..=limit).find(|offset| accept(*offset)),
            Self::Segmented { text } => {
                let mut boundary = offset as usize;
                while !text.is_char_boundary(boundary) {
                    boundary += 1;
                }
                let mut limit = limit as usize;
                while !text.is_char_boundary(limit) {
                    limit -= 1;
                }
                if boundary > limit {
                    return None;
                }
                let searchable = &text[..limit];
                let mut cursor = GraphemeCursor::new(boundary, text.len(), true);
                if boundary == limit || cursor.is_boundary(searchable, 0) == Ok(true) {
                    if accept(boundary as u32) {
                        return Some(boundary as u32);
                    }
                    if boundary == limit {
                        return None;
                    }
                }
                loop {
                    match cursor.next_boundary(searchable, 0) {
                        Ok(Some(boundary)) if accept(boundary as u32) => {
                            return Some(boundary as u32);
                        }
                        Ok(Some(_)) => {}
                        Ok(None) => return accept(limit as u32).then_some(limit as u32),
                        Err(_) => return None,
                    }
                }
            }
            Self::Indexed { bits, .. } => {
                let mut offset = offset as usize;
                let limit = limit as usize;
                loop {
                    let mut word_ix = offset / 64;
                    let mut word = bits[word_ix] & (u64::MAX << (offset % 64));
                    while word == 0 && (word_ix + 1) * 64 <= limit {
                        word_ix += 1;
                        word = bits[word_ix];
                    }
                    if word == 0 {
                        return None;
                    }
                    let boundary = word_ix * 64 + word.trailing_zeros() as usize;
                    if boundary > limit {
                        return None;
                    }
                    if accept(boundary as u32) {
                        return Some(boundary as u32);
                    }
                    offset = boundary + 1;
                    if offset > limit {
                        return None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Inlay, MAX_LINE_LEN, display_map::HighlightKey, hover_links::InlayHighlight,
        test::editor_test_context::EditorTestContext,
    };
    use gpui::TestAppContext;
    use language::Point;
    use multi_buffer::MultiBufferOffset;
    use project::InlayId;
    use rand::prelude::*;
    use settings::SettingsStore;

    #[gpui::test]
    async fn test_ruler_chunks_match_full_row_layout(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let text = format!(
            "{}\u{1}{}e\u{301}{}",
            "e\u{301}".repeat(CHUNK_LEN as usize / 3 + 5),
            "🙂".repeat(MAX_LINE_LEN),
            "x".repeat(3_000)
        );
        let (ruler, snapshot, shaper) = ruler_for_text(&mut cx, &text);
        let full_row = shaper.compose_columns(&snapshot, DisplayRow(0), 0..text.len() as u32);

        assert_eq!(ruler.len(), text.len() as u32);
        assert!(ruler.chunks.len() >= 3);
        assert_eq!(ruler.chunks.iter().filter(|chunk| chunk.fixed).count(), 0);
        let boundaries = GraphemeBoundaries::new(&text);
        assert!(
            ruler
                .starts
                .iter()
                .all(|start| boundaries.is_boundary(*start))
        );
        let tolerance = ScrollPixelOffset::from(full_row.width()) * 1e-4;
        assert!((ruler.width() - ScrollPixelOffset::from(full_row.width())).abs() < tolerance);
        for column in (0..=text.len())
            .step_by(97)
            .filter(|column| text.is_char_boundary(*column))
        {
            let x = ruler.x_for_column(column as u32, &snapshot, DisplayRow(0), &shaper);
            let full_x = ScrollPixelOffset::from(full_row.x_for_index(column));
            assert!(
                (x - full_x).abs() < tolerance,
                "column {column}: {x} != {full_x}"
            );
            assert_eq!(
                ruler.column_for_x(x, &snapshot, DisplayRow(0), &shaper),
                column as u32
            );
        }
    }

    #[gpui::test]
    async fn test_ruler_reuses_unchanged_chunks_across_edits(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let mut rulers_after_edit = |text: &str, column: u32, insert: &str| {
            let (before, _, shaper) = ruler_for_text(&mut cx, text);
            cx.update_editor(|editor, _, cx| {
                editor.edit([(Point::new(0, column)..Point::new(0, column), insert)], cx);
            });
            let snapshot =
                cx.update_editor(|editor, window, cx| editor.snapshot(window, cx).display_snapshot);
            let no_widths = HashMap::default();
            let reused =
                RowRuler::new(&snapshot, DisplayRow(0), &shaper, Some(&before), &no_widths);
            let fresh = RowRuler::new(&snapshot, DisplayRow(0), &shaper, None, &no_widths);
            let edited_text = snapshot.text();
            let boundaries = GraphemeBoundaries::new(&edited_text);
            assert_eq!(reused.len(), edited_text.len() as u32);
            assert!(
                reused
                    .starts
                    .iter()
                    .all(|start| boundaries.is_boundary(*start))
            );
            assert!((reused.width() - fresh.width()).abs() < fresh.width() * 1e-6);
            (before, reused)
        };

        let text = "漢字".repeat(MAX_LINE_LEN * 4);
        let (before, reused) = rulers_after_edit(&text, text.len() as u32 / 2, "ab");
        assert!(before.chunks.len() >= 8);
        let same_hash = |(old, new): &(&RulerChunk, &RulerChunk)| old.hash == new.hash;
        let prefix = before
            .chunks
            .iter()
            .zip(&reused.chunks)
            .take_while(same_hash)
            .count();
        let suffix = before
            .chunks
            .iter()
            .rev()
            .zip(reused.chunks.iter().rev())
            .take_while(same_hash)
            .count();
        assert!(prefix > 0 && suffix > 0);
        assert!(reused.chunks.len() - prefix - suffix <= 4);
    }

    #[gpui::test]
    async fn test_rulers_follow_edits_and_highlights(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let long = "漢字".repeat(MAX_LINE_LEN);
        cx.set_state(&format!("short\nˇ{long}\nother"));
        let ruler_for = |cx: &mut EditorTestContext, row: u32| {
            cx.update_editor(|editor, window, cx| {
                let snapshot = editor.snapshot(window, cx).display_snapshot;
                let shaper = editor
                    .text_layout_details(window, cx)
                    .ruler_shaper(&snapshot, DisplayRow(row));
                snapshot.ruled_row(DisplayRow(row), shaper).ruler
            })
        };
        let edit = |cx: &mut EditorTestContext, at: Point, text: &str| {
            cx.update_editor(|editor, _, cx| editor.edit([(at..at, text)], cx));
        };
        let highlight = |cx: &mut EditorTestContext, key, row: u32, columns: Range<u32>, style| {
            cx.update_editor(|editor, _, cx| {
                let buffer = editor.buffer().read(cx).snapshot(cx);
                let range = buffer.anchor_before(Point::new(row, columns.start))
                    ..buffer.anchor_after(Point::new(row, columns.end));
                editor.highlight_text_key(key, vec![range], style, false, cx);
            });
        };
        let clear = |cx: &mut EditorTestContext, key| {
            cx.update_editor(|editor, _, cx| editor.clear_highlights(key, cx));
        };
        let bold = HighlightStyle {
            font_weight: Some(FontWeight::BOLD),
            ..HighlightStyle::default()
        };
        let background = HighlightStyle {
            background_color: Some(gpui::red()),
            ..HighlightStyle::default()
        };
        let (read, write, search) = (
            HighlightKey::DocumentHighlightRead,
            HighlightKey::DocumentHighlightWrite,
            HighlightKey::BufferSearchHighlights,
        );
        let bold_columns = 3_000..3_012;

        let first = ruler_for(&mut cx, 1);
        edit(&mut cx, Point::new(2, 0), "x");
        assert!(Arc::ptr_eq(&first, &ruler_for(&mut cx, 1)));
        edit(&mut cx, Point::new(0, 0), "above\n");
        assert!(Arc::ptr_eq(&first, &ruler_for(&mut cx, 2)));
        highlight(&mut cx, read, 3, 0..1, bold);
        assert!(Arc::ptr_eq(&first, &ruler_for(&mut cx, 2)));
        highlight(&mut cx, write, 2, bold_columns.clone(), background);
        assert!(Arc::ptr_eq(&first, &ruler_for(&mut cx, 2)));
        clear(&mut cx, write);
        assert!(Arc::ptr_eq(&first, &ruler_for(&mut cx, 2)));

        highlight(&mut cx, search, 2, bold_columns.clone(), bold);
        let bolded = ruler_for(&mut cx, 2);
        let changed = (0..first.chunks.len())
            .filter(|ix| first.chunks[*ix].hash != bolded.chunks[*ix].hash)
            .collect::<Vec<_>>();
        let affected = (0..first.chunks.len())
            .filter(|ix| {
                let context = first.chunk_context(*ix);
                context.start < bold_columns.end && bold_columns.start < context.end
            })
            .collect::<Vec<_>>();
        assert_eq!((&first.starts, &changed), (&bolded.starts, &affected));
        assert!(!affected.is_empty() && affected.len() <= 2);
        clear(&mut cx, search);
        let unbolded = ruler_for(&mut cx, 2);
        assert!(!Arc::ptr_eq(&bolded, &unbolded));

        edit(&mut cx, Point::new(2, 3), "漢");
        let edited = ruler_for(&mut cx, 2);
        assert!(!Arc::ptr_eq(&unbolded, &edited));
        assert_eq!(edited.len(), long.len() as u32 + 3);
        edit(&mut cx, Point::new(2, 0), "\n");
        assert!(Arc::ptr_eq(&edited, &ruler_for(&mut cx, 3)));
    }

    #[gpui::test]
    async fn test_rows_chunk_only_beside_isolated_chars_or_at_hard_breaks(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let spaced = format!(
            "{} {},{}",
            "x".repeat(5_000),
            "y".repeat(3_000),
            "z".repeat(3_000)
        );
        let ligatures = format!("\u{754c}aa{}", "!=".repeat(1_050));
        let punctuation = "!=".repeat(10_000);
        for (text, inner_starts) in [
            (&spaced, vec![5_000]),
            (&ligatures, Vec::new()),
            (
                &punctuation,
                vec![MAX_CHUNK_LEN, 2 * MAX_CHUNK_LEN, 3 * MAX_CHUNK_LEN],
            ),
        ] {
            let (ruler, ..) = ruler_for_text(&mut cx, text);
            let mut starts = vec![0];
            starts.extend(inner_starts);
            starts.push(text.len() as u32);
            assert_eq!(ruler.starts, starts);
            let contexts = ruler
                .chunks
                .iter()
                .map(|chunk| (chunk.context_before, chunk.context_after))
                .collect::<Vec<_>>();
            if text == &spaced {
                assert_eq!(contexts, [(0, SHAPING_CONTEXT), (SHAPING_CONTEXT, 0)]);
            } else {
                assert_eq!(contexts, vec![(0, 0); ruler.chunks.len()]);
            }
        }
    }

    #[gpui::test]
    async fn test_rulers_follow_syntax_changes_outside_the_edited_range(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let language = language::rust_lang();
        language.set_theme(&theme::SyntaxTheme::new_test([("comment", gpui::red())]));
        cx.set_state(&format!("ˇ/*\n{}\n*/\nfn main() {{}}", "x".repeat(4_096)));
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.run_until_parked();
        let ruler_for = |cx: &mut EditorTestContext| {
            cx.update_editor(|editor, window, cx| {
                let snapshot = editor.snapshot(window, cx).display_snapshot;
                let shaper = editor
                    .text_layout_details(window, cx)
                    .ruler_shaper(&snapshot, DisplayRow(1));
                snapshot.ruled_row(DisplayRow(1), shaper).ruler
            })
        };
        let commented = ruler_for(&mut cx);
        assert!(commented.syntax().is_some());
        cx.update_editor(|editor, _, cx| {
            editor.edit([(Point::new(0, 0)..Point::new(0, 2), "")], cx)
        });
        cx.run_until_parked();
        assert!(!cx.buffer(|buffer, _| buffer.is_parsing()));
        let uncommented = ruler_for(&mut cx);
        assert!(!Arc::ptr_eq(&commented, &uncommented));
        assert_eq!(commented.starts, uncommented.starts);
    }

    #[test]
    fn test_grapheme_boundary_search_is_bounded_by_the_limit() {
        let text = format!("a{}b", "\u{301}".repeat(100));
        let cluster_end = text.len() as u32 - 1;
        let at_or_after = |boundaries: &GraphemeBoundaries, offset, limit| {
            boundaries.next_matching(offset, limit, |_| true)
        };
        let boundaries = GraphemeBoundaries::new(&text);
        assert_eq!(at_or_after(&boundaries, 1, 50), None);
        assert_eq!(at_or_after(&boundaries, 1, cluster_end), None);
        assert_eq!(
            at_or_after(&boundaries, 1, text.len() as u32),
            Some(cluster_end)
        );
        assert_eq!(
            at_or_after(&boundaries, cluster_end, text.len() as u32),
            Some(cluster_end)
        );
        assert!(boundaries.is_boundary(cluster_end));
        assert!(!boundaries.is_boundary(3));

        let flags = "🇺🇸".repeat(10);
        let boundaries = GraphemeBoundaries::new(&flags);
        assert_eq!(at_or_after(&boundaries, 1, 7), None);
        assert_eq!(at_or_after(&boundaries, 1, 8), Some(8));
        assert_eq!(at_or_after(&boundaries, 9, flags.len() as u32), Some(16));

        for len in [63, 127, 6207] {
            let text = format!("🇺🇸a{}", "b".repeat(len - "🇺🇸a".len()));
            let len = len as u32;
            let boundaries = GraphemeBoundaries::new(&text);
            assert_eq!(boundaries.next_matching(len - 1, len, |_| false), None);
            assert_eq!(
                boundaries.next_matching(len - 1, len, |offset| offset == len),
                Some(len)
            );
        }
    }

    #[gpui::test]
    async fn test_ruler_bounds_chunks_across_oversized_grapheme_clusters(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let cluster = format!("a{}", "\u{301}".repeat(20_000));
        let text = format!("{}{cluster}{}", "x".repeat(3_000), "y".repeat(3_000));
        let (ruler, ..) = ruler_for_text(&mut cx, &text);
        assert_eq!(ruler.len(), text.len() as u32);
        let boundaries = GraphemeBoundaries::new(&text);
        let cluster_range = 3_000..3_000 + cluster.len() as u32;
        for (ix, boundary) in ruler.starts.iter().enumerate() {
            assert!(text.is_char_boundary(*boundary as usize));
            if cluster_range.start < *boundary && *boundary < cluster_range.end {
                assert_eq!(ruler.chunks[ix - 1].context_after, 0);
                assert_eq!(ruler.chunks[ix].context_before, 0);
            } else {
                assert!(
                    boundaries.is_boundary(*boundary),
                    "{boundary} splits a cluster"
                );
            }
        }
        for chunk in &ruler.chunks {
            assert!(chunk.len + chunk.context_before + chunk.context_after <= MAX_SHAPED_LEN);
        }
    }

    #[gpui::test]
    async fn test_ruler_chunks_stay_bounded_on_replacement_heavy_rows(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let pairs = 8 * CHUNK_LEN;
        let text = "\u{1}a".repeat(pairs as usize);
        let (ruler, snapshot, shaper) = ruler_for_text(&mut cx, &text);
        assert_eq!(ruler.len(), text.len() as u32);
        assert!(ruler.chunks.len() <= (text.len() / CHUNK_LEN as usize) + 1);

        let symbol = crate::display_map::replacement('\u{1}')
            .map(String::from)
            .unwrap_or_default();
        let width_of = |text: &str| shaper.layout(text, &[shaper.run(None, text.len())]).width;
        let pair_width = ScrollPixelOffset::from(width_of(&symbol) + width_of("a"));
        let tolerance = pair_width * pairs as ScrollPixelOffset * 1e-4;
        for pair in [0, 1, CHUNK_LEN / 2 + 1, pairs - 1, pairs] {
            let x = ruler.x_for_column(2 * pair, &snapshot, DisplayRow(0), &shaper);
            let expected = pair_width * pair as ScrollPixelOffset;
            assert!(
                (x - expected).abs() < tolerance,
                "pair {pair}: {x} != {expected}"
            );
            assert_eq!(
                ruler.column_for_x(x, &snapshot, DisplayRow(0), &shaper),
                2 * pair
            );
        }
    }

    #[gpui::test(iterations = 20)]
    async fn test_cached_rulers_match_fresh_rulers_after_random_changes(
        cx: &mut TestAppContext,
        mut rng: StdRng,
    ) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        let alphabet = ["漢", "a", "b", " ", "\t", "\u{1}", "é", "\n"];
        let random_text = |rng: &mut StdRng, len: usize, newlines: bool| {
            let choices = if newlines {
                &alphabet[..]
            } else {
                &alphabet[..alphabet.len() - 1]
            };
            (0..len)
                .map(|_| choices[rng.random_range(0..choices.len())])
                .collect::<String>()
        };
        let rows = (0..4)
            .map(|_| random_text(&mut rng, 700, false))
            .collect::<Vec<_>>();
        cx.set_state(&format!("ˇ{}", rows.join("\n")));

        let mut inlay_ids = Vec::new();
        let mut next_inlay_id = 0;
        let mut retained_count = 0;
        for _ in 0..20 {
            let before = long_row_rulers(&mut cx);
            let operation = rng.random_range(0..10);
            cx.update_editor(|editor, window, cx| {
                let buffer = editor.buffer().read(cx).snapshot(cx);
                let random_offset = |rng: &mut StdRng| {
                    buffer.clip_offset(
                        rng.random_range(MultiBufferOffset(0)..=buffer.len()),
                        Bias::Left,
                    )
                };
                match operation {
                    0..=4 => {
                        let start = random_offset(&mut rng);
                        let end = buffer.clip_offset(
                            (start + rng.random_range(0..4usize)).min(buffer.len()),
                            Bias::Right,
                        );
                        let len = rng.random_range(0..4);
                        let text = random_text(&mut rng, len, true);
                        editor.edit([(start..end, text)], cx);
                    }
                    5..=6 => {
                        let offset = random_offset(&mut rng);
                        let position = if rng.random_bool(0.5) {
                            buffer.anchor_before(offset)
                        } else {
                            buffer.anchor_after(offset)
                        };
                        let id = next_inlay_id;
                        next_inlay_id += 1;
                        inlay_ids.push(id);
                        let len = rng.random_range(1..4);
                        let text = random_text(&mut rng, len, false);
                        let inlay = Inlay::mock_hint(id, position, text.as_str());
                        editor
                            .display_map
                            .update(cx, |map, cx| map.splice_inlays(&[], vec![inlay], cx));
                    }
                    7 => {
                        if !inlay_ids.is_empty() {
                            let id = inlay_ids.swap_remove(rng.random_range(0..inlay_ids.len()));
                            editor.display_map.update(cx, |map, cx| {
                                map.splice_inlays(&[InlayId::Hint(id)], Vec::new(), cx)
                            });
                        }
                    }
                    8 => {
                        let start = random_offset(&mut rng);
                        let end = buffer.clip_offset(
                            (start + rng.random_range(1..20usize)).min(buffer.len()),
                            Bias::Right,
                        );
                        editor.fold_ranges(vec![start..end], false, window, cx);
                    }
                    _ => {
                        editor.unfold_ranges(
                            &[MultiBufferOffset(0)..buffer.len()],
                            true,
                            false,
                            cx,
                        );
                    }
                }
            });

            for (row, cached, fresh) in long_row_rulers(&mut cx) {
                retained_count += before
                    .iter()
                    .filter(|(_, previous, _)| Arc::ptr_eq(previous, &cached))
                    .count();
                assert_eq!(
                    cached.len(),
                    fresh.len(),
                    "row {row:?} after operation {operation}"
                );
                assert!(
                    (cached.width() - fresh.width()).abs() <= fresh.width() * 1e-5,
                    "row {row:?} after operation {operation}: {} != {}",
                    cached.width(),
                    fresh.width()
                );
            }
        }
        assert!(retained_count > 0);
    }

    #[gpui::test]
    async fn test_rulers_rebuild_when_boundary_edits_move_edge_inlays(cx: &mut TestAppContext) {
        init_test(cx);
        let long = "漢".repeat(MAX_LINE_LEN);
        for (id, inlay_at_row_end) in [(0, true), (1, false)] {
            let mut cx = EditorTestContext::new(cx).await;
            cx.set_state(&format!("ˇ{long}\nz"));
            cx.update_editor(|editor, _, cx| {
                let buffer = editor.buffer().read(cx).snapshot(cx);
                let position = if inlay_at_row_end {
                    buffer.anchor_after(MultiBufferOffset(long.len()))
                } else {
                    buffer.anchor_before(MultiBufferOffset(0))
                };
                let inlay = Inlay::mock_hint(id, position, "x");
                editor
                    .display_map
                    .update(cx, |map, cx| map.splice_inlays(&[], vec![inlay], cx));
            });
            let with_inlay = displayed_row_ruler(&mut cx, 0);
            assert_eq!(with_inlay.len(), long.len() as u32 + 1);

            let (at, inserted, long_row) = if inlay_at_row_end {
                (Point::new(0, long.len() as u32), "\n", 0)
            } else {
                (Point::zero(), "y\n", 1)
            };
            cx.update_editor(|editor, _, cx| editor.edit([(at..at, inserted)], cx));
            let without_inlay = displayed_row_ruler(&mut cx, long_row);
            assert!(!Arc::ptr_eq(&with_inlay, &without_inlay));
            assert_eq!(without_inlay.len(), long.len() as u32);
        }
    }

    #[gpui::test]
    async fn test_multiline_inlay_highlights_rebuild_continuation_rows(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx).await;
        cx.set_state("ˇab");
        let inlay_text = format!("p\n{}\nr", "漢".repeat(MAX_LINE_LEN));
        let inlay_position = cx.update_editor(|editor, _, cx| {
            let position = editor
                .buffer()
                .read(cx)
                .snapshot(cx)
                .anchor_after(MultiBufferOffset(1));
            let inlay = Inlay::mock_hint(0, position, inlay_text.as_str());
            editor
                .display_map
                .update(cx, |map, cx| map.splice_inlays(&[], vec![inlay], cx));
            position
        });
        let before = displayed_row_ruler(&mut cx, 1);
        assert_eq!(before.len(), (inlay_text.len() - "p\n\nr".len()) as u32);

        cx.update_editor(|editor, _, cx| {
            editor.highlight_inlays(
                HighlightKey::HoveredLinkState,
                vec![InlayHighlight {
                    inlay: InlayId::Hint(0),
                    inlay_position,
                    range: 2..5,
                }],
                HighlightStyle {
                    font_weight: Some(FontWeight::BOLD),
                    ..HighlightStyle::default()
                },
                cx,
            )
        });
        assert!(!Arc::ptr_eq(&before, &displayed_row_ruler(&mut cx, 1)));
    }

    fn ruler_for_text(
        cx: &mut EditorTestContext,
        text: &str,
    ) -> (RowRuler, DisplaySnapshot, RulerShaper) {
        cx.set_state(&format!("ˇ{text}"));
        let (snapshot, shaper) = cx.update_editor(|editor, window, cx| {
            let snapshot = editor.snapshot(window, cx).display_snapshot;
            let shaper = editor
                .text_layout_details(window, cx)
                .ruler_shaper(&snapshot, DisplayRow(0));
            (snapshot, shaper)
        });
        let ruler = RowRuler::new(&snapshot, DisplayRow(0), &shaper, None, &HashMap::default());
        (ruler, snapshot, shaper)
    }

    fn long_row_rulers(cx: &mut EditorTestContext) -> Vec<(DisplayRow, Arc<RowRuler>, RowRuler)> {
        cx.update_editor(|editor, window, cx| {
            let snapshot = editor.snapshot(window, cx).display_snapshot;
            let details = editor.text_layout_details(window, cx);
            (0..=snapshot.max_point().row().0)
                .map(DisplayRow)
                .filter(|row| snapshot.is_long_unwrapped_row(*row))
                .map(|row| {
                    let shaper = details.ruler_shaper(&snapshot, row);
                    let fresh = RowRuler::new(&snapshot, row, &shaper, None, &HashMap::default());
                    (row, snapshot.ruled_row(row, shaper).ruler, fresh)
                })
                .collect()
        })
    }

    fn displayed_row_ruler(cx: &mut EditorTestContext, row: u32) -> Arc<RowRuler> {
        cx.update_editor(|editor, window, cx| {
            let snapshot = editor.snapshot(window, cx).display_snapshot;
            let shaper = editor
                .text_layout_details(window, cx)
                .ruler_shaper(&snapshot, DisplayRow(row));
            snapshot.ruled_row(DisplayRow(row), shaper).ruler
        })
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings = SettingsStore::test(cx);
            cx.set_global(settings);
            crate::init(cx);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
        });
    }
}
