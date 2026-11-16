//! The voices scene of the home page (W3 scene 3, S27-926): the anchor report in each builtin
//! style. The build stages one page per style; here the scroll drives them. While the scene's stage
//! is pinned, each style's page rises from below over the last, which settles back and dims, and
//! once a rising page passes the middle the page takes that style (`data-voice` on `<html>`, whose
//! blocks the build writes from core's tokens). With reduced motion, and on the static landing that
//! `prod` serves until Daniel accepts the scroll story (STY-44), nothing moves: the five pages
//! stand side by side, each shown once its report has drawn, and the page keeps its own style.
use super::homepage::report_state;
use super::{document, now, on, query, style, window};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Element, HtmlIFrameElement};

/// The share of each style's stretch of scroll in which it holds still before the next rises.
const HOLD: f64 = 0.4;
/// How long a page whose report has started may take to draw before it shows anyway.
const DRAW_MS: f64 = 2500.0;

/// Where the scene stands at `progress`, 0 where the stage pins and 1 where it lets go, with `count`
/// styles: how far each page has risen into place (0 below the stage, 1 in place) and which style is
/// on screen, the last page past halfway. The first page is in place from the start, and every
/// style holds for `HOLD` of its stretch, the last one included.
pub(crate) fn crossing(progress: f64, count: usize) -> (Vec<f64>, usize) {
    let position = progress.clamp(0.0, 1.0) * (count.saturating_sub(1) as f64 + HOLD);
    let rises: Vec<f64> = (0..count)
        .map(|page| {
            if page == 0 {
                return 1.0;
            }
            let t = (position - (page - 1) as f64 - HOLD) / (1.0 - HOLD);
            ease(t.clamp(0.0, 1.0))
        })
        .collect();
    let on_screen = rises.iter().rposition(|rise| *rise >= 0.5).unwrap_or(0);
    (rises, on_screen)
}

