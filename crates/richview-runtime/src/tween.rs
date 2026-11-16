//! Glides between two states of a chart (FEA-BR-02, FEA-CH-52). Marks keep their identity and
//! move between real positions: a point or bar is the same mark in both states when it has the
//! same series and the same place among that series' marks of its kind. A mark without a partner
//! fades out or in. Lines and areas with the same shape morph point by point; a line that gains
//! observations, as playback reveals a series, grows to its new end. A label with the same words
//! in both states, such as a country's name in every frame of a playback, travels with its mark
//! at full strength; the others belong to one state only, so a label leaving fades as the marks
//! set off and one arriving appears as they land, and no label ever shows a value between the two.

use richview_chart::geometry::{Cmd, Geometry, Item, Path, Rect, Rgba, Role};
use std::collections::HashMap;

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn mix2(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [mix(a[0], b[0], t), mix(a[1], b[1], t)]
}

fn faded(c: Rgba, k: f64) -> Rgba {
    let [r, g, b, a] = c.0;
    Rgba([r, g, b, a * k as f32])
}

/// `item` with every colour's alpha scaled by `k`.
pub fn fade(item: &Item, k: f64) -> Item {
    let mut item = item.clone();
    match &mut item {
        Item::Rect { fill, stroke, .. } => {
            fill.color = faded(fill.color, k);
            if let Some(s) = stroke {
                s.color = faded(s.color, k);
            }
        }
        Item::Line { stroke, .. } => stroke.color = faded(stroke.color, k),
        Item::Area { fill, .. } => fill.color = faded(fill.color, k),
        Item::Point { fill, stroke, .. } => {
            if let Some(f) = fill {
                f.color = faded(f.color, k);
            }
            if let Some(s) = stroke {
                s.color = faded(s.color, k);
            }
        }
        Item::Text(t) => t.color = faded(t.color, k),
    }
    item
}

/// Which mark this is across states: its kind, series and place among that series' marks.
fn keys(items: &[Item]) -> Vec<Option<(u8, Option<usize>, usize)>> {
    let mut seen: HashMap<(u8, Option<usize>), usize> = HashMap::new();
    items
        .iter()
        .map(|item| {
            if item.role() != Role::Mark {
                return None;
            }
            let kind = match item {
                Item::Rect { .. } => 0,
                Item::Point { .. } => 1,
                Item::Line { .. } => 2,
                Item::Area { .. } => 3,
                Item::Text(_) => return None,
            };
            let n = seen.entry((kind, item.series())).or_default();
            *n += 1;
            Some((kind, item.series(), *n - 1))
        })
        .collect()
}

