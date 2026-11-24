//! Chart text for WebGL (FEA-CH-30): each text item of the model is drawn once with the canvas
//! text API into an atlas and drawn at its model anchor. Each cell fits the ink the browser
//! actually paints, including synthesized weights and fallback faces. The atlas reports the faces
//! that had not loaded yet, so the host draws again once they have.

use crate::mesh::{Mesh, Motion};
use richview_chart::geometry::{Anchor, Face, Text};
use richview_chart::layout::ChartTheme;
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

const PAD: f64 = 3.0;
const ATLAS_WIDTH: f64 = 2048.0;

pub struct Atlas {
    pub canvas: HtmlCanvasElement,
    /// Per text index: six vertices of [`super::gl::TEXT_FLOATS`] floats.
    pub vertices: Vec<f32>,
    pub ranges: Vec<(i32, i32)>,
    /// Each font this atlas drew in a fallback face, with the characters drawn in it.
    pub unloaded: Vec<(String, String)>,
}

/// The texture holds a copy of the atlas: its pixels go as soon as it drops, not when the
/// garbage collector next runs, so drawing many charts in turn never piles up atlases.
impl Drop for Atlas {
    fn drop(&mut self) {
        self.canvas.set_width(0);
        self.canvas.set_height(0);
    }
}

pub fn family(theme: &ChartTheme, face: Face) -> String {
    match face {
        Face::Data => theme.data_font.clone(),
        Face::Figures => theme
            .figures_font
            .clone()
            .unwrap_or_else(|| theme.data_font.clone()),
        Face::Ui => theme.ui_font.clone(),
    }
}

struct Cell {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

#[derive(Default)]
struct Lettering<'a> {
    font: &'a str,
    align: &'a str,
}

impl<'a> Lettering<'a> {
    fn set(&mut self, ctx: &CanvasRenderingContext2d, font: &'a str, align: &'a str) {
        if font != self.font {
            ctx.set_font(font);
            self.font = font;
        }
        if align != self.align {
            ctx.set_text_align(align);
            self.align = align;
        }
    }
}

fn ink_cell(ctx: &CanvasRenderingContext2d, t: &Text, dpr: f64) -> Cell {
    let b = t.bounds;
    let (mut left, mut top, mut right, mut bottom) =
        (b.x - t.x, b.y - t.y, b.right() - t.x, b.bottom() - t.y);
    if let Ok(ink) = ctx.measure_text(&t.text) {
        left = left.min(-ink.actual_bounding_box_left());
        top = top.min(-ink.actual_bounding_box_ascent());
        right = right.max(ink.actual_bounding_box_right());
        bottom = bottom.max(ink.actual_bounding_box_descent());
    }
    Cell {
        left: left - PAD,
        top: top - PAD,
        width: ((right - left + 2.0 * PAD) * dpr).ceil(),
        height: ((bottom - top + 2.0 * PAD) * dpr).ceil(),
    }
}

/// Rasterise every text of `mesh` at `dpr` device pixels per CSS pixel.
pub fn build(
    document: &web_sys::Document,
    mesh: &Mesh,
    theme: &ChartTheme,
    dpr: f64,
) -> Result<Option<Atlas>, String> {
    if mesh.texts.is_empty() {
        return Ok(None);
    }
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|_| "no canvas")?
        .dyn_into()
        .map_err(|_| "no canvas")?;
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .map_err(|_| "no 2d")?
        .ok_or("no 2d")?
        .dyn_into()
        .map_err(|_| "no 2d")?;
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    let styles: Vec<_> = mesh
        .texts
        .iter()
        .map(|q| {
            let t = &q.text;
            let font = format!("{} {}px {}", t.weight, t.size, family(theme, t.face));
            let align = match t.anchor {
                Anchor::Start => "left",
                Anchor::Middle => "center",
                Anchor::End => "right",
            };
            (font, align)
        })
        .collect();
    let mut fonts: BTreeMap<&str, BTreeSet<char>> = BTreeMap::new();
    let mut letters = Lettering::default();
    let cells: Vec<_> = mesh
        .texts
        .iter()
        .zip(&styles)
        .map(|(q, (font, align))| {
            letters.set(&ctx, font, align);
            fonts.entry(font).or_default().extend(q.text.text.chars());
            ink_cell(&ctx, &q.text, dpr)
        })
        .collect();
    // Shelf packing is in device pixels. A single cell can need more than a standard row.
    let width = cells
        .iter()
        .fold(ATLAS_WIDTH, |width, cell| width.max(cell.width));
    let mut positions = Vec::with_capacity(cells.len());
    let (mut x, mut y, mut shelf) = (0.0f64, 0.0f64, 0.0f64);
    for cell in &cells {
        if x + cell.width > width {
            x = 0.0;
            y += shelf;
            shelf = 0.0;
        }
        positions.push((x, y));
        x += cell.width;
        shelf = shelf.max(cell.height);
    }
    let height = y + shelf;
    canvas.set_width(width as u32);
    canvas.set_height(height as u32);
    ctx.set_fill_style_str("#ffffff");
    ctx.set_text_baseline("alphabetic");
    let mut vertices = Vec::with_capacity(mesh.texts.len() * 6 * super::gl::TEXT_FLOATS);
    let mut ranges = Vec::with_capacity(mesh.texts.len());
    let mut letters = Lettering::default();
    for (((q, (font, align)), cell), (cx, cy)) in
        mesh.texts.iter().zip(&styles).zip(&cells).zip(positions)
    {
        let t = &q.text;
        letters.set(&ctx, font, align);
        let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, cx, cy);
        let _ = ctx.fill_text(&t.text, -cell.left, -cell.top);
        // Quad over the box, rotated about the anchor point.
        let first = (vertices.len() / super::gl::TEXT_FLOATS) as i32;
        let (x0, y0) = (t.x + cell.left, t.y + cell.top);
        let (x1, y1) = (x0 + cell.width / dpr, y0 + cell.height / dpr);
        let (u0, v0) = (cx / width, cy / height);
        let (u1, v1) = ((cx + cell.width) / width, (cy + cell.height) / height);
        let (s, c) = (t.rotate.to_radians().sin(), t.rotate.to_radians().cos());
        let rot = |px: f64, py: f64| -> [f64; 2] {
            let (dx, dy) = (px - t.x, py - t.y);
            [t.x + dx * c - dy * s, t.y + dx * s + dy * c]
        };
        let [r, g, bl, a] = t.color.0;
        let kind = if q.motion == Motion::Late {
            Motion::Late as u8 as f32
        } else {
            0.0
        };
        let corners = [
            (x0, y0, u0, v0),
            (x1, y0, u1, v0),
            (x1, y1, u1, v1),
            (x0, y0, u0, v0),
            (x1, y1, u1, v1),
            (x0, y1, u0, v1),
        ];
        for (px, py, u, v) in corners {
            let [qx, qy] = rot(px, py);
            vertices
                .extend_from_slice(&[qx as f32, qy as f32, u as f32, v as f32, r, g, bl, a, kind]);
        }
        ranges.push((first, 6));
    }
    let _ = ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    // A face that has not loaded draws in a fallback; `check` asks per font and characters, so a
    // face that covers only some characters (unicode-range) counts for the texts that use them.
    let faces = document.fonts();
    let unloaded = fonts
        .into_iter()
        .map(|(font, chars)| (font.to_owned(), chars.into_iter().collect::<String>()))
        .filter(|(font, text)| !faces.check_with_text(font, text).unwrap_or(true))
        .collect();
    Ok(Some(Atlas {
        canvas,
        vertices,
        ranges,
        unloaded,
    }))
}
