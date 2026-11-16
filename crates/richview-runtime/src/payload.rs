//! A figure's payload, parsed and ready to lay out at the width the figure occupies.

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Observation, Readout, XScale};
use richview_chart::layout::{
    ChartTheme, Options, Prepared, layout_prepared, layout_prepared_reusing,
};
pub use richview_chart::payload::{Payload, file_name};
use richview_chart::payload::{Quant, StateDerived};
use richview_chart::reader_controls::{Controls, Resolved, State};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
struct TextLayoutKey {
    width: f64,
    ratio: Option<f64>,
    hidden: Vec<String>,
    zoom: Option<[f64; 2]>,
    aside: bool,
}

/// One state's preparations for the series hidden with them, over the rows and over the shown
/// rows (PRF-19).
#[derive(Debug, Clone)]
struct Preparations {
    hidden: Vec<String>,
    rows: Rc<Prepared>,
    shown: Rc<Prepared>,
}

#[derive(Debug, Clone)]
struct EmbeddedReadout {
    id: String,
    row: Option<usize>,
    series: Option<usize>,
    source: Rc<Readout>,
    shown: Rc<Readout>,
    exact: Rc<Readout>,
}

/// Full-state text is independent of width when there are no on-mark value texts. Keep its
/// source identities too, so a different observation or edited payload cannot inherit it.
#[derive(Debug, Clone)]
struct EmbeddedReadouts {
    prepared: Rc<Prepared>,
    panels: Vec<usize>,
    values: Vec<EmbeddedReadout>,
    /// Single-series, unstacked, unannotated views have no zoom-dependent text ordering.
    /// Retained source Rcs keep these pointer keys alive for the cache's lifetime.
    by_source: Option<Vec<((usize, *const Readout), usize)>>,
    derived: Vec<richview_chart::derived::Derived>,
    summary: String,
}

/// A parsed payload.
#[derive(Debug, Clone)]
pub struct Chart {
    pub payload: Payload,
    pub block: ChartBlock,
    /// Rows as objects, the engine's input.
    pub objects: Vec<Value>,
    /// The same rows with every grid value at the author's precision, which the readouts and
    /// on-chart texts are laid out from, so no text is formatted from a grid value (TYP-36).
    shown_objects: Vec<Value>,
    /// The reader's controls and the state they are in (FEA-BR-01).
    pub controls: Controls,
    pub state: State,
    /// The current state's block and rows, with options that hold colours and, for a playback,
    /// the scales over every frame (FEA-BR-03, FEA-BR-31).
    shown: Option<(Resolved, Options)>,
    /// The same state over shown values; its playback fit changes only with the selectors.
    text_shown: Option<(Resolved, Options)>,
    text_cache: RefCell<Option<(TextLayoutKey, Rc<Geometry>)>>,
    embedded_cache: RefCell<Option<EmbeddedReadouts>>,
    /// The current state's preparations: a zoom or resize frame reads its rows and readouts from
    /// there (PRF-19).
    prepared: RefCell<Option<Preparations>>,
    /// The screen's theme, kept while the chart draws for paper in the payload's print theme
    /// (FEA-CH-67).
    screen_theme: Option<ChartTheme>,
    /// A key the chart draws itself stands beside its plot: the expanded view in a short, wide
    /// window (S27-875).
    pub aside: bool,
}

impl Chart {
    pub fn parse(json: &str) -> Result<Chart, String> {
        let payload: Payload = serde_json::from_str(json)
            .map_err(|e| format!("Chart payload is not readable: {e}"))?;
        Chart::from_payload(payload)
    }

    pub fn from_payload(payload: Payload) -> Result<Chart, String> {
        let block = ChartBlock::from_value(&payload.block).ok_or("Chart block is not valid")?;
        let objects = payload.objects();
        let shown_objects = Payload {
            rows: payload.shown_rows(),
            quant: Default::default(),
            ..payload.clone()
        }
        .objects();
        let controls = Controls::parse(&payload.block, &objects);
        let mut chart = Chart {
            payload,
            block,
            objects,
            shown_objects,
            state: controls.initial(),
            controls,
            shown: None,
            text_shown: None,
            text_cache: RefCell::default(),
            embedded_cache: RefCell::default(),
            prepared: RefCell::default(),
            screen_theme: None,
            aside: false,
        };
        let state = chart.state.clone();
        chart.set_state(state);
        Ok(chart)
    }

    /// Draw for paper, or for the screen again. Paper takes the light counterpart's theme the
    /// payload carries when the style has one, as the printed page does (FEA-CH-67, STY-21);
    /// what was laid out in the other theme is dropped. Returns whether the theme changed.
    pub fn set_paper(&mut self, paper: bool) -> bool {
        // The screen's theme is held exactly while the chart is on paper.
        if paper == self.screen_theme.is_some() {
            return false;
        }
        if paper {
            let Some(print) = self.payload.print.clone() else {
                return false;
            };
            self.screen_theme = Some(std::mem::replace(&mut self.payload.theme, print));
        } else if let Some(screen) = self.screen_theme.take() {
            self.payload.theme = screen;
        }
        // Every layout and preparation held was made for the other theme, and a preparation
        // belongs to one (`Prepared`).
        self.shown = None;
        self.text_shown = None;
        self.text_cache.get_mut().take();
        self.embedded_cache.get_mut().take();
        self.prepared.get_mut().take();
        let state = self.state.clone();
        self.set_state(state);
        true
    }

