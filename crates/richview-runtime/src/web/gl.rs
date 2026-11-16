//! WebGL2 drawing of a tessellated chart (ARC-18). One context serves every chart on the page:
//! each chart draws into it and is copied to the chart's own 2D canvas, so browsers' limits on
//! live contexts never apply and a lost context costs one redraw.

use crate::mesh::{ANALYTIC_CIRCLE, FLOATS, Mesh, Op};
use richview_chart::geometry::{Hatch, Rect};
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

const SOLID_VS: &str = r#"#version 300 es
in vec2 a_pos; in vec4 a_color; in vec2 a_anchor; in float a_kind; in float a_delay;
uniform vec2 u_size; uniform float u_time; uniform vec4 u_dur; uniform vec4 u_bez; uniform float u_label;
out vec4 v_color; out vec2 v_pos; out vec2 v_anchor; out float v_kind; out float v_progress;
out vec2 v_circle_delta; flat out float v_circle_radius;
const float CIRCLE_KIND = __ANALYTIC_CIRCLE__;
float ease(float t) { t = clamp(t, 0.0, 1.0); return 1.0 - pow(1.0 - t, 3.0); }
float curve(float t) {
  if (t <= 0.0) return 0.0;
  if (t >= 1.0) return 1.0;
  float lo = 0.0; float hi = 1.0;
  for (int i = 0; i < 20; i++) {
    float s = 0.5 * (lo + hi); float r = 1.0 - s;
    float x = 3.0 * r * r * s * u_bez.x + 3.0 * r * s * s * u_bez.z + s * s * s;
    if (x < t) lo = s; else hi = s;
  }
  float s = 0.5 * (lo + hi); float r = 1.0 - s;
  return 3.0 * r * r * s * u_bez.y + 3.0 * r * s * s * u_bez.w + s * s * s;
}
void main() {
  bool circle = a_kind >= CIRCLE_KIND;
  vec2 p = a_pos; float k = circle ? a_kind - CIRCLE_KIND : a_kind; float prog = 1.0; float alpha = 1.0;
  // An analytic circle keeps its radius in the delay slot and so never waits.
  float radius = circle ? a_delay : 0.0; float delay = circle ? 0.0 : a_delay;
  if (k == 1.0 || k == 5.0) {
    prog = curve((u_time - delay) / u_dur.x);
    if (k == 1.0) p.y = a_anchor.y + (p.y - a_anchor.y) * prog; else p.x = a_anchor.x + (p.x - a_anchor.x) * prog;
  } else if (k == 2.0 || k == 3.0 || k == 9.0) {
    // A sweep or a fan is a position round the figure: it never runs past its end.
    prog = min(curve(u_time / u_dur.y), 1.0);
  } else if (k == 4.0) {
    prog = curve(u_time / u_dur.z); alpha = clamp(prog, 0.0, 1.0);
  } else if (k == 6.0) {
    prog = ease((u_time - u_dur.w) / u_label); alpha = prog;
  } else if (k == 7.0) {
    prog = curve((u_time - delay) / u_dur.x); p = a_anchor + (p - a_anchor) * prog;
  } else if (k == 8.0) {
    prog = curve((u_time - delay) / u_dur.z); alpha = clamp(prog, 0.0, 1.0);
    float s = 0.5 + 0.5 * alpha;
    if (circle) radius *= s; else p = a_anchor + (p - a_anchor) * s;
  }
  v_color = vec4(a_color.rgb, a_color.a * alpha);
  v_pos = p; v_anchor = a_anchor; v_kind = k; v_progress = prog;
  v_circle_delta = p - a_anchor; v_circle_radius = radius;
  vec2 clip = p / u_size * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
}"#;

