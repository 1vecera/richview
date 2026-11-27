//! The live example's scene of the home page (W3 scene 2): "Your agent writes one file."
//! In story builds the scene pins while the reader scrolls, and the scroll writes the anchor report
//! block by block. In the source, the lines not yet written are dim and the block being written is
//! lit; at wide layouts a thin wire joins it to the block it makes. In the report, each block rises
//! into place as its source block completes, and the chart draws along x as its lines are written
//! (INT-61). Scrolling back unwrites. The report shown is the build's own page of that report
//! (`landing/anchor`) in a same-origin frame, so the scene can reach its blocks; the first
//! edit, style or reset hands the example to the sandboxed live frame, the only frame visitor
//! input ever reaches (FEA-SF-29). With reduced motion, and on the static landing that `prod`
//! serves until Daniel accepts the scroll story (STY-44), the scene neither pins nor moves.
use super::homepage::report_state;
use super::{document, now, on, query, style, window};
use crate::reveal::{Block, END_HOLD, Kind, blocks, lines_written, written};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Element, HtmlIFrameElement, HtmlTextAreaElement};

const SVG: &str = "http://www.w3.org/2000/svg";
/// The page the scene writes: the anchor report as the build renders it.
const PAGE: &str = "/landing/anchor";
/// The earliest the scene's frame is made after the site runtime starts. The first screen is the
/// painting agent's clip, which needs no runtime, so nothing else waits for it.
const AFTER_START_MS: f64 = 600.0;
/// How long the page must rest from scrolling before the scene's frame is made. Making it starts its
/// report, up to 180 ms of main-thread work in Chromium, so it waits for a pause instead
/// of landing mid-scroll; until then the live frame shows the report whole. The site
/// runtime starts on the reader's first gesture, often a scroll.
const QUIET_MS: f64 = 250.0;
/// How long a frame whose runtime has started may take to draw before it shows anyway.
const DRAW_MS: f64 = 2500.0;
/// How far a report block rises into place, in the report's pixels.
const RISE_PX: f64 = 8.0;
/// How much of the report's head shows before the scroll writes it, as the source shows its lines
/// not yet written: an empty report page reads as a frame that failed to load (lead).
const HEAD_GHOST: f64 = 0.16;

/// One block of the report as the scene shows it: the elements that rise and fade together, and a
/// chart's figure, whose canvas draws along x.
struct Shown {
    parts: Vec<Element>,
    chart: Option<Element>,
}

/// Lengths the stylesheet sets, read once and again after a resize: the editor's line height and
/// top padding, and the sticky top where the stage pins.
#[derive(Clone, Copy, Default)]
struct Lines {
    height: f64,
    top: f64,
    pin: f64,
}

struct Scene {
    section: Element,
    track: Element,
    pin: Element,
    stage: Element,
    area: HtmlTextAreaElement,
    lit: Element,
    veil: Element,
    turn: Element,
    wire: Option<Element>,
    viewport: Element,
    frame: Option<HtmlIFrameElement>,
    loaded_at: f64,
    started_at: Option<f64>,
    drawn: u8,
    ready: bool,
    /// The report's blocks, once its frame has drawn; empty when they do not match the source's,
    /// in which case the report shows whole (`mismatch`).
    shown: Vec<Shown>,
    mismatch: bool,
    blocks: Vec<Block>,
    lines: Option<Lines>,
    handed: bool,
    scheduled: bool,
}

/// What makes the scene's frame, and its pending timer.
struct Maker {
    make: Closure<dyn FnMut()>,
    timer: Option<i32>,
}

thread_local! {
    static SCENE: RefCell<Option<Scene>> = const { RefCell::new(None) };
    static MAKER: RefCell<Option<Maker>> = const { RefCell::new(None) };
}