    fn base(&self) -> Options {
        Options {
            lang: self.payload.lang.clone(),
            colors: self.payload.colors.clone(),
            grids: self.payload.quant.clone(),
            ..Options::default()
        }
    }

    /// What core derived for the authored control state. Reader interactions keep these bins,
    /// tallies and order, hiding or clipping marks rather than deriving from grid rows (TYP-36).
    fn derived_state(&self, state: &[usize]) -> Option<&StateDerived> {
        let state = if self.controls.is_empty() {
            &[][..]
        } else {
            state
        };
        self.payload.derived.iter().find(|d| d.state == state)
    }

    fn derived_for(&self, state: &[usize]) -> Vec<richview_chart::derived::Derived> {
        self.derived_state(state)
            .map(|d| d.derived.clone())
            .unwrap_or_default()
    }

    /// The horizontal position grid. Generated density positions use their own lattice even
    /// when the observations are integers; QQ supplies one when it has no source x.
    fn x_grid(&self, block: &ChartBlock) -> Option<Quant> {
        let source = block
            .encoding
            .x
            .as_ref()
            .and_then(|x| self.payload.quant.get(x))
            .cloned();
        let generated = || {
            let key = match block.kind.name() {
                "qq" => "qq.ranking",
                "density" => "density.estimates",
                _ => return None,
            };
            let record = self
                .derived_state(&self.state)?
                .derived
                .iter()
                .find(|d| d.panel == 0 && d.key == key)?;
            serde_json::from_value(record.value.get("x_grid")?.clone()).ok()
        };
        if block.kind.name() == "density" {
            generated().or(source)
        } else {
            source.or_else(generated)
        }
    }

    /// Move the controls to `state`. Colours and fitted scales are recomputed only when a
    /// selector changes, never between playback frames.
    pub fn set_state(&mut self, state: State) {
        if self.controls.is_empty() {
            return;
        }
        let same = self.shown.is_some()
            && self.controls.selectors(&state) == self.controls.selectors(&self.state);
        let options = match (&self.shown, same) {
            (Some((_, o)), true) => o.clone(),
            _ => self.controls.options(
                &self.payload.block,
                &self.objects,
                &state,
                &self.payload.theme,
                &self.base(),
            ),
        };
        let text_options = match (&self.text_shown, same) {
            (Some((_, o)), true) => o.clone(),
            _ => self.controls.options(
                &self.payload.block,
                &self.shown_objects,
                &state,
                &self.payload.theme,
                &self.base(),
            ),
        };
        self.shown = self
            .controls
            .chart(&self.payload.block, &self.objects, &state)
            .map(|r| (r, options));
        self.text_shown = self
            .controls
            .chart(&self.payload.block, &self.shown_objects, &state)
            .map(|r| (r, text_options));
        self.text_cache.get_mut().take();
        self.embedded_cache.get_mut().take();
        self.prepared.get_mut().take();
        self.state = state;
    }

    /// Indices of the payload rows the current state shows (FEA-BR-32).
    pub fn kept(&self) -> Option<&[usize]> {
        self.shown.as_ref().map(|(r, _)| r.kept.as_slice())
    }

    /// Keep a requested zoom inside the data and stop where a position-grid step occupies
    /// about two CSS pixels (TYP-36). Calendar and native integer positions retain their own
    /// resolution and minimum window; they carry no grid precision to expose. A full window resets.
    pub fn clamp_zoom(&self, window: [f64; 2], scale: &XScale) -> Option<[f64; 2]> {
        let [lo, hi] = scale.extent;
        let [a, b] = window;
        if ![lo, hi, a, b].iter().all(|v| v.is_finite()) || a >= b || lo >= hi {
            return None;
        }
        let block = self
            .shown
            .as_ref()
            .map(|(r, _)| &r.block)
            .unwrap_or(&self.block);
        let step = self
            .x_grid(block)
            .filter(|q| q.steps > 0 && q.resolution != Some(1.0))
            .map(|q| (q.max - q.min) / f64::from(q.steps));
        let Some(step) = step else {
            let (a, b) = (a.max(lo), b.min(hi));
            return (b - a >= (hi - lo) * 1e-6 && (a > lo || b < hi)).then_some([a, b]);
        };
        let pixels = (scale.range[1] - scale.range[0]).abs();
        let minimum = (step * pixels / 2.0).max((hi - lo) * 1e-6).min(hi - lo);
        let span = (b - a).max(minimum).min(hi - lo);
        if span == b - a && a >= lo && b <= hi {
            return (span < hi - lo).then_some(window);
        }
        let center = a / 2.0 + b / 2.0;
        let start = (center - span / 2.0).max(lo).min(hi - span);
        (span < hi - lo).then_some([start, start + span])
    }

    /// The chart's geometry at `width` CSS pixels with the reader's hidden series and zoom.
    pub fn geometry(&self, width: f64, hidden: &[String], zoom: Option<[f64; 2]>) -> Geometry {
        self.geometry_at_ratio(width, None, hidden, zoom)
    }