/// Starts slowly, as a sheet lifted off a pile, and settles into place.
fn ease(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// The pages a reader at `position` (in styles, negative before the stage pins, `count - 1` plus the
/// last hold where it lets go) needs live. Creating a page's frame stalls the page while its report
/// starts, so a frame is only created as the reader enters a hold, when nothing on screen moves
/// (S27-930): through style `k`'s hold the page beneath it, it and the next are live, and while the
/// next rises only it and the next. Going forward the next page loads as the hold begins, a hold
/// before it rises; going back the page beneath loads as the hold begins, a hold before the page on
/// it descends. Before the stage pins only the first page shows, so only it loads.
pub(crate) fn wanted(position: f64, count: usize) -> Vec<bool> {
    if position < 0.0 {
        return (0..count).map(|page| page == 0).collect();
    }
    let position = position.min(count.saturating_sub(1) as f64 + HOLD);
    let base = position.floor();
    let holding = position - base < HOLD;
    let base = base as usize;
    (0..count)
        .map(|page| page == base || page == base + 1 || (holding && page + 1 == base))
        .collect()
}

/// How far, in styles, a page stays live past the positions that want it, so a reader who lingers
/// where a hold meets a rise does not create its frame again and again.
const LINGER: f64 = 0.1;

/// The pages a reader at `position` keeps live: those wanted within `LINGER` of it. Never more than
/// three, the most `wanted` asks for.
pub(crate) fn kept(position: f64, count: usize) -> Vec<bool> {
    let (before, here, after) = (
        wanted(position - LINGER, count),
        wanted(position, count),
        wanted(position + LINGER, count),
    );
    (0..count)
        .map(|page| before[page] || here[page] || after[page])
        .collect()
}

struct Page {
    sheet: Element,
    holder: Element,
    src: String,
    title: String,
    frame: Option<HtmlIFrameElement>,
    loaded_at: f64,
    started_at: Option<f64>,
    drawn: u8,
    ready: bool,
}

struct Scene {
    section: Element,
    track: Element,
    stage: Element,
    rail: Vec<Element>,
    pages: Vec<Page>,
    voices: Vec<String>,
    voice: Option<usize>,
    scheduled: bool,
}

thread_local! {static SCENE:RefCell<Option<Scene>>=const{RefCell::new(None)};}

/// Pins the scene and drives it from the scroll, unless motion is reduced or the landing is static.
pub(crate) fn start() {
    let Some(section) = query(".voices") else {
        return;
    };
    let root = document().document_element();
    let static_landing = root
        .as_ref()
        .and_then(|e| e.get_attribute("data-landing"))
        .is_some_and(|value| value == "static");
    let reduced = window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    if static_landing || reduced {
        load_strip_near(&section);
        return;
    }
    let (Some(track), Some(stage)) = (
        section.query_selector(".voices__track").ok().flatten(),
        section.query_selector(".voices__stage").ok().flatten(),
    ) else {
        return;
    };
    let rail = all(&section, ".voices__rail li");
    let mut pages = vec![];
    for sheet in all(&section, ".voices__sheet") {
        let Some(holder) = sheet.query_selector(".voices__page").ok().flatten() else {
            return;
        };
        // The markup holds no frames (S27-927): the scene creates each page's as the reader comes
        // to it (`wanted`).
        let src = holder.get_attribute("data-src").unwrap_or_default();
        let title = holder.get_attribute("data-title").unwrap_or_default();
        pages.push(Page {
            sheet,
            holder,
            src,
            title,
            frame: None,
            loaded_at: now(),
            started_at: None,
            drawn: 0,
            ready: false,
        });
    }
    if pages.len() < 2 {
        return;
    }
    let voices = pages
        .iter()
        .map(|page| page.sheet.get_attribute("data-voice").unwrap_or_default())
        .collect();
    let _ = section.set_attribute("data-mode", "pinned");
    link_faces_near(&section);
    SCENE.with(|scene| {
        *scene.borrow_mut() = Some(Scene {
            section,
            track,
            stage,
            rail,
            pages,
            voices,
            voice: None,
            scheduled: false,
        });
    });
    let target: web_sys::EventTarget = window().into();
    on(&target, "scroll", |_| schedule());
    on(&target, "resize", |_| schedule());
    schedule();
}

/// The staged report-face catalog, which holds the styles' faces as embedded fonts.
const FACES: &str = "/richview-fonts.css";

/// The voices give the page each style's faces, and only the report-face catalog holds them all;
/// the page's own faces are site fonts. So the page does not link the catalog, its first paint
/// does not wait for it, and a first visit does not download it (S27-927, PRF-36). The scene links
/// it once the reader comes within a screen of it, which leaves the faces a screen of scrolling to
/// load before the stage pins and the page takes the first style.
fn link_faces_near(section: &Element) {
    let mark = section.clone();
    let near = move || {
        let screen = window()
            .inner_height()
            .ok()
            .and_then(|height| height.as_f64())
            .unwrap_or(0.0);
        mark.get_bounding_client_rect().top() < 2.0 * screen
    };
    if near() {
        link_faces();
        return;
    }
    let mut linked = false;
    let target: web_sys::EventTarget = window().into();
    on(&target, "scroll", move |_| {
        if !linked && near() {
            linked = true;
            link_faces();
        }
    });
}

/// Links the report-face catalog into the page, once.
fn link_faces() {
    let page = document();
    let Some(head) = page.head() else {
        return;
    };
    let selector = format!("link[href='{FACES}']");
    if head.query_selector(&selector).ok().flatten().is_some() {
        return;
    }
    let Ok(link) = page.create_element("link") else {
        return;
    };
    let _ = link.set_attribute("rel", "stylesheet");
    let _ = link.set_attribute("href", FACES);
    let _ = head.append_child(&link);
}

/// Side by side, each page shows once its report has drawn, as in the pinned scene. The markup holds
/// no frames (S27-927): each page gets its frame as it comes within two screens, which leaves its
/// report two screens of scrolling to load and draw, since the strip does not pin.
fn load_strip_near(section: &Element) {
    let mut waiting = all(section, ".voices__sheet");
    let mut load_near = move || {
        let screen = window()
            .inner_height()
            .ok()
            .and_then(|height| height.as_f64())
            .unwrap_or(0.0);
        waiting.retain(|sheet| {
            let place = sheet.get_bounding_client_rect();
            let near = place.top() < 2.0 * screen && place.bottom() > -screen;
            if near {
                load_sheet(sheet);
            }
            !near
        });
    };
    load_near();
    let target: web_sys::EventTarget = window().into();
    on(&target, "scroll", move |_| load_near());
}

/// Gives a strip page a frame on its named report and watches it from insertion.
fn load_sheet(sheet: &Element) {
    let (Some(holder), Ok(frame)) = (
        sheet.query_selector(".voices__page").ok().flatten(),
        document().create_element("iframe"),
    ) else {
        return;
    };
    let title = holder.get_attribute("data-title").unwrap_or_default();
    let _ = frame.set_attribute("title", &title);
    let _ = frame.set_attribute("tabindex", "-1");
    let src = holder.get_attribute("data-src").unwrap_or_default();
    let _ = frame.set_attribute("src", &src);
    if holder.append_child(&frame).is_ok() {
        // A resource may keep the iframe's load event pending after its chart has drawn.
        watch_ready(sheet.clone(), frame.unchecked_into(), now(), None, 0);
    }
}

/// Checks a page's report every frame until it has drawn, or until `DRAW_MS` after its runtime
/// started (no WebGL), or until it is clear the runtime will not start; then shows it.
fn watch_ready(
    sheet: Element,
    frame: HtmlIFrameElement,
    inserted: f64,
    started: Option<f64>,
    drawn: u8,
) {
    let next = Closure::once_into_js(move || {
        let (running, charts) = report_state(&frame);
        let started = started.or_else(|| running.then(now));
        let drawn = if charts { drawn.saturating_add(1) } else { 0 };
        let since = started.unwrap_or(inserted);
        if drawn > 2 || now() - since > DRAW_MS * if running { 1.0 } else { 3.2 } {
            let _ = sheet.set_attribute("data-ready", "");
            return;
        }
        watch_ready(sheet, frame, inserted, started, drawn);
    });
    let _ = window().request_animation_frame(next.unchecked_ref());
}

fn all(parent: &Element, selector: &str) -> Vec<Element> {
    let Ok(list) = parent.query_selector_all(selector) else {
        return vec![];
    };
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|node| node.dyn_into::<Element>().ok())
        .collect()
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

impl Scene {
    /// Places every page for the current scroll, keeps the live pages loaded, and returns whether a
    /// page is still waiting for its report to draw.
    fn update(&mut self) -> bool {
        let track = self.track.get_bounding_client_rect();
        let pin = self.stage.get_bounding_client_rect();
        let viewport = window()
            .inner_height()
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(800.0);
        // The stage pins at its sticky top: that is where the track's top is when the scene starts.
        let pin_top = css_px(&self.stage, "top");
        let travel = (track.height() - pin.height()).max(1.0);
        let progress = (pin_top - track.top()) / travel;
        let count = self.pages.len();
        let (rises, on_screen) = crossing(progress, count);
        // In styles, negative before the stage pins.
        let position = progress * (count as f64 - 1.0 + HOLD);
        for (page, rise) in self.pages.iter().zip(&rises) {
            style(&page.sheet, "--rise", &format!("{rise:.4}"));
        }
        for (index, page) in self.pages.iter().enumerate() {
            let under = rises.get(index + 1).copied().unwrap_or(0.0);
            style(&page.sheet, "--under", &format!("{under:.4}"));
        }
        // Above the scene the page keeps its own style; from the pin on it wears the style on
        // screen, and past the scene the last one stays.
        let reached = track.top() <= pin_top + 1.0;
        let voice = reached.then_some(on_screen);
        if voice != self.voice {
            self.voice = voice;
            if let Some(root) = document().document_element() {
                let _ = match voice.and_then(|index| self.voices.get(index)) {
                    Some(name) => root.set_attribute("data-voice", name),
                    None => root.remove_attribute("data-voice"),
                };
            }
            for (index, item) in self.rail.iter().enumerate() {
                let _ = if voice == Some(index) {
                    item.set_attribute("aria-current", "true")
                } else {
                    item.remove_attribute("aria-current")
                };
            }
        }
        // The first page loads as the scene comes within a screen and a half, the next as the stage
        // pins, and after that each as the reader enters a hold (`wanted`).
        let near = track.top() - pin_top < viewport * 1.5 && track.bottom() > -viewport;
        let (wanted, kept) = if near {
            (wanted(position, count), kept(position, count))
        } else {
            (vec![false; count], vec![false; count])
        };
        let mut waiting = false;
        for (index, page) in self.pages.iter_mut().enumerate() {
            if wanted[index] && page.frame.is_none() {
                page.load();
            } else if !kept[index] && page.frame.is_some() {
                page.unload();
            }
            waiting |= page.watch();
        }
        let _ = self
            .section
            .set_attribute("data-progress", &format!("{progress:.3}"));
        waiting
    }
}

impl Page {
    /// Gives the page a new frame on its report: a new element rather than a new address for the
    /// old one, so the reader's back button never steps through the scene's pages.
    fn load(&mut self) {
        let Ok(frame) = document().create_element("iframe") else {
            return;
        };
        let _ = frame.set_attribute("title", &self.title);
        let _ = frame.set_attribute("tabindex", "-1");
        let _ = frame.set_attribute("src", &self.src);
        if self.holder.append_child(&frame).is_ok() {
            self.frame = Some(frame.unchecked_into());
            self.loaded_at = now();
            self.started_at = None;
            self.drawn = 0;
            self.ready = false;
            let _ = self.sheet.remove_attribute("data-ready");
        }
    }

    fn unload(&mut self) {
        if let Some(frame) = self.frame.take() {
            frame.remove();
        }
        self.ready = false;
        let _ = self.sheet.remove_attribute("data-ready");
    }

    /// Shows the page's report once it has drawn, so the notice a chart shows before its runtime
    /// starts never appears, or after `DRAW_MS` when it starts without drawing (no WebGL) or
    /// never starts. Returns whether it is still waiting.
    fn watch(&mut self) -> bool {
        let Some(frame) = &self.frame else {
            return false;
        };
        if self.ready {
            return false;
        }
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
            let _ = self.sheet.set_attribute("data-ready", "");
            return false;
        }
        true
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_style_holds_then_each_rises_in_turn_and_the_last_holds_too() {
        let (rises, on_screen) = crossing(0.0, 5);
        assert_eq!(rises, vec![1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(on_screen, 0);
        // The first style's hold: nothing moves.
        let (rises, _) = crossing(HOLD / 4.4 * 0.99, 5);
        assert_eq!(rises[1], 0.0);
        // Halfway through the first crossing the second page is half risen and on screen.
        let (rises, on_screen) = crossing((HOLD + (1.0 - HOLD) / 2.0) / 4.4, 5);
        assert!((rises[1] - 0.5).abs() < 1e-9, "{rises:?}");
        assert_eq!(on_screen, 1);
        // The last style is in place before the scene ends, and holds to its end.
        for progress in [4.0 / 4.4, 1.0] {
            let (rises, on_screen) = crossing(progress, 5);
            assert!(
                rises.iter().all(|rise| (rise - 1.0).abs() < 1e-9),
                "{rises:?}"
            );
            assert_eq!(on_screen, 4);
        }
    }

    #[test]
    fn outside_the_scene_the_crossing_stays_at_its_ends() {
        assert_eq!(crossing(-3.0, 5), crossing(0.0, 5));
        assert_eq!(crossing(7.0, 5), crossing(1.0, 5));
    }

    #[test]
    fn a_page_rises_only_forward_and_settles_without_overshoot() {
        let mut last = vec![0.0; 5];
        for step in 0..=440 {
            let (rises, _) = crossing(f64::from(step) / 440.0, 5);
            for (rise, before) in rises.iter().zip(&last) {
                assert!((0.0..=1.0).contains(rise));
                assert!(rise >= before, "step {step}: {rises:?}");
            }
            last = rises;
        }
    }

    #[test]
    fn a_hold_keeps_the_page_beneath_the_page_and_the_next_and_a_rise_the_two_it_moves() {
        assert_eq!(wanted(-0.5, 5), vec![true, false, false, false, false]);
        assert_eq!(wanted(0.0, 5), vec![true, true, false, false, false]);
        assert_eq!(wanted(2.2, 5), vec![false, true, true, true, false]);
        assert_eq!(wanted(2.7, 5), vec![false, false, true, true, false]);
        assert_eq!(wanted(4.2, 5), vec![false, false, false, true, true]);
        assert_eq!(wanted(4.4, 5), vec![false, false, false, false, true]);
        assert_eq!(wanted(9.0, 5), vec![false, false, false, false, true]);
    }

    /// Walks the scene from `from` to `to` in small steps, as `Scene::update` loads and empties the
    /// pages, and returns where each frame was created, with the most frames ever live.
    fn walk(live: &mut [bool], from: f64, to: f64) -> (Vec<(usize, f64)>, usize) {
        let (mut created, mut most) = (vec![], 0);
        let steps = ((to - from).abs() / 0.01).round() as i32;
        for step in 0..=steps {
            let position = from + (to - from) * f64::from(step) / f64::from(steps.max(1));
            let (wanted, kept) = (wanted(position, live.len()), kept(position, live.len()));
            for page in 0..live.len() {
                if wanted[page] && !live[page] {
                    live[page] = true;
                    created.push((page, position));
                } else if !kept[page] {
                    live[page] = false;
                }
            }
            most = most.max(live.iter().filter(|page| **page).count());
        }
        (created, most)
    }

    #[test]
    fn a_frame_is_created_only_as_the_reader_enters_a_hold_and_at_most_three_are_live() {
        let mut live = vec![false; 5];
        let (forward, most_forward) = walk(&mut live, -1.0, 5.0);
        let (back, most_back) = walk(&mut live, 5.0, -1.0);
        assert!(
            most_forward <= 3 && most_back <= 3,
            "{most_forward} {most_back}"
        );
        assert_eq!(forward.len(), 5, "{forward:?}");
        // Back from the end, the last page is live and every other page is created again.
        assert_eq!(back.len(), 4, "{back:?}");
        for (page, position) in forward.iter().chain(&back) {
            let holding = *position >= 0.0 && position - position.floor() < HOLD + 1e-9;
            let before_the_pin = *position < 0.0 && *page == 0;
            assert!(holding || before_the_pin, "page {page} at {position}");
        }
    }

    #[test]
    fn a_reader_lingering_where_a_hold_meets_a_rise_creates_no_frame_twice() {
        let mut live = vec![false; 5];
        walk(&mut live, -1.0, 2.0 + HOLD);
        for _ in 0..10 {
            let (forward, _) = walk(&mut live, 2.0 + HOLD - 0.05, 2.0 + HOLD + 0.05);
            let (back, _) = walk(&mut live, 2.0 + HOLD + 0.05, 2.0 + HOLD - 0.05);
            assert!(
                forward.is_empty() && back.is_empty(),
                "{forward:?} {back:?}"
            );
        }
    }
}