/// Pins the scene and drives it from the scroll, unless motion is reduced or the landing is static.
pub(crate) fn start() {
    let Some(section) = query(".demo") else {
        return;
    };
    let static_landing = document()
        .document_element()
        .and_then(|root| root.get_attribute("data-landing"))
        .is_some_and(|value| value == "static");
    let reduced = window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    if static_landing || reduced {
        return;
    }
    let find = |selector: &str| section.query_selector(selector).ok().flatten();
    let (Some(track), Some(pin), Some(stage), Some(viewport)) = (
        find(".demo__track"),
        find(".demo__pin"),
        find(".demo__stage"),
        find(".report-viewport"),
    ) else {
        return;
    };
    let Some(area) = query("#demo-source").and_then(|e| e.dyn_into::<HtmlTextAreaElement>().ok())
    else {
        return;
    };
    let Some(code) = area.parent_element() else {
        return;
    };
    let blocks = blocks(&area.value());
    if blocks.is_empty() {
        return;
    }
    let make = |tag: &str, class: &str| -> Option<Element> {
        let element = document().create_element(tag).ok()?;
        element.set_class_name(class);
        let _ = element.set_attribute("aria-hidden", "true");
        Some(element)
    };
    let (Some(lit), Some(veil), Some(turn)) = (
        make("div", "code__lit"),
        make("div", "code__veil"),
        make("p", "demo__turn"),
    ) else {
        return;
    };
    // The band goes under the text and the veil over it, and the chip sits in the editor's corner;
    // none of them takes the pointer.
    let _ = code.insert_before(&lit, code.first_child().as_ref());
    let _ = code.append_child(&veil);
    turn.set_text_content(Some("Your turn. Edit the source."));
    let _ = code.append_child(&turn);
    let wire = wire(&stage);
    let _ = section.set_attribute("data-mode", "pinned");
    SCENE.with(|scene| {
        *scene.borrow_mut() = Some(Scene {
            section,
            track,
            pin,
            stage,
            area,
            lit,
            veil,
            turn,
            wire,
            viewport,
            frame: None,
            loaded_at: now(),
            started_at: None,
            drawn: 0,
            ready: false,
            shown: vec![],
            mismatch: false,
            blocks,
            lines: None,
            handed: false,
            scheduled: false,
        });
    });
    let earliest = now() + AFTER_START_MS;
    MAKER.with(|maker| {
        *maker.borrow_mut() = Some(Maker {
            make: Closure::new(make_frame),
            timer: None,
        });
    });
    let target: web_sys::EventTarget = window().into();
    on(&target, "scroll", move |_| {
        make_when_quiet(earliest);
        schedule();
    });
    on(&target, "resize", |_| {
        SCENE.with(|scene| {
            if let Some(scene) = scene.borrow_mut().as_mut() {
                scene.lines = None;
            }
        });
        schedule();
    });
    make_when_quiet(earliest);
    schedule();
}

/// Makes the scene's frame once the page has rested from scrolling for `QUIET_MS`, and no earlier
/// than `earliest`: each scroll before then moves the moment on. Does nothing once the frame is made
/// or the example handed over.
fn make_when_quiet(earliest: f64) {
    let waiting = SCENE.with(|scene| {
        scene
            .borrow()
            .as_ref()
            .is_some_and(|scene| scene.frame.is_none() && !scene.handed)
    });
    if !waiting {
        return;
    }
    MAKER.with(|maker| {
        let mut maker = maker.borrow_mut();
        let Some(maker) = maker.as_mut() else {
            return;
        };
        if let Some(handle) = maker.timer.take() {
            window().clear_timeout_with_handle(handle);
        }
        let delay = (earliest - now()).max(QUIET_MS);
        maker.timer = window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                maker.make.as_ref().unchecked_ref(),
                delay.round() as i32,
            )
            .ok();
    });
}

/// The wire's path, in an SVG over the stage.
fn wire(stage: &Element) -> Option<Element> {
    let svg = document().create_element_ns(Some(SVG), "svg").ok()?;
    let _ = svg.set_attribute("class", "demo__wire");
    let _ = svg.set_attribute("aria-hidden", "true");
    let path = document().create_element_ns(Some(SVG), "path").ok()?;
    let _ = svg.append_child(&path);
    stage.append_child(&svg).ok()?;
    Some(path)
}

fn make_frame() {
    SCENE.with(|scene| {
        let mut scene = scene.borrow_mut();
        let Some(scene) = scene.as_mut() else {
            return;
        };
        if scene.handed || scene.frame.is_some() {
            return;
        }
        let Ok(frame) = document().create_element("iframe") else {
            return;
        };
        frame.set_class_name("demo__reveal");
        let _ = frame.set_attribute(
            "title",
            "The report the source makes, written as you scroll",
        );
        let _ = frame.set_attribute("tabindex", "-1");
        let _ = frame.set_attribute("aria-hidden", "true");
        let _ = frame.set_attribute("src", PAGE);
        if scene.viewport.append_child(&frame).is_ok() {
            scene.frame = Some(frame.unchecked_into());
            scene.loaded_at = now();
            cover(&scene.viewport, true);
        }
    });
    schedule();
}