const SOLID_FS: &str = r#"#version 300 es
precision highp float;
in vec4 v_color; in vec2 v_pos; in vec2 v_anchor; in float v_kind; in float v_progress;
in vec2 v_circle_delta; flat in float v_circle_radius;
uniform vec2 u_sweep; uniform int u_hatch; uniform float u_hatch_pitch; uniform vec4 u_hatch_ink; uniform float u_dpr;
out vec4 o;
const float WASH = 48.0;
void main() {
  vec4 c = v_color;
  if (v_circle_radius > 0.0) {
    c.a *= clamp(0.5 + (v_circle_radius - length(v_circle_delta)) * u_dpr, 0.0, 1.0);
    if (c.a <= 0.0) discard;
  }
  // The sweep's front runs WASH past the right edge so the band behind it ends fully drawn:
  // strokes end at the front, fills fade in over the band behind it.
  float front = u_sweep.x + v_progress * (u_sweep.y + WASH);
  if (v_kind > 1.5 && v_kind < 2.5 && v_pos.x > front) discard;
  if (v_kind > 8.5 && v_kind < 9.5) c.a *= clamp((front - v_pos.x) / WASH, 0.0, 1.0);
  if (v_kind > 2.5 && v_kind < 3.5) {
    vec2 d = v_pos - v_anchor;
    float a = atan(d.x, -d.y);
    if (a < 0.0) a += 6.2831853;
    if (a > v_progress * 6.2831853 + 0.0001) discard;
  }
  if (u_hatch > 0) {
    vec2 p = gl_FragCoord.xy / u_dpr;
    bool on = false;
    float k = u_hatch_pitch;
    if (u_hatch == 1) on = mod(p.x + p.y, k) < 1.2;
    else if (u_hatch == 2) on = mod(p.x - p.y, k) < 1.2;
    else if (u_hatch == 3) on = mod(p.y, k) < 1.2;
    else if (u_hatch == 4) on = mod(p.x, k) < 1.2;
    else if (u_hatch == 5) on = mod(p.x, k) < 1.2 || mod(p.y, k) < 1.2;
    else on = length(mod(p, k) - k / 2.0) < 0.9;
    if (on) c = vec4(u_hatch_ink.rgb, c.a);
  }
  o = vec4(c.rgb * c.a, c.a);
}"#;

const TEXT_VS: &str = r#"#version 300 es
in vec2 a_pos; in vec2 a_uv; in vec4 a_color; in float a_kind;
uniform vec2 u_size; uniform float u_time; uniform vec4 u_dur; uniform float u_label;
out vec2 v_uv; out vec4 v_color;
void main() {
  float alpha = 1.0;
  if (a_kind == 6.0) { float t = clamp((u_time - u_dur.w) / u_label, 0.0, 1.0); alpha = 1.0 - pow(1.0 - t, 3.0); }
  v_uv = a_uv; v_color = vec4(a_color.rgb, a_color.a * alpha);
  vec2 clip = a_pos / u_size * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
}"#;

const TEXT_FS: &str = r#"#version 300 es
precision highp float;
in vec2 v_uv; in vec4 v_color; uniform sampler2D u_tex; out vec4 o;
void main() { float a = texture(u_tex, v_uv).a * v_color.a; o = vec4(v_color.rgb * a, a); }"#;

pub const TEXT_FLOATS: usize = 9;

/// Motion clock for one draw: elapsed ms and the entrance's durations.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    pub time: f32,
    /// Grow, sweep or fan, settle, and when labels start.
    pub durations: [f32; 4],
    /// The entrance curve as CSS cubic-bezier control points `[x1, y1, x2, y2]`.
    pub bezier: [f32; 4],
    /// Duration of the label fade.
    pub label: f32,
    /// Sweep start and width in CSS px.
    pub sweep: [f32; 2],
}

impl Clock {
    pub fn settled() -> Clock {
        Clock {
            time: 1.0e9,
            durations: [500.0, 700.0, 400.0, 0.0],
            bezier: [0.33, 1.0, 0.68, 1.0],
            label: 200.0,
            sweep: [0.0, 1.0e6],
        }
    }
}

struct Program {
    program: WebGlProgram,
    uniforms: Vec<(&'static str, Option<WebGlUniformLocation>)>,
    shaders: Vec<web_sys::WebGlShader>,
}

impl Program {
    fn uniform(&self, name: &str) -> Option<&WebGlUniformLocation> {
        self.uniforms
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, u)| u.as_ref())
    }

    fn release(&self, gl: &Gl) {
        for shader in &self.shaders {
            gl.delete_shader(Some(shader));
        }
        gl.delete_program(Some(&self.program));
    }
}

/// Uploaded geometry of one chart: kept until the chart lays out again.
pub struct Uploaded {
    vao: WebGlVertexArrayObject,
    buffer: WebGlBuffer,
    text_vao: Option<WebGlVertexArrayObject>,
    text_buffer: Option<WebGlBuffer>,
    texture: Option<WebGlTexture>,
    /// Text vertex range per text index.
    text_ranges: Vec<(i32, i32)>,
    /// The renderer these objects belong to; a replaced context cannot draw them.
    generation: u32,
}

