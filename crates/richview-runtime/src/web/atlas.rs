//! Chart text for WebGL (FEA-CH-30): each text item of the model is drawn once with the canvas
//! text API into an atlas, in white, at its model position, and drawn as a textured quad over the
//! box the engine measured. Placement comes from the model; the browser only rasterises glyphs,
//! and only in the style's faces: the atlas reports the fonts whose faces had not loaded yet, so
//! the host draws again once they have.

use crate::mesh::{Mesh, Motion};
use richview_chart::geometry::{Anchor, Face};
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
    /// Each font this atlas drew before its faces had loaded, with the characters drawn in it: the
    /// browser drew those in a fallback face, which can be wider than the box the engine measured
    /// and spill into the next cell.
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
    // Shelf packing in device pixels.
    let mut cells = Vec::with_capacity(mesh.texts.len());
    let (mut x, mut y, mut shelf) = (0.0f64, 0.0f64, 0.0f64);
    for q in &mesh.texts {
        let b = q.text.bounds;
        let w = ((b.w + 2.0 * PAD) * dpr).ceil();
        let h = ((b.h + 2.0 * PAD) * dpr).ceil();
        if x + w > ATLAS_WIDTH {
            x = 0.0;
            y += shelf;
            shelf = 0.0;
        }
        cells.push((x, y, w, h));
        x += w;
        shelf = shelf.max(h);
    }
    let height = (y + shelf).max(1.0);
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|_| "no canvas")?
        .dyn_into()
        .map_err(|_| "no canvas")?;
    canvas.set_width(ATLAS_WIDTH as u32);
    canvas.set_height(height as u32);
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .map_err(|_| "no 2d")?
        .ok_or("no 2d")?
        .dyn_into()
        .map_err(|_| "no 2d")?;
    ctx.set_fill_style_str("#ffffff");
    ctx.set_text_baseline("alphabetic");
    let mut vertices = Vec::with_capacity(mesh.texts.len() * 6 * super::gl::TEXT_FLOATS);
    let mut ranges = Vec::with_capacity(mesh.texts.len());
    let mut fonts: BTreeMap<String, BTreeSet<char>> = BTreeMap::new();
    let mut last_font = String::new();
    let mut last_align = "";
    for (q, (cx, cy, cw, ch)) in mesh.texts.iter().zip(&cells) {
        let t = &q.text;
        let b = t.bounds;
        let font = format!("{} {}px {}", t.weight, t.size, family(theme, t.face));
        if font != last_font {
            ctx.set_font(&font);
            last_font.clone_from(&font);
        }
        fonts.entry(font).or_default().extend(t.text.chars());
        let (align, ax) = match t.anchor {
            Anchor::Start => ("left", t.x - b.x),
            Anchor::Middle => ("center", t.x - b.x),
            Anchor::End => ("right", t.x - b.x),
        };
        if align != last_align {
            ctx.set_text_align(align);
            last_align = align;
        }
        let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, *cx, *cy);
        let _ = ctx.fill_text(&t.text, PAD + ax, PAD + (t.y - b.y));
        // Quad over the box, rotated about the anchor point.
        let first = (vertices.len() / super::gl::TEXT_FLOATS) as i32;
        let (x0, y0, x1, y1) = (
            b.x - PAD,
            b.y - PAD,
            b.x - PAD + cw / dpr,
            b.y - PAD + ch / dpr,
        );
        let (u0, v0, u1, v1) = (
            cx / ATLAS_WIDTH,
            cy / height,
            (cx + cw) / ATLAS_WIDTH,
            (cy + ch) / height,
        );
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
        .map(|(font, chars)| (font, chars.into_iter().collect::<String>()))
        .filter(|(font, text)| !faces.check_with_text(font, text).unwrap_or(true))
        .collect();
    Ok(Some(Atlas {
        canvas,
        vertices,
        ranges,
        unloaded,
    }))
}