/// Ends the scene for good when the reader edits the example, picks a style or resets it: the
/// written report gives way to the sandboxed live frame, which takes every edit from here.
pub(crate) fn hand_over() {
    let changed = SCENE.with(|scene| {
        let mut scene = scene.borrow_mut();
        let Some(scene) = scene.as_mut() else {
            return false;
        };
        if scene.handed {
            return false;
        }
        scene.handed = true;
        if let Some(frame) = scene.frame.take() {
            frame.remove();
        }
        cover(&scene.viewport, false);
        scene.shown.clear();
        let _ = scene.section.set_attribute("data-handed", "");
        true
    });
    if changed {
        schedule();
    }
}

/// While the scene's report covers the live frame, the live frame takes no pointer and is no Tab
/// stop, so nothing the reader cannot see takes their input. It stays readable to assistive
/// technology, which the scene's own frame is hidden from: both hold the same report. The stylesheet
/// drops the pointer under `data-covered`, and the live example's buffer swap keeps the Tab stop
/// off while it is set; the hand-over gives both back.
fn cover(viewport: &Element, covered: bool) {
    if covered {
        let _ = viewport.set_attribute("data-covered", "");
    } else {
        let _ = viewport.remove_attribute("data-covered");
    }
    if let Some(live) = query("#demo-result") {
        if covered {
            let _ = live.set_attribute("tabindex", "-1");
        } else {
            let _ = live.remove_attribute("tabindex");
        }
    }
}

/// Asks for one frame, however many scroll events arrive before it.
fn schedule() {
    let first = SCENE.with(|scene| {
        scene.borrow_mut().as_mut().is_some_and(|scene| {
            let first = !scene.scheduled;
            scene.scheduled = true;
            first
        })
    });
    if first {
        let next = Closure::once_into_js(frame);
        let _ = window().request_animation_frame(next.unchecked_ref());
    }
}

fn frame() {
    let again = SCENE.with(|scene| {
        let mut scene = scene.borrow_mut();
        let Some(scene) = scene.as_mut() else {
            return false;
        };
        scene.scheduled = false;
        scene.update()
    });
    if again {
        schedule();
    }
}

fn span(t: f64, from: f64, to: f64) -> f64 {
    ((t - from) / (to - from)).clamp(0.0, 1.0)
}