    /// [`Chart::geometry`] with the panels' width-to-height `ratio` in place of the author's or
    /// the chart type's, when given: the expanded chart on a phone held upright fills its turned
    /// dialog (FEA-CH-68).
    pub fn geometry_at_ratio(
        &self,
        width: f64,
        ratio: Option<f64>,
        hidden: &[String],
        zoom: Option<[f64; 2]>,
    ) -> Geometry {
        self.geometry_with_observations(width, ratio, hidden, zoom, Vec::new())
    }

    /// Lay out an ordinary zoom using outgoing panel buffers as storage only.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn geometry_reusing(
        &self,
        width: f64,
        hidden: &[String],
        zoom: Option<[f64; 2]>,
        observations: Vec<Vec<Observation>>,
    ) -> Geometry {
        self.geometry_with_observations(width, None, hidden, zoom, observations)
    }

    fn geometry_with_observations(
        &self,
        width: f64,
        ratio: Option<f64>,
        hidden: &[String],
        zoom: Option<[f64; 2]>,
        observations: Vec<Vec<Observation>>,
    ) -> Geometry {
        let (block, rows, base) = match &self.shown {
            Some((r, o)) => (&r.block, r.rows.as_slice(), o.clone()),
            None => (&self.block, self.objects.as_slice(), self.base()),
        };
        let pristine = hidden.is_empty() && zoom.is_none();
        let reusable_shown = !block.stack
            && block.annotations.is_empty()
            && block.encoding.label.is_none()
            && block.highlight.is_none();
        let mut opts = Options {
            width,
            hidden: hidden.to_vec(),
            zoom,
            aside: self.aside,
            derived: self.derived_for(&self.state),
            ..base
        };
        let prepared = self.preparations(hidden).rows;
        // A hidden-series change replaces the source preparation. Retaining that cache
        // would keep the old full-source allocation alive even though it cannot be reused.
        let stale = self
            .embedded_cache
            .borrow()
            .as_ref()
            .is_some_and(|cached| !Rc::ptr_eq(&prepared, &cached.prepared));
        if stale {
            self.embedded_cache.borrow_mut().take();
        }
        if hidden.is_empty()
            && let Some(cached) = &*self.embedded_cache.borrow()
            && Rc::ptr_eq(&prepared, &cached.prepared)
            && (pristine || reusable_shown && cached.by_source.is_some())
        {
            opts.derived.clone_from(&cached.derived);
            let mut block = block.clone();
            if let Some(ratio) = ratio {
                block.style.ratio = Some(ratio);
            }
            let mut geometry = layout_prepared_reusing(
                &block,
                rows,
                &self.payload.theme,
                &opts,
                &prepared,
                observations,
            );
            let matched = if pristine {
                self.apply_cached_readouts(&mut geometry, cached)
            } else {
                Self::apply_cached_shown(&mut geometry, cached)
            };
            if matched {
                geometry.restore_value_drop_text();
                return geometry;
            }
        }
        if pristine {
            self.embedded_cache.borrow_mut().take();
        }
        // Every number the reader sees is a shown value, never a grid value (TYP-36): the
        // readouts and on-chart texts come from a layout over the rows at the author's
        // precision, with the same exact-state records as the marks.
        let text = self.text_layout(ratio, &opts);
        // Interaction totals are sums of shown values. Reuse the text layout's records too,
        // so the geometry cannot compute those sums from grid values instead.
        if let Some(text) = &text {
            opts.derived.clone_from(&text.derived);
        }
        let mut geometry = match ratio {
            Some(ratio) => {
                let mut block = block.clone();
                block.style.ratio = Some(ratio);
                layout_prepared(&block, rows, &self.payload.theme, &opts, &prepared)
            }
            None => layout_prepared(block, rows, &self.payload.theme, &opts, &prepared),
        };
        // Save grid identities before the shown-value text replaces them. Only the common
        // line/area/scatter case without value labels can skip the second layout on resize.
        let cacheable = pristine
            && matches!(
                block.kind,
                richview_chart::block::ChartType::Line
                    | richview_chart::block::ChartType::Area
                    | richview_chart::block::ChartType::Scatter
            )
            && geometry.mark_texts().is_empty()
            && text.as_ref().is_some_and(|t| t.mark_texts().is_empty());
        let sources: Vec<_> = if cacheable {
            geometry
                .panels
                .iter()
                .flat_map(|p| &p.observations)
                .map(|o| (o.row, o.series, Rc::clone(&o.readout)))
                .collect()
        } else {
            Vec::new()
        };
        if let Some(text) = &text {
            Self::show_text(&mut geometry, text);
        }
        let shown: Vec<_> = if cacheable {
            geometry
                .panels
                .iter()
                .flat_map(|p| &p.observations)
                .map(|o| Rc::clone(&o.readout))
                .collect()
        } else {
            Vec::new()
        };
        if pristine {
            self.show_embedded_text(&mut geometry);
        }
        if cacheable {
            let ids = geometry.mark_ids();
            let state = if self.controls.is_empty() {
                &[][..]
            } else {
                &self.state
            };
            let entries: Vec<_> = self
                .payload
                .text
                .iter()
                .filter(|t| t.state == state)
                .collect();
            if entries.len() == ids.len() && entries.iter().zip(&ids).all(|(e, id)| e.id == *id) {
                let values: Vec<_> = ids
                    .into_iter()
                    .zip(sources)
                    .zip(shown)
                    .zip(geometry.panels.iter().flat_map(|p| &p.observations))
                    .map(
                        |(((id, (row, series, source)), shown), o)| EmbeddedReadout {
                            id,
                            row,
                            series,
                            source,
                            shown,
                            exact: Rc::clone(&o.readout),
                        },
                    )
                    .collect();
                let panels: Vec<_> = geometry
                    .panels
                    .iter()
                    .map(|p| p.observations.len())
                    .collect();
                let by_source = if geometry.series.len() == 1 && reusable_shown {
                    let mut map = Vec::with_capacity(values.len());
                    let mut i = 0;
                    for (panel, &count) in panels.iter().enumerate() {
                        for _ in 0..count {
                            map.push(((panel, Rc::as_ptr(&values[i].source)), i));
                            i += 1;
                        }
                    }
                    // Ambiguous identities fall back to the ordinary shown-value layout.
                    map.sort_unstable_by_key(|(key, _)| *key);
                    map.windows(2).all(|w| w[0].0 != w[1].0).then_some(map)
                } else {
                    None
                };
                *self.embedded_cache.borrow_mut() = Some(EmbeddedReadouts {
                    prepared: Rc::clone(&prepared),
                    panels,
                    values,
                    by_source,
                    derived: opts.derived.clone(),
                    summary: geometry.summary.clone(),
                });
            }
        }
        geometry.restore_value_drop_text();
        geometry
    }

    /// A zoom subset keeps its source's shown text, never the pristine embedded override.
    /// A miss discards this geometry and takes the normal two-layout path.
    fn apply_cached_shown(geometry: &mut Geometry, cached: &EmbeddedReadouts) -> bool {
        let Some(map) = &cached.by_source else {
            return false;
        };
        if geometry.panels.len() != cached.panels.len()
            || geometry.series.len() != 1
            || !geometry.mark_texts().is_empty()
        {
            return false;
        }
        let mut first = 0;
        for (panel, p) in geometry.panels.iter_mut().enumerate() {
            let end = first + cached.panels[panel];
            let mut next = first;
            for o in &mut p.observations {
                // Zooms retain source order. Adjacent observations need no new bisection;
                // sparse subsets still locate and validate each gap by source identity.
                let index = match cached.values.get(next).filter(|_| next < end) {
                    Some(c) if Rc::ptr_eq(&o.readout, &c.source) => next,
                    _ => {
                        let Ok(found) = map
                            .binary_search_by_key(&(panel, Rc::as_ptr(&o.readout)), |(key, _)| {
                                *key
                            })
                        else {
                            return false;
                        };
                        map[found].1
                    }
                };
                let c = &cached.values[index];
                if o.row != c.row || o.series != c.series {
                    return false;
                }
                o.readout = Rc::clone(&c.shown);
                next = index + 1;
            }
            first = end;
        }
        geometry.summary.clone_from(&cached.summary);
        true
    }

    fn apply_cached_readouts(&self, geometry: &mut Geometry, cached: &EmbeddedReadouts) -> bool {
        if !geometry
            .panels
            .iter()
            .map(|p| p.observations.len())
            .eq(cached.panels.iter().copied())
            || !geometry.mark_texts().is_empty()
        {
            return false;
        }
        let state = if self.controls.is_empty() {
            &[][..]
        } else {
            &self.state
        };
        let mut entries = self.payload.text.iter().filter(|t| t.state == state);
        for (o, c) in geometry
            .panels
            .iter()
            .flat_map(|p| &p.observations)
            .zip(&cached.values)
        {
            let Some(e) = entries.next() else {
                return false;
            };
            if o.row != c.row
                || o.series != c.series
                || e.id != c.id
                || e.heading != c.exact.heading
                || !(Rc::ptr_eq(&o.readout, &c.source) || o.readout == c.source)
                || c.shown
                    .lines
                    .iter()
                    .zip(&c.exact.lines)
                    .any(|(source, exact)| {
                        let value = e
                            .lines
                            .iter()
                            .find(|l| l.series == source.series && l.name == source.name)
                            .map_or(&source.value, |l| &l.value);
                        *value != exact.value
                    })
            {
                return false;
            }
        }
        if entries.next().is_some() {
            return false;
        }
        for (o, c) in geometry
            .panels
            .iter_mut()
            .flat_map(|p| &mut p.observations)
            .zip(&cached.values)
        {
            o.readout = Rc::clone(&c.exact);
        }
        geometry.summary.clone_from(&cached.summary);
        true
    }

    /// The layout over the rows at the author's precision, or `None` when the state shows none.
    fn text_layout(&self, ratio: Option<f64>, opts: &Options) -> Option<Rc<Geometry>> {
        let key = TextLayoutKey {
            width: opts.width,
            ratio,
            hidden: opts.hidden.clone(),
            zoom: opts.zoom,
            aside: opts.aside,
        };
        if let Some((cached_key, geometry)) = &*self.text_cache.borrow()
            && *cached_key == key
        {
            return Some(Rc::clone(geometry));
        }
        let (mut block, rows, base) = match &self.text_shown {
            Some((r, o)) => (r.block.clone(), r.rows.as_slice(), o.clone()),
            None if self.shown.is_some() => return None,
            None => (
                self.block.clone(),
                self.shown_objects.as_slice(),
                self.base(),
            ),
        };
        if let Some(ratio) = ratio {
            block.style.ratio = Some(ratio);
        }
        // The scales over every playback frame are fitted to these rows too, so a key or an
        // axis never reads an extent the grid moved outward.
        let opts = Options {
            width: opts.width,
            hidden: opts.hidden.clone(),
            zoom: opts.zoom,
            aside: opts.aside,
            derived: opts.derived.clone(),
            ..base
        };
        let prepared = self.preparations(&opts.hidden).shown;
        let geometry = Rc::new(layout_prepared(
            &block,
            rows,
            &self.payload.theme,
            &opts,
            &prepared,
        ));
        *self.text_cache.borrow_mut() = Some((key, Rc::clone(&geometry)));
        Some(geometry)
    }

    /// The current state's preparations with `hidden` hidden; a new hidden set replaces them.
    fn preparations(&self, hidden: &[String]) -> Preparations {
        let mut cache = self.prepared.borrow_mut();
        match &*cache {
            Some(held) if held.hidden == hidden => held.clone(),
            _ => cache
                .insert(Preparations {
                    hidden: hidden.to_vec(),
                    rows: Rc::default(),
                    shown: Rc::default(),
                })
                .clone(),
        }
    }

    /// The current title, chosen controls and summary, for the chart's accessible name.
    pub fn accessible_name(&self, geometry: &Geometry) -> String {
        let block = self
            .shown
            .as_ref()
            .map(|(r, _)| &r.block)
            .unwrap_or(&self.block);
        let mut parts = vec![block.title.trim().trim_end_matches('.').to_string()];
        let state = self.controls.name(&self.state);
        if !state.is_empty() {
            parts.push(state);
        }
        if !geometry.summary.is_empty() {
            parts.push(geometry.summary.trim().trim_end_matches('.').to_string());
        }
        parts.retain(|p| !p.is_empty());
        parts.join(". ")
    }

    /// Readouts, summary and printed texts of the shown-rows layout, matched to the marks by id.
    fn show_text(geometry: &mut Geometry, text: &Geometry) {
        if Self::same_marks(geometry, text) {
            // Every observation has the id of the one at its place, so no id is needed.
            for (o, shown) in geometry
                .panels
                .iter_mut()
                .flat_map(|p| &mut p.observations)
                .zip(text.panels.iter().flat_map(|p| &p.observations))
            {
                o.readout.clone_from(&shown.readout);
            }
        } else {
            let readouts: std::collections::HashMap<String, &Rc<Readout>> = text
                .mark_ids()
                .into_iter()
                .zip(text.panels.iter().flat_map(|p| &p.observations))
                .map(|(id, o)| (id, &o.readout))
                .collect();
            let ids = geometry.mark_ids();
            for (id, o) in ids
                .iter()
                .zip(geometry.panels.iter_mut().flat_map(|p| &mut p.observations))
            {
                if let Some(readout) = readouts.get(id) {
                    o.readout = Rc::clone(readout);
                }
            }
        }
        geometry.summary.clone_from(&text.summary);
        geometry.set_mark_texts(&text.mark_texts());
    }

    /// Whether each observation of `geometry` has the mark id of the observation at its place in
    /// `text`, and no two of `text` share one: the same panels, with the same rows and series in
    /// the same order, and no row twice.
    fn same_marks(geometry: &Geometry, text: &Geometry) -> bool {
        let place = |o: &richview_chart::geometry::Observation| (o.row, o.series);
        let same = geometry.panels.len() == text.panels.len()
            && geometry.panels.iter().zip(&text.panels).all(|(a, b)| {
                a.observations.len() == b.observations.len()
                    && a.observations
                        .iter()
                        .map(place)
                        .eq(b.observations.iter().map(place))
            });
        if !same {
            return false;
        }
        let mut rows: Vec<usize> = text
            .panels
            .iter()
            .flat_map(|p| &p.observations)
            .filter_map(|o| o.row)
            .collect();
        rows.sort_unstable();
        rows.windows(2).all(|w| w[0] != w[1])
    }

    /// The readouts the exact-row layout embedded for the current control state replace the
    /// ones from the shown rows, matched by mark id, so a legend toggle or a zoom that drops
    /// observations leaves the rest on their own text. Only values and headings are replaced,
    /// so a hidden series stays out of the readout.
    fn show_embedded_text(&self, geometry: &mut Geometry) {
        let state: &[usize] = if self.controls.is_empty() {
            &[]
        } else {
            &self.state
        };
        let entries: std::collections::HashMap<&str, &richview_chart::payload::ObservationText> =
            self.payload
                .text
                .iter()
                .filter(|t| t.state == state && !t.id.is_empty())
                .map(|t| (t.id.as_str(), t))
                .collect();
        if entries.is_empty() {
            return;
        }
        let ids = geometry.mark_ids();
        for (id, observation) in ids
            .iter()
            .zip(geometry.panels.iter_mut().flat_map(|p| &mut p.observations))
        {
            let Some(entry) = entries.get(id.as_str()) else {
                continue;
            };
            let readout = Rc::make_mut(&mut observation.readout);
            readout.heading = entry.heading.clone();
            for line in &mut readout.lines {
                if let Some(text) = entry
                    .lines
                    .iter()
                    .find(|l| l.series == line.series && l.name == line.name)
                {
                    line.value = text.value.clone();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rows_become_objects_in_source_column_order() {
        let payload = Payload {
            id: "c".into(),
            block: json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}}),
            columns: vec!["c".into(), "v".into()],
            rows: vec![vec![json!("A"), json!(3)], vec![json!("B"), json!(5)]],
            ..Payload::default()
        };
        let chart = Chart::from_payload(payload).unwrap();
        assert_eq!(chart.objects[1], json!({"c": "B", "v": 5}));
        assert!(chart.geometry(640.0, &[], None).failure.is_none());
    }

    #[test]
    fn readouts_and_labels_are_laid_out_from_shown_values_not_grid_values() {
        use richview_chart::payload::{ObservationText, Quant, TextLine};
        // Dense bars and four coincident lines force value drops; the grid says 7.01/2.99.
        for kind in ["bar", "line"] {
            let series = if kind == "line" { 4 } else { 1 };
            let rows: Vec<Vec<Value>> = (0..20)
                .flat_map(|i| (0..series).map(move |s| vec![json!(i), json!(if i % 2 == 0 { 299 } else { 701 }), json!(format!("Series {s} with a deliberately long name that belongs in the legend"))]))
                .collect();
            let shown = (0..20)
                .flat_map(|i| (0..series).map(move |_| json!(if i % 2 == 0 { 3 } else { 7 })))
                .collect();
            let payload = Payload {
                id: "c".into(),
                block: json!({"type":kind,"title":"T","style":{"orientation":"vertical"},"encoding":{"x":"c","y":"v","color":"s"}}),
                columns: vec!["c".into(), "v".into(), "s".into()],
                rows,
                quant: [(
                    "v".to_string(),
                    Quant {
                        min: 0.0,
                        max: 10.0,
                        steps: 1000,
                        ..Default::default()
                    },
                )]
                .into(),
                shown: [("v".to_string(), shown)].into(),
                ..Payload::default()
            };
            let mut chart = Chart::from_payload(payload).unwrap();
            let first = chart.geometry(390.0, &[], None);
            let ids = first.mark_ids();
            chart.payload.text = first.panels[0]
                .observations
                .iter()
                .enumerate()
                .map(|(obs, o)| ObservationText {
                    state: vec![],
                    panel: 0,
                    obs,
                    id: ids[obs].clone(),
                    heading: o.readout.heading.clone(),
                    lines: o
                        .readout
                        .lines
                        .iter()
                        .map(|l| TextLine {
                            name: l.name.clone(),
                            value: l.value.clone(),
                            derived: l.derived,
                            series: l.series,
                            names_series: l.names_series,
                        })
                        .collect(),
                })
                .collect();
            for width in [390.0, 400.0] {
                // The second pristine layout exercises the embedded-readout cache when labels cannot fit.
                for repeat in 0..2 {
                    let geometry = chart.geometry(width, &[], None);
                    let readouts: Vec<_> = geometry
                        .panels
                        .iter()
                        .flat_map(|p| &p.observations)
                        .flat_map(|o| o.readout.lines.iter().map(|l| l.value.as_str()))
                        .collect();
                    assert!(
                        readouts.contains(&"7"),
                        "{kind}/{width}/{repeat}: {readouts:?}"
                    );
                    assert!(
                        readouts
                            .iter()
                            .all(|v| !v.contains("7.01") && !v.contains("2.99")),
                        "{readouts:?}"
                    );
                    let diagnostics = geometry
                        .checks
                        .iter()
                        .filter(|check| check.kind == "label-dropped")
                        .map(|check| format!("{} {}", check.message, check.details))
                        .collect::<Vec<_>>()
                        .join(" | ");
                    assert!(
                        !diagnostics.contains("7.01") && !diagnostics.contains("2.99"),
                        "{kind}/{width}/{repeat}: {diagnostics}"
                    );
                    let labels: Vec<_> =
                        geometry.mark_texts().into_iter().map(|t| t.text).collect();
                    assert!(
                        labels
                            .iter()
                            .all(|v| !v.contains("7.01") && !v.contains("2.99")),
                        "{labels:?}"
                    );
                    let drops: Vec<_> = geometry
                        .dropped
                        .iter()
                        .filter(|d| d.kind.as_deref() == Some("value"))
                        .collect();
                    assert!(
                        !drops.is_empty(),
                        "{kind}/{width}: the fixture must exercise drop provenance"
                    );
                    assert!(
                        drops.iter().all(|d| d.text == "3" || d.text == "7"),
                        "{kind}/{width}/{repeat}: {drops:?}"
                    );
                    if kind == "line" {
                        assert!(
                            geometry.mark_texts().is_empty(),
                            "all four-value kinds exceed the narrow budget and names use the legend"
                        );
                        assert!(
                            chart.embedded_cache.borrow().is_some(),
                            "the cache path must be exercised"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_filter_state_does_not_draw_the_initial_states_derived_structure() {
        // Core embedded the funnel's records for the first option; the second option has other
        // rows, and drawing it with those records once panicked at hydration.
        let rows: Vec<Vec<Value>> = [
            ("North", "a", 100.0),
            ("North", "b", 60.0),
            ("North", "c", 30.0),
            ("South", "a", 40.0),
            ("South", "b", 35.0),
        ]
        .iter()
        .map(|(r, s, v)| vec![json!(r), json!(s), json!(v)])
        .collect();
        let payload = Payload {
            id: "c".into(),
            block: json!({"type": "funnel", "title": "T", "encoding": {"x": "s", "y": "v"},
                "controls": [{"kind": "filter", "id": "r", "field": "r", "options": ["North", "South"]}]}),
            columns: vec!["r".into(), "s".into(), "v".into()],
            rows,
            derived: vec![richview_chart::payload::StateDerived {
                state: vec![0],
                derived: vec![richview_chart::derived::Derived {
                    panel: 0,
                    key: "funnel.texts".into(),
                    value: json!({"labels": ["100", "60", "30"],
                        "of_first": ["100%", "60%", "30%"],
                        "of_previous": ["", "60%", "50%"]}),
                }],
            }],
            ..Payload::default()
        };
        let mut chart = Chart::from_payload(payload).unwrap();
        assert!(
            chart
                .geometry(640.0, &[], None)
                .derived
                .iter()
                .any(|d| d.key == "funnel.texts")
        );
        chart.set_state(vec![1]);
        let g = chart.geometry(640.0, &[], None);
        assert!(g.failure.is_none(), "{:?}", g.failure);
        let labels: Vec<_> = g.mark_texts().into_iter().map(|t| t.text).collect();
        assert!(
            labels.contains(&"40".into()) && labels.contains(&"35".into()),
            "{labels:?}"
        );
        assert!(
            !labels.contains(&"100".into()) && !labels.contains(&"60".into()),
            "{labels:?}"
        );
    }

    #[test]
    fn a_legend_toggle_still_reads_shown_values() {
        use richview_chart::payload::Quant;
        // Grid steps put 7 at 7.01; hiding the other series draws a different layout, whose
        // readouts are still laid out from the shown 7, never from the grid.
        let quant = Quant {
            min: 0.0,
            max: 10.0,
            steps: 1000,
            ..Default::default()
        };
        let payload = Payload {
            id: "c".into(),
            block: json!({"type": "bar", "title": "T",
                "encoding": {"x": "c", "y": "v", "color": "s"}}),
            columns: vec!["c".into(), "v".into(), "s".into()],
            rows: vec![
                vec![json!("A"), json!(299), json!("one")],
                vec![json!("A"), json!(701), json!("two")],
            ],
            quant: [("v".to_string(), quant)].into(),
            shown: [("v".to_string(), vec![json!(3), json!(7)])].into(),
            ..Payload::default()
        };
        let chart = Chart::from_payload(payload).unwrap();
        let values = |g: &Geometry| -> Vec<String> {
            g.panels
                .iter()
                .flat_map(|p| &p.observations)
                .flat_map(|o| o.readout.lines.iter().map(|l| l.value.clone()))
                .collect()
        };
        for hidden in [vec![], vec!["one".to_string()]] {
            let v = values(&chart.geometry(640.0, &hidden, None));
            assert!(
                v.iter().all(|t| !t.contains("7.01") && !t.contains("2.99")),
                "{hidden:?} {v:?}"
            );
        }
        let v = values(&chart.geometry(640.0, &["one".into()], None));
        assert!(v.contains(&"7".to_string()), "{v:?}");
    }

    #[test]
    fn a_state_draws_its_rows_with_scales_over_every_frame() {
        let rows: Vec<Vec<Value>> = [
            ("1960", "A", 1.0),
            ("1960", "B", 2.0),
            ("2000", "A", 10.0),
            ("2000", "B", 20.0),
        ]
        .iter()
        .map(|(y, s, v)| vec![json!(y), json!(s), json!(v), json!(v)])
        .collect();
        let payload = Payload {
            id: "c".into(),
            block: json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "v", "color": "s"},
                "controls": [{"kind": "playback", "id": "y", "field": "year"},
                             {"kind": "filter", "id": "s", "field": "s", "options": ["A", "B"]}]}),
            columns: vec!["year".into(), "s".into(), "x".into(), "v".into()],
            rows,
            ..Payload::default()
        };
        let mut chart = Chart::from_payload(payload).unwrap();
        assert_eq!(chart.state, [1, 0], "the last frame, the first option");
        assert_eq!(chart.kept(), Some(&[2][..]));
        let ticks = |g: &Geometry| -> Vec<String> {
            g.items()
                .filter_map(|i| match i {
                    richview_chart::geometry::Item::Text(t)
                        if t.role == richview_chart::geometry::Role::AxisLabel =>
                    {
                        Some(t.text.clone())
                    }
                    _ => None,
                })
                .collect()
        };
        let last = chart.geometry(640.0, &[], None);
        chart.set_state(vec![0, 0]);
        assert_eq!(chart.kept(), Some(&[0][..]));
        assert_eq!(
            ticks(&chart.geometry(640.0, &[], None)),
            ticks(&last),
            "frames share their axes"
        );
        chart.set_state(vec![0, 1]);
        assert_eq!(chart.kept(), Some(&[1][..]));
    }

    #[test]
    fn text_cache_follows_layout_inputs_and_control_changes() {
        let mut chart = Chart::from_payload(Payload {
            block: json!({"type": "bar", "title": "T", "encoding": {"x": "q", "y": "v"},
                "controls": [{"kind": "filter", "id": "r", "field": "r", "options": ["North", "South"]}]}),
            columns: vec!["r".into(), "q".into(), "v".into()],
            rows: vec![vec![json!("North"), json!("A"), json!(3)], vec![json!("South"), json!("A"), json!(40)]],
            ..Default::default()
        }).unwrap();
        let opts = Options {
            width: 640.0,
            ..chart.base()
        };
        let first = chart.text_layout(None, &opts).unwrap();
        assert!(Rc::ptr_eq(&first, &chart.text_layout(None, &opts).unwrap()));
        for changed in [
            Options {
                width: 360.0,
                ..opts.clone()
            },
            Options {
                hidden: vec!["A".into()],
                ..opts.clone()
            },
            Options {
                zoom: Some([0.0, 1.0]),
                ..opts.clone()
            },
            Options {
                aside: true,
                ..opts.clone()
            },
        ] {
            assert!(!Rc::ptr_eq(
                &first,
                &chart.text_layout(None, &changed).unwrap()
            ));
            assert!(Rc::ptr_eq(
                &chart.text_layout(None, &changed).unwrap(),
                &chart.text_layout(None, &changed).unwrap()
            ));
        }
        assert!(!Rc::ptr_eq(
            &first,
            &chart.text_layout(Some(1.0), &opts).unwrap()
        ));
        chart.set_state(vec![1]);
        let south = chart.text_layout(None, &opts).unwrap();
        assert!(!Rc::ptr_eq(&first, &south));
        assert_eq!(south.panels[0].observations[0].readout.lines[0].value, "40");
    }
}

#[cfg(test)]
mod embedded_text_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_runtime_shows_the_embedded_readouts_not_grid_text() {
        let payload = Payload {
            id: "c".into(),
            block: json!({"type": "line", "title": "T", "encoding": {"x": "d", "y": "v"}}),
            columns: vec!["d".into(), "v".into()],
            rows: vec![
                vec![json!("2024-03-01"), json!(3.141538461538462)],
                vec![json!("2024-03-02"), json!(2.718241758241758)],
                vec![json!("2024-03-03"), json!(1.414065934065934)],
            ],
            ..Payload::default()
        };
        let mut chart = Chart::from_payload(payload.clone()).unwrap();
        let g = chart.geometry(640.0, &[], None);
        let own: Vec<String> = g
            .panels
            .iter()
            .flat_map(|p| p.observations.iter())
            .flat_map(|o| o.readout.lines.iter().map(|l| l.value.clone()))
            .collect();
        assert_eq!(own.len(), 3, "{own:?}");
        let ids = g.mark_ids();
        let text: Vec<_> = g
            .panels
            .iter()
            .enumerate()
            .flat_map(|(panel, p)| {
                let ids = &ids;
                p.observations.iter().enumerate().map(move |(obs, o)| {
                    richview_chart::payload::ObservationText {
                        state: vec![],
                        panel,
                        obs,
                        id: ids[obs].clone(),
                        heading: format!("h{obs}"),
                        lines: o
                            .readout
                            .lines
                            .iter()
                            .map(|l| richview_chart::payload::TextLine {
                                name: l.name.clone(),
                                value: format!("exact{obs}"),
                                derived: false,
                                series: l.series,
                                names_series: l.names_series,
                            })
                            .collect(),
                    }
                })
            })
            .collect();
        chart.payload.text = text;
        let g = chart.geometry(640.0, &[], None);
        let shown: Vec<String> = g
            .panels
            .iter()
            .flat_map(|p| p.observations.iter())
            .flat_map(|o| o.readout.lines.iter().map(|l| l.value.clone()))
            .collect();
        assert_eq!(shown, ["exact0", "exact1", "exact2"]);
        for width in [390.0, 900.0, 640.0] {
            let geometry = chart.geometry(width, &[], None);
            let observations = &geometry.panels[0].observations;
            assert_eq!(observations[1].readout.heading, "h1");
            assert_eq!(observations[1].readout.lines[0].value, "exact1");
        }
        let [lo, hi] = g.panels[0].x.as_ref().unwrap().extent;
        for window in [[lo, hi - (hi - lo) * 0.2], [lo + (hi - lo) * 0.2, hi]] {
            for width in [390.0, 900.0] {
                let cold = Chart::from_payload(chart.payload.clone()).unwrap();
                assert_eq!(
                    chart.geometry(width, &[], Some(window)),
                    cold.geometry(width, &[], Some(window)),
                    "a zoom reuses shown text, not the full-state embedded overrides"
                );
            }
        }
        // Public payload text can be edited in place, with the same allocation and length.
        // A width-only cache must not leave the old heading or value on that observation.
        chart.payload.text[1].heading.replace_range(.., "H1");
        chart.payload.text[1].lines[0]
            .value
            .replace_range(.., "EXACT1");
        let resized = chart.geometry(390.0, &[], None);
        assert_eq!(resized.panels[0].observations[1].readout.heading, "H1");
        assert_eq!(
            resized.panels[0].observations[1].readout.lines[0].value,
            "EXACT1"
        );
        assert_eq!(
            resized.panels[0].observations[2].readout.lines[0].value,
            "exact2"
        );
        let previous = Rc::downgrade(&chart.prepared.borrow().as_ref().unwrap().rows);
        let hidden = vec!["v".into()];
        let cold = Chart::from_payload(chart.payload.clone()).unwrap();
        assert_eq!(
            chart.geometry(640.0, &hidden, None),
            cold.geometry(640.0, &hidden, None)
        );
        assert!(
            previous.upgrade().is_none(),
            "a stale readout cache must release its source preparation"
        );
    }
}