/// The same path with every point moved toward `b`'s, when both have the same commands.
fn morph(a: &Path, b: &Path, t: f64) -> Option<Path> {
    if a.cmds.len() != b.cmds.len() {
        return None;
    }
    let cmds = a
        .cmds
        .iter()
        .zip(&b.cmds)
        .map(|(x, y)| match (x, y) {
            (Cmd::M { p: p0 }, Cmd::M { p }) => Some(Cmd::M {
                p: mix2(*p0, *p, t),
            }),
            (Cmd::L { p: p0 }, Cmd::L { p }) => Some(Cmd::L {
                p: mix2(*p0, *p, t),
            }),
            (
                Cmd::C {
                    c1: a1,
                    c2: a2,
                    p: p0,
                },
                Cmd::C { c1, c2, p },
            ) => Some(Cmd::C {
                c1: mix2(*a1, *c1, t),
                c2: mix2(*a2, *c2, t),
                p: mix2(*p0, *p, t),
            }),
            (Cmd::Z, Cmd::Z) => Some(Cmd::Z),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Path { cmds })
}

fn right_end(items: &[Item]) -> Option<f64> {
    items
        .iter()
        .filter(|i| i.role() == Role::Mark && matches!(i, Item::Line { .. } | Item::Area { .. }))
        .filter_map(|i| i.bounds())
        .map(|r| r.right())
        .reduce(f64::max)
}

/// Whether an item is text that names or values a mark, as opposed to the chart's chrome.
fn is_label(item: &Item) -> bool {
    matches!(item, Item::Text(_)) && matches!(item.role(), Role::Label | Role::Annotation)
}

/// For each label of `new`, the label of `old` with the same words and role, paired in order
/// of appearance: the same name in two states.
fn same_labels(old: &[Item], new: &[Item]) -> Vec<Option<usize>> {
    let mut by_words: HashMap<(&str, bool), Vec<usize>> = HashMap::new();
    for (i, item) in old.iter().enumerate().rev() {
        if let Item::Text(t) = item
            && is_label(item)
        {
            by_words
                .entry((t.text.as_str(), t.role == Role::Label))
                .or_default()
                .push(i);
        }
    }
    new.iter()
        .map(|item| match item {
            Item::Text(t) if is_label(item) => by_words
                .get_mut(&(t.text.as_str(), t.role == Role::Label))
                .and_then(Vec::pop),
            _ => None,
        })
        .collect()
}

/// `b` as it appears a fraction `t` of the way from `a`: matched marks and the labels they share
/// in between, unmatched ones fading. At `t = 1` this is `b`.
pub fn between(a: &Geometry, b: &Geometry, t: f64) -> Geometry {
    let t = t.clamp(0.0, 1.0);
    let mut out = b.clone();
    if t >= 1.0 {
        return out;
    }
    // A label of one state only arrives with the marks, or leaves as they set off.
    let late = ((t - 0.8) / 0.2).clamp(0.0, 1.0);
    let early = (1.0 - t / 0.2).clamp(0.0, 1.0);
    for (k, panel) in out.panels.iter_mut().enumerate() {
        let Some(before) = a.panels.get(k) else {
            continue;
        };
        let old = keys(&before.items);
        let new = keys(&panel.items);
        let named = same_labels(&before.items, &panel.items);
        let mut kept = vec![false; before.items.len()];
        let partner: HashMap<_, usize> = old
            .iter()
            .enumerate()
            .filter_map(|(i, key)| key.map(|key| (key, i)))
            .collect();
        let mut matched = vec![false; before.items.len()];
        let mut grows = false;
        let mut items = Vec::with_capacity(panel.items.len());
        for ((item, key), same) in panel.items.iter().zip(&new).zip(&named) {
            if let (Item::Text(text), Some(i)) = (item, *same)
                && let Item::Text(was) = &before.items[i]
            {
                kept[i] = true;
                let (dx, dy) = (
                    mix(was.x, text.x, t) - text.x,
                    mix(was.y, text.y, t) - text.y,
                );
                let mut moved = text.clone();
                moved.x += dx;
                moved.y += dy;
                moved.bounds.x += dx;
                moved.bounds.y += dy;
                items.push(Item::Text(moved));
                continue;
            }
            let from = key.and_then(|key| partner.get(&key).copied());
            if let Some(i) = from {
                matched[i] = true;
            }
            let next = match (item, from.map(|i| &before.items[i])) {
                (
                    Item::Point { center, radius, .. },
                    Some(Item::Point {
                        center: c0,
                        radius: r0,
                        ..
                    }),
                ) => {
                    let mut moved = item.clone();
                    if let Item::Point {
                        center: c,
                        radius: r,
                        ..
                    } = &mut moved
                    {
                        *c = mix2(*c0, *center, t);
                        *r = mix(*r0, *radius, t);
                    }
                    moved
                }
                (Item::Rect { rect, .. }, Some(Item::Rect { rect: r0, .. })) => {
                    let mut moved = item.clone();
                    if let Item::Rect { rect: r, .. } = &mut moved {
                        *r = Rect::new(
                            mix(r0.x, rect.x, t),
                            mix(r0.y, rect.y, t),
                            mix(r0.w, rect.w, t),
                            mix(r0.h, rect.h, t),
                        );
                    }
                    moved
                }
                (Item::Line { path, .. }, Some(Item::Line { path: p0, .. }))
                | (Item::Area { path, .. }, Some(Item::Area { path: p0, .. })) => {
                    match morph(p0, path, t) {
                        Some(p) => {
                            let mut moved = item.clone();
                            if let Item::Line { path, .. } | Item::Area { path, .. } = &mut moved {
                                *path = p;
                            }
                            moved
                        }
                        None => {
                            grows = true;
                            item.clone()
                        }
                    }
                }
                _ if item.role() == Role::Mark => fade(item, t),
                _ if matches!(item.role(), Role::Label | Role::Annotation | Role::Leader) => {
                    fade(item, late)
                }
                _ => item.clone(),
            };
            items.push(next);
        }
        // Marks the new state no longer has fade out where they were, and so do its labels,
        // quickly, so a name never lingers away from its mark.
        for (i, item) in before.items.iter().enumerate() {
            if old[i].is_some() && !matched[i] {
                items.push(fade(item, 1.0 - t));
            } else if is_label(item) && !kept[i] && early > 0.0 {
                items.push(fade(item, early));
            }
        }
        // A line that gained observations grows from its old end to its new one; the plot's
        // clip does the revealing.
        if grows
            && let (Some(from), Some(to)) = (right_end(&before.items), right_end(&panel.items))
            && to > from
        {
            let edge = mix(from, to, t);
            panel.plot.w = (edge - panel.plot.x).max(0.0);
        }
        panel.items = items;
        // Readouts belong to the new state only once it has arrived.
        panel.observations.clear();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    use serde_json::{Value, json};

    fn draw(rows: &[Value], kind: &str) -> Geometry {
        let block = ChartBlock::from_value(
            &json!({"type": kind, "title": "T", "encoding": {"x": "x", "y": "y", "color": "s"}}),
        )
        .unwrap();
        layout(
            &block,
            rows,
            &ChartTheme::default(),
            &Options {
                width: 640.0,
                ..Options::default()
            },
        )
    }

    fn points(g: &Geometry) -> Vec<[f64; 2]> {
        g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Point {
                    center,
                    role: Role::Mark,
                    ..
                } => Some(*center),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn matched_points_travel_and_arrive_exactly() {
        let a = draw(
            &[
                json!({"x": 1, "y": 1, "s": "A"}),
                json!({"x": 2, "y": 2, "s": "B"}),
            ],
            "scatter",
        );
        let b = draw(
            &[
                json!({"x": 3, "y": 3, "s": "A"}),
                json!({"x": 2, "y": 2, "s": "B"}),
            ],
            "scatter",
        );
        let half = between(&a, &b, 0.5);
        let (pa, pb, ph) = (points(&a), points(&b), points(&half));
        assert_eq!(ph.len(), 2, "no mark doubles while matched");
        for k in 0..2 {
            assert!((ph[k][0] - (pa[k][0] + pb[k][0]) / 2.0).abs() < 1e-9);
        }
        assert_eq!(between(&a, &b, 1.0), b, "the glide ends on the new state");
        assert!(
            half.panels[0].observations.is_empty(),
            "no readout between states"
        );
    }

    fn labels(g: &Geometry) -> Vec<(String, [f64; 2], f32)> {
        g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Label => {
                    Some((t.text.clone(), [t.x, t.y], t.color.0[3]))
                }
                _ => None,
            })
            .collect()
    }

    fn named(rows: &[Value]) -> Geometry {
        let block = ChartBlock::from_value(&json!({"type": "scatter", "title": "T",
            "encoding": {"x": "x", "y": "y", "label": "name"}}))
        .unwrap();
        layout(
            &block,
            rows,
            &ChartTheme::default(),
            &Options {
                width: 640.0,
                ..Options::default()
            },
        )
    }

    #[test]
    fn a_name_in_both_states_travels_with_its_mark_at_full_strength() {
        let frame = |shift: f64| {
            named(&[
                json!({"x": 1.0 + shift, "y": 1, "name": "Andorra"}),
                json!({"x": 5.0 + shift, "y": 4, "name": "Latvia"}),
                json!({"x": 9.0, "y": 2, "name": "Gabon"}),
            ])
        };
        let (a, b) = (frame(0.0), frame(2.0));
        let (la, lb) = (labels(&a), labels(&b));
        assert!(!la.is_empty() && la.len() == lb.len(), "{la:?} {lb:?}");
        for t in [0.1, 0.5, 0.9] {
            let lh = labels(&between(&a, &b, t));
            assert_eq!(lh.len(), lb.len(), "each name shows once at {t}: {lh:?}");
            for ((name, at, alpha), (_, end, full)) in lh.iter().zip(&lb) {
                let start = la.iter().find(|l| &l.0 == name).unwrap().1;
                assert_eq!(alpha, full, "{name} keeps its strength at {t}");
                assert!(
                    (at[0] - (start[0] + (end[0] - start[0]) * t)).abs() < 1e-9,
                    "{name} at {t}"
                );
            }
        }
    }

    #[test]
    fn a_name_of_one_state_only_leaves_early_and_arrives_late() {
        let a = named(&[json!({"x": 1, "y": 1, "name": "Andorra"})]);
        let b = named(&[json!({"x": 1, "y": 1, "name": "Latvia"})]);
        let start = labels(&between(&a, &b, 0.05));
        assert!(
            start.iter().any(|l| l.0 == "Andorra" && l.2 > 0.0),
            "{start:?}"
        );
        let middle = labels(&between(&a, &b, 0.5));
        assert!(
            middle.iter().all(|l| l.2 == 0.0),
            "no name between: {middle:?}"
        );
        let end = labels(&between(&a, &b, 0.95));
        assert!(end.iter().any(|l| l.0 == "Latvia" && l.2 > 0.0), "{end:?}");
        assert!(!end.iter().any(|l| l.0 == "Andorra"), "{end:?}");
    }

    #[test]
    fn a_mark_without_partner_fades_and_a_growing_line_is_revealed() {
        let a = draw(&[json!({"x": 1, "y": 1, "s": "A"})], "scatter");
        let b = draw(
            &[
                json!({"x": 1, "y": 1, "s": "A"}),
                json!({"x": 2, "y": 2, "s": "A"}),
            ],
            "scatter",
        );
        let half = between(&a, &b, 0.25);
        let alphas: Vec<f32> = half.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Point {
                    fill: Some(f),
                    role: Role::Mark,
                    ..
                } => Some(f.color.0[3]),
                _ => None,
            })
            .collect();
        assert_eq!(alphas.len(), 2);
        assert!(
            alphas[1] < alphas[0] * 0.5,
            "the newcomer is still faint: {alphas:?}"
        );
        let rows = |n: usize| {
            (1..=n)
                .map(|i| json!({"x": format!("2024-{i:02}-01"), "y": i, "s": "A"}))
                .collect::<Vec<_>>()
        };
        // Playback fits the axes to every frame, so the short line ends inside the plot.
        let long = draw(&rows(6), "line");
        let block = ChartBlock::from_value(
            &json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "y", "color": "s"}}),
        )
        .unwrap();
        let opts = Options {
            width: 640.0,
            fit: Some(long.fit.clone()),
            ..Options::default()
        };
        let short = layout(&block, &rows(3), &ChartTheme::default(), &opts);
        let mid = between(&short, &long, 0.5);
        assert!(
            mid.panels[0].plot.w < long.panels[0].plot.w,
            "the clip reveals the new end"
        );
    }
}