/// Starts briskly and settles, as a line set in type.
fn ease(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

impl Scene {
    /// Draws the scene for the current scroll and returns whether it still waits for its report.
    fn update(&mut self) -> bool {
        let track = self.track.get_bounding_client_rect();
        let pin = self.pin.get_bounding_client_rect();
        let pin_top = self.line_box().pin;
        let travel = (track.height() - pin.height()).max(1.0);
        let progress = if self.handed {
            1.0
        } else {
            ((pin_top - track.top()) / travel).clamp(0.0, 1.0)
        };
        let done = written(progress, &self.blocks);
        // The latest block begun is the one being written, or the last one written.
        let current = done.iter().rposition(|t| *t > 0.0);
        let finished = done.iter().all(|t| *t >= 1.0);
        let waiting = self.watch();
        // Until its report has drawn, or if the report's blocks do not match the source's, the
        // example shows whole, as on the static landing.
        if (!self.ready || self.mismatch) && !self.handed {
            self.show_all_lines(false);
            self.draw_wire(None, None, true);
            return waiting;
        }
        self.draw_source(&done, current, finished, progress);
        let shown = if self.handed {
            None
        } else {
            self.draw_report(&done, current)
        };
        self.draw_wire(current, shown, finished);
        let turn = finished && !self.handed && progress > 1.0 - END_HOLD / 2.0;
        let _ = if turn {
            self.section.set_attribute("data-turn", "")
        } else {
            self.section.remove_attribute("data-turn")
        };
        let _ = self
            .turn
            .set_attribute("aria-hidden", if turn { "false" } else { "true" });
        let _ = self
            .section
            .set_attribute("data-progress", &format!("{progress:.3}"));
        waiting
    }

    /// Shows the frame once its report has drawn, or after `DRAW_MS` when it starts without
    /// drawing, and reads its blocks then. Returns whether it is still waiting.
    fn watch(&mut self) -> bool {
        if self.ready || self.handed {
            return false;
        }
        let Some(frame) = &self.frame else {
            return false;
        };
        let (running, drawn) = report_state(frame);
        if running && self.started_at.is_none() {
            self.started_at = Some(now());
        }
        self.drawn = if drawn {
            self.drawn.saturating_add(1)
        } else {
            0
        };
        let since = self.started_at.unwrap_or(self.loaded_at);
        if self.drawn > 2 || now() - since > DRAW_MS * if running { 1.0 } else { 3.2 } {
            self.ready = true;
            self.shown = frame
                .content_document()
                .map(|report| report_blocks(&report))
                .unwrap_or_default();
            self.mismatch = self.shown.len() != self.blocks.len();
            if self.mismatch {
                self.shown.clear();
            }
            let _ = frame.set_attribute("data-ready", "");
            return false;
        }
        true
    }

    fn line_box(&mut self) -> Lines {
        if let Some(lines) = self.lines {
            return lines;
        }
        let lines = Lines {
            height: css_px(&self.area, "line-height").max(1.0),
            top: css_px(&self.area, "padding-top"),
            pin: css_px(&self.pin, "top"),
        };
        self.lines = Some(lines);
        lines
    }

    /// Before the scene's report is ready, and once it is handed over, the source shows whole.
    fn show_all_lines(&self, lit: bool) {
        style(&self.lit, "--lit-a", if lit { "1" } else { "0" });
        style(&self.veil, "--veil-y", "100000px");
    }

    /// Dims the lines not yet written, lights the block being written and keeps it in view.
    fn draw_source(&mut self, done: &[f64], current: Option<usize>, finished: bool, progress: f64) {
        if self.handed {
            self.show_all_lines(false);
            return;
        }
        let lines = self.line_box();
        let total = self.blocks.last().map_or(0, |block| block.end);
        let written_to = match current {
            Some(i) => self.blocks[i].start + lines_written(&self.blocks[i], done[i]),
            None => 0,
        };
        let written_to = if finished { total } else { written_to };
        // Keep the line being written a little above the middle of the editor.
        let view = f64::from(self.area.client_height());
        let max = (f64::from(self.area.scroll_height()) - view).max(0.0);
        let wanted = (lines.top + written_to as f64 * lines.height - view * 0.55).clamp(0.0, max);
        let scroll = f64::from(self.area.scroll_top());
        let scroll = if (wanted - scroll).abs() >= 1.0 {
            self.area.set_scroll_top(wanted.round() as i32);
            f64::from(self.area.scroll_top())
        } else {
            scroll
        };
        let y = |line: usize| lines.top + line as f64 * lines.height - scroll;
        style(
            &self.veil,
            "--veil-y",
            &if finished {
                "100000px".to_owned()
            } else {
                format!("{:.1}px", y(written_to))
            },
        );
        match current.map(|i| self.blocks[i]) {
            Some(block) if !finished || progress < 1.0 - END_HOLD / 2.0 => {
                style(
                    &self.lit,
                    "--lit-y",
                    &format!("{:.1}px", y(block.start) - 2.0),
                );
                style(
                    &self.lit,
                    "--lit-h",
                    &format!("{:.1}px", block.lines() as f64 * lines.height + 4.0),
                );
                style(&self.lit, "--lit-a", "1");
            }
            _ => style(&self.lit, "--lit-a", "0"),
        }
    }

    /// Shows each report block as far as its source block is written, and scrolls the report so
    /// the newest block is in view. Returns how far the current block shows.
    fn draw_report(&self, done: &[f64], current: Option<usize>) -> Option<f64> {
        let frame = self.frame.as_ref()?;
        let mut current_shown = None;
        for (index, (block, part)) in self.blocks.iter().zip(&self.shown).enumerate() {
            let t = done[index];
            let shown = match part.chart {
                // A chart's frame and title come first; its marks draw along x as its lines come.
                Some(_) => ease(span(t, 0.0, 0.25)),
                None => ease(span(t, 0.55, 1.0)),
            };
            if Some(index) == current {
                current_shown = Some(shown);
            }
            // The first block is the report's head (`report_blocks`).
            let floor = if index == 0 { HEAD_GHOST } else { 0.0 };
            let opacity = format!("{:.3}", floor + (1.0 - floor) * shown);
            let rise = format!("translateY({:.2}px)", RISE_PX * (1.0 - shown));
            for element in &part.parts {
                style(element, "opacity", &opacity);
                style(element, "transform", &rise);
            }
            if let Some(figure) = &part.chart
                && let Ok(Some(canvas)) = figure.query_selector("canvas")
            {
                let drawn = if block.kind == Kind::Fence {
                    span(t, 0.15, 1.0)
                } else {
                    1.0
                };
                style(
                    &canvas,
                    "clip-path",
                    &format!("inset(0 {:.2}% 0 0)", 100.0 * (1.0 - drawn)),
                );
            }
        }
        // Keep the newest block's foot in the report's view.
        if let (Some(i), Some(view)) = (current, frame.content_window())
            && let Some(last) = self.shown[i].parts.last()
        {
            let height = view
                .inner_height()
                .ok()
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);
            let scrolled = view.scroll_y().unwrap_or(0.0);
            let foot = last.get_bounding_client_rect().bottom() + scrolled;
            let wanted = (foot + 32.0 - height).max(0.0);
            if (wanted - scrolled).abs() >= 1.0 {
                view.scroll_to_with_x_and_y(0.0, wanted.round());
            }
        }
        current_shown
    }

    /// Joins the lit source block to the report block it makes, at wide layouts.
    fn draw_wire(&self, current: Option<usize>, shown: Option<f64>, finished: bool) {
        let Some(path) = &self.wire else {
            return;
        };
        let hide = || {
            let _ = path.set_attribute("d", "");
        };
        let (Some(i), Some(shown), Some(frame)) = (current, shown, &self.frame) else {
            return hide();
        };
        if finished || shown <= 0.0 || self.handed {
            return hide();
        }
        let stage = self.stage.get_bounding_client_rect();
        if stage.width() <= 0.0 {
            return hide();
        }
        let band = self.lit.get_bounding_client_rect();
        let box_ = frame.get_bounding_client_rect();
        let unscaled = f64::from(frame.offset_width()).max(1.0);
        let scale = box_.width() / unscaled;
        let Some(first) = self.shown[i].parts.first() else {
            return hide();
        };
        let block = first.get_bounding_client_rect();
        let x1 = band.right() - stage.left() - 6.0;
        let y1 = band.top() + band.height() / 2.0 - stage.top();
        let x2 = box_.left() + block.left() * scale - stage.left() - 6.0;
        let y2 = box_.top() + (block.top() + block.height().min(48.0) / 2.0) * scale - stage.top();
        // Only where both ends are on screen in their panes.
        let viewport = self.viewport.get_bounding_client_rect();
        let y2_page = y2 + stage.top();
        if y2_page < viewport.top() || y2_page > viewport.bottom() || x2 <= x1 {
            return hide();
        }
        let bend = (x2 - x1) * 0.5;
        let _ = path.set_attribute(
            "d",
            &format!(
                "M{x1:.1} {y1:.1} C{:.1} {y1:.1} {:.1} {y2:.1} {x2:.1} {y2:.1}",
                x1 + bend,
                x2 - bend
            ),
        );
        let _ = path.set_attribute("opacity", &format!("{shown:.3}"));
    }
}