impl Uploaded {
    /// Whether these objects live in `renderer`'s context.
    pub fn belongs_to(&self, renderer: &Renderer) -> bool {
        self.generation == renderer.generation
    }
}

pub struct Renderer {
    pub canvas: HtmlCanvasElement,
    pub gl: Gl,
    solid: Program,
    text: Program,
    generation: u32,
    pending: bool,
    valid: bool,
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // Objects from a lost generation must not be deleted on the restored context.
        if self.valid && !self.gl.is_context_lost() {
            self.solid.release(&self.gl);
            self.text.release(&self.gl);
        }
    }
}

const SOLID_UNIFORMS: &[&str] = &[
    "u_size",
    "u_time",
    "u_dur",
    "u_bez",
    "u_label",
    "u_sweep",
    "u_hatch",
    "u_hatch_ink",
    "u_hatch_pitch",
    "u_dpr",
];
const TEXT_UNIFORMS: &[&str] = &["u_size", "u_time", "u_dur", "u_label", "u_tex"];
const COMPLETION_STATUS_KHR: u32 = 0x91B1;

thread_local! {
    static GENERATION: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn compile(gl: &Gl, kind: u32, source: &str) -> Result<web_sys::WebGlShader, String> {
    let shader = gl.create_shader(kind).ok_or("no shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    Ok(shader)
}

fn link(gl: &Gl, vs: &str, fs: &str, attributes: &[&str]) -> Result<Program, String> {
    let program = gl.create_program().ok_or("no program")?;
    let vertex = match compile(gl, Gl::VERTEX_SHADER, vs) {
        Ok(shader) => shader,
        Err(error) => {
            gl.delete_program(Some(&program));
            return Err(error);
        }
    };
    let fragment = match compile(gl, Gl::FRAGMENT_SHADER, fs) {
        Ok(shader) => shader,
        Err(error) => {
            gl.delete_shader(Some(&vertex));
            gl.delete_program(Some(&program));
            return Err(error);
        }
    };
    gl.attach_shader(&program, &vertex);
    gl.attach_shader(&program, &fragment);
    for (i, name) in attributes.iter().enumerate() {
        gl.bind_attrib_location(&program, i as u32, name);
    }
    gl.link_program(&program);
    Ok(Program {
        program,
        uniforms: Vec::new(),
        shaders: vec![vertex, fragment],
    })
}

impl Renderer {
    pub fn new(document: &web_sys::Document) -> Result<Renderer, String> {
        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .map_err(|_| "no canvas")?
            .dyn_into()
            .map_err(|_| "no canvas")?;
        let attrs = web_sys::WebGlContextAttributes::new();
        attrs.set_antialias(true);
        attrs.set_stencil(true);
        attrs.set_premultiplied_alpha(true);
        attrs.set_preserve_drawing_buffer(false);
        attrs.set_alpha(true);
        let gl: Gl = canvas
            .get_context_with_context_options("webgl2", &attrs)
            .map_err(|_| "WebGL2 is unavailable")?
            .ok_or("WebGL2 is unavailable")?
            .dyn_into()
            .map_err(|_| "WebGL2 is unavailable")?;
        Self::from_context(canvas, gl)
    }

    /// Recreate programs and resource identity on this canvas's restored context.
    pub fn restored(&self) -> Result<Renderer, String> {
        Self::from_context(self.canvas.clone(), self.gl.clone())
    }

    fn from_context(canvas: HtmlCanvasElement, gl: Gl) -> Result<Renderer, String> {
        let parallel = gl
            .get_extension("KHR_parallel_shader_compile")
            .ok()
            .flatten()
            .is_some();
        let solid = link(
            &gl,
            &SOLID_VS.replace("__ANALYTIC_CIRCLE__", &format!("{ANALYTIC_CIRCLE:.1}")),
            SOLID_FS,
            &["a_pos", "a_color", "a_anchor", "a_kind", "a_delay"],
        )?;
        let text = match link(
            &gl,
            TEXT_VS,
            TEXT_FS,
            &["a_pos", "a_uv", "a_color", "a_kind"],
        ) {
            Ok(program) => program,
            Err(error) => {
                solid.release(&gl);
                return Err(error);
            }
        };
        let generation = GENERATION.with(|g| {
            g.set(g.get().wrapping_add(1));
            g.get()
        });
        let mut renderer = Renderer {
            canvas,
            gl,
            solid,
            text,
            generation,
            pending: true,
            valid: true,
        };
        if !parallel {
            renderer.finish()?;
        }
        Ok(renderer)
    }

    /// Whether programs are still compiling; no draw or uniform lookup is valid yet.
    pub fn pending(&self) -> bool {
        self.pending
    }

    /// Mark this generation invalid as soon as its context is lost.
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    pub fn invalid(&self) -> bool {
        !self.valid
    }

    /// Poll without waiting for the driver; a lost context must never be mistaken for success.
    pub fn poll(&mut self) -> Result<bool, String> {
        if self.invalid() || self.lost() {
            return Ok(false);
        }
        if !self.pending {
            return Ok(true);
        }
        if [&self.solid, &self.text].iter().any(|p| {
            !self
                .gl
                .get_program_parameter(&p.program, COMPLETION_STATUS_KHR)
                .as_bool()
                .unwrap_or(false)
        }) {
            return Ok(false);
        }
        self.finish()?;
        Ok(true)
    }

    fn finish(&mut self) -> Result<(), String> {
        for (program, uniforms) in [
            (&mut self.solid, SOLID_UNIFORMS),
            (&mut self.text, TEXT_UNIFORMS),
        ] {
            if !self
                .gl
                .get_program_parameter(&program.program, Gl::LINK_STATUS)
                .as_bool()
                .unwrap_or(false)
            {
                let mut error = self
                    .gl
                    .get_program_info_log(&program.program)
                    .unwrap_or_default();
                for shader in &program.shaders {
                    if !self
                        .gl
                        .get_shader_parameter(shader, Gl::COMPILE_STATUS)
                        .as_bool()
                        .unwrap_or(false)
                    {
                        error.push_str(&self.gl.get_shader_info_log(shader).unwrap_or_default());
                    }
                }
                return Err(error);
            }
            program.uniforms = uniforms
                .iter()
                .map(|name| (*name, self.gl.get_uniform_location(&program.program, name)))
                .collect();
            for shader in program.shaders.drain(..) {
                self.gl.detach_shader(&program.program, &shader);
                self.gl.delete_shader(Some(&shader));
            }
        }
        self.pending = false;
        Ok(())
    }

    pub fn lost(&self) -> bool {
        self.gl.is_context_lost()
    }

    /// Upload a chart's mesh and its text quads (`text` holds, per text index, the six vertices
    /// of its quad in [`TEXT_FLOATS`] floats each) with the text atlas canvas.
    pub fn upload(
        &self,
        mesh: &Mesh,
        text: &[f32],
        text_ranges: Vec<(i32, i32)>,
        atlas: Option<&HtmlCanvasElement>,
    ) -> Result<Uploaded, String> {
        let gl = &self.gl;
        let vao = gl.create_vertex_array().ok_or("no vertex array")?;
        gl.bind_vertex_array(Some(&vao));
        let buffer = gl.create_buffer().ok_or("no buffer")?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&buffer));
        unsafe {
            let view = js_sys::Float32Array::view(&mesh.vertices);
            gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &view, Gl::STATIC_DRAW);
        }
        let stride = (FLOATS * 4) as i32;
        for (i, (size, offset)) in [(2, 0), (4, 2), (2, 6), (1, 8), (1, 9)].iter().enumerate() {
            gl.enable_vertex_attrib_array(i as u32);
            gl.vertex_attrib_pointer_with_i32(
                i as u32,
                *size,
                Gl::FLOAT,
                false,
                stride,
                offset * 4,
            );
        }
        gl.bind_vertex_array(None);
        let (mut text_vao, mut text_buffer, mut texture) = (None, None, None);
        if let Some(atlas) = atlas
            && !text.is_empty()
        {
            let tvao = gl.create_vertex_array().ok_or("no vertex array")?;
            gl.bind_vertex_array(Some(&tvao));
            let tb = gl.create_buffer().ok_or("no buffer")?;
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&tb));
            unsafe {
                let view = js_sys::Float32Array::view(text);
                gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &view, Gl::STATIC_DRAW);
            }
            let stride = (TEXT_FLOATS * 4) as i32;
            for (i, (size, offset)) in [(2, 0), (2, 2), (4, 4), (1, 8)].iter().enumerate() {
                gl.enable_vertex_attrib_array(i as u32);
                gl.vertex_attrib_pointer_with_i32(
                    i as u32,
                    *size,
                    Gl::FLOAT,
                    false,
                    stride,
                    offset * 4,
                );
            }
            gl.bind_vertex_array(None);
            let tex = gl.create_texture().ok_or("no texture")?;
            gl.bind_texture(Gl::TEXTURE_2D, Some(&tex));
            gl.pixel_storei(Gl::UNPACK_PREMULTIPLY_ALPHA_WEBGL, 0);
            gl.tex_image_2d_with_u32_and_u32_and_html_canvas_element(
                Gl::TEXTURE_2D,
                0,
                Gl::RGBA as i32,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                atlas,
            )
            .map_err(|_| "text upload failed")?;
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
            text_vao = Some(tvao);
            text_buffer = Some(tb);
            texture = Some(tex);
        }
        Ok(Uploaded {
            vao,
            buffer,
            text_vao,
            text_buffer,
            texture,
            text_ranges,
            generation: self.generation,
        })
    }

    pub fn release(&self, up: Uploaded) {
        // Objects of a lost context went with it.
        if !up.belongs_to(self) {
            return;
        }
        let gl = &self.gl;
        gl.delete_vertex_array(Some(&up.vao));
        gl.delete_buffer(Some(&up.buffer));
        if let Some(v) = up.text_vao {
            gl.delete_vertex_array(Some(&v));
        }
        if let Some(b) = up.text_buffer {
            gl.delete_buffer(Some(&b));
        }
        if let Some(t) = up.texture {
            gl.delete_texture(Some(&t));
        }
    }

    /// Size the shared drawing buffer to at least `w`×`h` device pixels and clear `w`×`h` of it.
    pub fn begin(&self, w: u32, h: u32, background: [f32; 4]) {
        if self.canvas.width() < w || self.canvas.height() < h {
            self.canvas.set_width(self.canvas.width().max(w).max(1));
            self.canvas.set_height(self.canvas.height().max(h).max(1));
        }
        let gl = &self.gl;
        // Keep the viewport at one device-pixel origin when the shared buffer grows. Changing
        // its origin can change rasterized line-edge pixels even when the geometry is identical.
        gl.viewport(0, 0, w as i32, h as i32);
        gl.disable(Gl::SCISSOR_TEST);
        gl.clear_color(
            background[0] * background[3],
            background[1] * background[3],
            background[2] * background[3],
            background[3],
        );
        gl.clear_stencil(0);
        gl.clear(Gl::COLOR_BUFFER_BIT | Gl::STENCIL_BUFFER_BIT);
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::ONE, Gl::ONE_MINUS_SRC_ALPHA);
    }

    fn scissor(&self, clip: Option<Rect>, size: [f64; 2], dpr: f64) {
        let gl = &self.gl;
        match clip {
            None => gl.disable(Gl::SCISSOR_TEST),
            Some(r) => {
                let h = (size[1] * dpr).round();
                gl.enable(Gl::SCISSOR_TEST);
                let x = (r.x * dpr).floor();
                let y = (h - r.bottom() * dpr).floor();
                gl.scissor(
                    x as i32,
                    y as i32,
                    (r.w * dpr).ceil().max(0.0) as i32,
                    (r.h * dpr).ceil().max(0.0) as i32,
                );
            }
        }
    }

    /// Draw one uploaded chart of CSS size `size` at `dpr`.
    pub fn draw(&self, mesh: &Mesh, up: &Uploaded, size: [f64; 2], dpr: f64, clock: Clock) {
        self.draw_range(mesh, up, size, dpr, clock, 0..mesh.ops.len());
    }

    /// Draw a contiguous span of the same uploaded operations without changing their order.
    pub(super) fn draw_range(
        &self,
        mesh: &Mesh,
        up: &Uploaded,
        size: [f64; 2],
        dpr: f64,
        clock: Clock,
        range: std::ops::Range<usize>,
    ) {
        let gl = &self.gl;
        let set_common = |p: &Program| {
            gl.use_program(Some(&p.program));
            gl.uniform2f(p.uniform("u_size"), size[0] as f32, size[1] as f32);
            gl.uniform1f(p.uniform("u_time"), clock.time);
            gl.uniform4f(
                p.uniform("u_dur"),
                clock.durations[0],
                clock.durations[1],
                clock.durations[2],
                clock.durations[3],
            );
            gl.uniform4f(
                p.uniform("u_bez"),
                clock.bezier[0],
                clock.bezier[1],
                clock.bezier[2],
                clock.bezier[3],
            );
            gl.uniform1f(p.uniform("u_label"), clock.label.max(1.0));
        };
        let use_solid = || {
            set_common(&self.solid);
            gl.uniform2f(
                self.solid.uniform("u_sweep"),
                clock.sweep[0],
                clock.sweep[1],
            );
            gl.uniform1f(self.solid.uniform("u_dpr"), dpr as f32);
            gl.uniform1i(self.solid.uniform("u_hatch"), 0);
            gl.bind_vertex_array(Some(&up.vao));
        };
        let mut solid_active = false;
        for op in mesh.ops.get(range).unwrap_or(&[]) {
            match op {
                Op::Tris { first, count, clip } => {
                    if !solid_active {
                        use_solid();
                        solid_active = true;
                    }
                    self.scissor(*clip, size, dpr);
                    gl.draw_arrays(Gl::TRIANGLES, *first as i32, *count as i32);
                }
                Op::Stencil {
                    fan,
                    cover,
                    hatch,
                    clip,
                } => {
                    if !solid_active {
                        use_solid();
                        solid_active = true;
                    }
                    self.scissor(*clip, size, dpr);
                    gl.enable(Gl::STENCIL_TEST);
                    gl.color_mask(false, false, false, false);
                    gl.stencil_func(Gl::ALWAYS, 0, 0xff);
                    gl.stencil_op(Gl::KEEP, Gl::KEEP, Gl::INVERT);
                    gl.draw_arrays(Gl::TRIANGLES, fan.0 as i32, fan.1 as i32);
                    gl.color_mask(true, true, true, true);
                    gl.stencil_func(Gl::NOTEQUAL, 0, 0xff);
                    gl.stencil_op(Gl::ZERO, Gl::ZERO, Gl::ZERO);
                    if let Some((h, ink, scale)) = hatch {
                        let code = match h {
                            Hatch::Diagonal => 1,
                            Hatch::Back => 2,
                            Hatch::Horizontal => 3,
                            Hatch::Vertical => 4,
                            Hatch::Cross => 5,
                            Hatch::Dots => 6,
                        };
                        gl.uniform1i(self.solid.uniform("u_hatch"), code);
                        gl.uniform1f(self.solid.uniform("u_hatch_pitch"), 6.0 * scale.max(1.0));
                        let [r, g, b, a] = ink.0;
                        gl.uniform4f(self.solid.uniform("u_hatch_ink"), r, g, b, a);
                    }
                    gl.draw_arrays(Gl::TRIANGLES, cover.0 as i32, cover.1 as i32);
                    gl.uniform1i(self.solid.uniform("u_hatch"), 0);
                    gl.disable(Gl::STENCIL_TEST);
                }
                Op::Text { index, clip } => {
                    let (Some(vao), Some(tex)) = (&up.text_vao, &up.texture) else {
                        continue;
                    };
                    let Some(&(first, count)) = up.text_ranges.get(*index as usize) else {
                        continue;
                    };
                    if count == 0 {
                        continue;
                    }
                    set_common(&self.text);
                    solid_active = false;
                    gl.active_texture(Gl::TEXTURE0);
                    gl.bind_texture(Gl::TEXTURE_2D, Some(tex));
                    gl.uniform1i(self.text.uniform("u_tex"), 0);
                    gl.bind_vertex_array(Some(vao));
                    self.scissor(*clip, size, dpr);
                    gl.draw_arrays(Gl::TRIANGLES, first, count);
                }
            }
        }
        gl.bind_vertex_array(None);
        gl.disable(Gl::SCISSOR_TEST);
    }

    /// Copy the drawn `w`×`h` device pixels from the bottom of the shared buffer to the chart.
    pub fn present(&self, target: &web_sys::CanvasRenderingContext2d, w: u32, h: u32) {
        target.clear_rect(0.0, 0.0, w as f64, h as f64);
        let _ = target
            .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                &self.canvas,
                0.0,
                f64::from(self.canvas.height().saturating_sub(h)),
                w as f64,
                h as f64,
                0.0,
                0.0,
                w as f64,
                h as f64,
            );
    }
}