/// The report's blocks in order: its header (title, lede, byline and the rule under them), then each
/// block at the top of `main`, a kept chart and its note as two. The site check in `reveal.rs` pins
/// this shape.
fn report_blocks(report: &web_sys::Document) -> Vec<Shown> {
    let Ok(Some(header)) = report.query_selector(".rv-report__header") else {
        return vec![];
    };
    let mut out = vec![Shown {
        parts: vec![header],
        chart: None,
    }];
    let Ok(Some(main)) = report.query_selector("main") else {
        return vec![];
    };
    // The selector matches elements only. They belong to the frame's window, which an instanceof
    // cast against this window's Element would reject, so they are taken as they are.
    let children = |parent: &Element| -> Vec<Element> {
        parent
            .query_selector_all(":scope > *")
            .map(|list| {
                (0..list.length())
                    .filter_map(|i| list.item(i))
                    .map(|node| node.unchecked_into::<Element>())
                    .collect()
            })
            .unwrap_or_default()
    };
    for child in children(&main) {
        let kept = child
            .class_name()
            .split_ascii_whitespace()
            .any(|name| name == "rv-keep");
        let parts = if kept { children(&child) } else { vec![child] };
        for part in parts {
            let chart = part
                .matches("figure[data-rv-chart]")
                .unwrap_or(false)
                .then(|| part.clone());
            out.push(Shown {
                parts: vec![part],
                chart,
            });
        }
    }
    out
}

/// A length the stylesheet resolved for `element`, in CSS pixels.
fn css_px(element: &Element, property: &str) -> f64 {
    window()
        .get_computed_style(element)
        .ok()
        .flatten()
        .and_then(|declaration| declaration.get_property_value(property).ok())
        .and_then(|value| value.trim_end_matches("px").parse().ok())
        .unwrap_or(0.0)
}
