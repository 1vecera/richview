//! Colour in OKLCH, RichView's colour model.
//!
//! Colours are authored as CSS `oklch()` and every derived colour is computed here: conversion
//! between sRGB, linear sRGB, OKLab and OKLCH (Björn Ottosson's OKLab, as CSS Color 4 uses it);
//! interpolation in OKLCH along the shorter hue arc, as `color-mix(in oklch, …)` does; gamut
//! mapping to sRGB by chroma reduction (CSS Color 4, §13.2), applied only when a colour is
//! output; WCAG 2 contrast; OKLab ΔE; colour-vision simulation (Machado, Oliveira and
//! Fernandes, 2009, full severity, in linear sRGB); and the checks that categorical palettes and
//! sequential ramps must pass (director decision D25).

/// A colour in OKLCH: lightness 0–1, chroma from 0 (grey) to about 0.37 at the sRGB limit, hue in
/// degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// Chroma below which a colour is treated as grey: its hue carries no information.
const ACHROMATIC: f64 = 1e-4;

impl Oklch {
    pub const fn new(l: f64, c: f64, h: f64) -> Self {
        Self { l, c, h }
    }

    fn lab(self) -> [f64; 3] {
        let h = self.h.to_radians();
        [self.l, self.c * h.cos(), self.c * h.sin()]
    }

    fn from_lab([l, a, b]: [f64; 3]) -> Self {
        let c = a.hypot(b);
        let h = if c < ACHROMATIC {
            0.0
        } else {
            b.atan2(a).to_degrees().rem_euclid(360.0)
        };
        Self { l, c, h }
    }

    /// Linear-light sRGB, unclamped: channels outside 0–1 mean the colour is outside sRGB.
    pub fn linear(self) -> [f64; 3] {
        let [l, a, b] = self.lab();
        let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
        [
            4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
            -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
            -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
        ]
    }

    pub fn from_linear([r, g, b]: [f64; 3]) -> Self {
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        Self::from_lab([
            0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        ])
    }

    /// From gamma-encoded sRGB channels in 0–1.
    pub fn from_srgb(rgb: [f64; 3]) -> Self {
        Self::from_linear(rgb.map(to_linear))
    }

    /// Whether the colour lies inside sRGB, within rounding.
    pub fn in_gamut(self) -> bool {
        self.linear().iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v))
    }

    /// Gamma-encoded sRGB in 0–1, gamut mapped: chroma is reduced at constant lightness and hue
    /// until the colour clips to sRGB within one just-noticeable difference (CSS Color 4).
    pub fn srgb(self) -> [f64; 3] {
        const JND: f64 = 0.02;
        const EPSILON: f64 = 1e-4;
        let clip = |c: Oklch| c.linear().map(|v| to_srgb(v.clamp(0.0, 1.0)));
        if self.l >= 1.0 {
            return [1.0; 3];
        }
        if self.l <= 0.0 {
            return [0.0; 3];
        }
        if self.in_gamut() {
            return self.linear().map(|v| to_srgb(v.clamp(0.0, 1.0)));
        }
        let distance = |c: Oklch, rgb: [f64; 3]| delta_e(c, Oklch::from_srgb(rgb));
        let mut current = self;
        let mut clipped = clip(current);
        if distance(current, clipped) < JND {
            return clipped;
        }
        let (mut min, mut max, mut min_in_gamut) = (0.0, self.c, true);
        while max - min > EPSILON {
            current.c = (min + max) / 2.0;
            if min_in_gamut && current.in_gamut() {
                min = current.c;
                continue;
            }
            clipped = clip(current);
            let e = distance(current, clipped);
            if e < JND {
                if JND - e < EPSILON {
                    break;
                }
                min_in_gamut = false;
                min = current.c;
            } else {
                max = current.c;
            }
        }
        clipped
    }

    /// The gamut-mapped colour as `#rrggbb`, for outputs that cannot take `oklch()`.
    pub fn hex(self) -> String {
        let [r, g, b] = self.srgb().map(|v| (v * 255.0).round() as u8);
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// CSS `oklch()` text: lightness and chroma to three decimals, hue to one.
    pub fn css(self) -> String {
        let trim = |v: f64, decimals: usize| {
            let s = format!("{v:.decimals$}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s == "-0" { "0".to_string() } else { s.to_string() }
        };
        format!(
            "oklch({} {} {})",
            trim(self.l, 3),
            trim(self.c, 3),
            trim(if self.c < ACHROMATIC { 0.0 } else { self.h }, 1)
        )
    }

    /// Relative luminance (WCAG 2) of the gamut-mapped colour.
    pub fn luminance(self) -> f64 {
        let [r, g, b] = self.srgb().map(to_linear);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
}

/// Decode one sRGB channel to linear light; values outside 0–1 keep their sign.
pub fn to_linear(v: f64) -> f64 {
    if v.abs() <= 0.04045 {
        v / 12.92
    } else {
        v.signum() * ((v.abs() + 0.055) / 1.055).powf(2.4)
    }
}

/// Encode one linear-light channel as sRGB; values outside 0–1 keep their sign.
pub fn to_srgb(v: f64) -> f64 {
    if v.abs() <= 0.0031308 {
        v * 12.92
    } else {
        v.signum() * (1.055 * v.abs().powf(1.0 / 2.4) - 0.055)
    }
}

/// The CSS Color 4 named colours (148 keywords) as sRGB bytes, lowercase and sorted for binary
/// search. `transparent` and `currentcolor` name no colour and are not here.
const NAMED: [(&str, [u8; 3]); 148] = [
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkgrey", [169, 169, 169]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("grey", [128, 128, 128]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightgrey", [211, 211, 211]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
];

/// The sRGB bytes of a CSS named colour, in any case; `None` for any other word.
pub fn named(name: &str) -> Option<[u8; 3]> {
    let name = name.trim().to_ascii_lowercase();
    NAMED
        .binary_search_by_key(&name.as_str(), |(known, _)| known)
        .ok()
        .map(|i| NAMED[i].1)
}

/// CIE Lab with the D50 white point (CSS `lab()` and `lch()`) as OKLCH, through XYZ and the
/// Bradford adaptation to D65 (CSS Color 4 §10).
fn from_cie_lab([l, a, b]: [f64; 3]) -> Oklch {
    const EPSILON: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let cube = |f: f64| {
        if f.powi(3) > EPSILON {
            f.powi(3)
        } else {
            (116.0 * f - 16.0) / KAPPA
        }
    };
    let y = if l > KAPPA * EPSILON { fy.powi(3) } else { l / KAPPA };
    // The D50 white point.
    let xyz = [
        cube(fx) * 0.3457 / 0.3585,
        y,
        cube(fz) * (1.0 - 0.3457 - 0.3585) / 0.3585,
    ];
    let d65 = [
        0.955_576_6 * xyz[0] - 0.023_039_3 * xyz[1] + 0.063_163_6 * xyz[2],
        -0.028_289_5 * xyz[0] + 1.009_941_6 * xyz[1] + 0.021_007_7 * xyz[2],
        0.012_298_2 * xyz[0] - 0.020_483 * xyz[1] + 1.329_909_8 * xyz[2],
    ];
    Oklch::from_linear([
        3.240_969_941_904_522_6 * d65[0] - 1.537_383_177_570_094 * d65[1] - 0.498_610_760_293_003_4 * d65[2],
        -0.969_243_636_280_879_6 * d65[0] + 1.875_967_501_507_707_7 * d65[1] + 0.041_555_057_407_175_59 * d65[2],
        0.055_630_079_696_993_66 * d65[0] - 0.203_976_958_888_976_52 * d65[1] + 1.056_971_514_242_878_6 * d65[2],
    ])
}

/// sRGB from hue (degrees), saturation and lightness (0–1), as CSS `hsl()` reads them.
fn from_hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    let (s, l) = (s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let a = s * l.min(1.0 - l);
    let channel = |n: f64| {
        let k = (n + h / 30.0).rem_euclid(12.0);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [channel(0.0), channel(8.0), channel(4.0)]
}

/// Parse a CSS colour: a name, hex (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), `rgb()`, `rgba()`,
/// `hsl()`, `hsla()`, `lab()`, `lch()`, `oklab()` or `oklch()`: every form the style grammar
/// admits except `transparent` and `currentcolor`, which name no colour. Alpha is ignored.
/// Lightness is a number or a percentage, chroma a number or a percentage of its range, hue is in
/// degrees (`deg` optional) and `none` reads as zero.
pub fn parse(css: &str) -> Option<Oklch> {
    let css = css.trim();
    if let Some([r, g, b]) = named(css) {
        return Some(Oklch::from_srgb([r, g, b].map(|c| f64::from(c) / 255.0)));
    }
    if let Some(hex) = css.strip_prefix('#') {
        let digits: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
        let channel = |i: usize| match digits.len() {
            3 | 4 => Some((digits[i] * 17) as f64 / 255.0),
            6 | 8 => Some((digits[2 * i] * 16 + digits[2 * i + 1]) as f64 / 255.0),
            _ => None,
        };
        return Some(Oklch::from_srgb([channel(0)?, channel(1)?, channel(2)?]));
    }
    let (name, rest) = css.split_once('(')?;
    let body = rest.strip_suffix(')')?;
    let body = body.split('/').next()?;
    let mut parts: Vec<&str> = body
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|p| !p.is_empty())
        .collect();
    let name = name.trim().to_ascii_lowercase();
    // The comma forms of `rgba()` and `hsla()` carry the alpha as a fourth number.
    if matches!(name.as_str(), "rgb" | "rgba" | "hsl" | "hsla") && parts.len() == 4 {
        parts.pop();
    }
    let [l, x, y] = parts.as_slice() else {
        return None;
    };
    let number = |p: &str, percent_of: f64| -> Option<f64> {
        if p == "none" {
            return Some(0.0);
        }
        let v = match p.strip_suffix('%') {
            Some(p) => p.parse::<f64>().ok()? / 100.0 * percent_of,
            None => p.trim_end_matches("deg").parse::<f64>().ok()?,
        };
        v.is_finite().then_some(v)
    };
    match name.as_str() {
        "oklch" => Some(Oklch::new(
            number(l, 1.0)?,
            number(x, 0.4)?.max(0.0),
            number(y, 0.0)?.rem_euclid(360.0),
        )),
        "oklab" => Some(Oklch::from_lab([number(l, 1.0)?, number(x, 0.4)?, number(y, 0.4)?])),
        "lab" => Some(from_cie_lab([number(l, 100.0)?, number(x, 125.0)?, number(y, 125.0)?])),
        "lch" => {
            let (c, h) = (number(x, 150.0)?.max(0.0), number(y, 0.0)?.to_radians());
            Some(from_cie_lab([number(l, 100.0)?, c * h.cos(), c * h.sin()]))
        }
        "rgb" | "rgba" => {
            let channel = |p: &str| number(p, 255.0).map(|v| v.clamp(0.0, 255.0) / 255.0);
            Some(Oklch::from_srgb([channel(l)?, channel(x)?, channel(y)?]))
        }
        "hsl" | "hsla" => {
            // Saturation and lightness are percentages; a bare number reads as one too.
            let share = |p: &str| match p.strip_suffix('%') {
                Some(_) => number(p, 1.0),
                None => number(p, 1.0).map(|v| v / 100.0),
            };
            Some(Oklch::from_srgb(from_hsl(
                number(l, 0.0)?.rem_euclid(360.0),
                share(x)?,
                share(y)?,
            )))
        }
        _ => None,
    }
}

/// The colour `t` (0–1) of the way from `a` to `b` in OKLCH: lightness and chroma linearly, hue
/// along the shorter arc; a grey takes the other colour's hue (CSS `color-mix(in oklch, …)`).
pub fn mix(a: Oklch, b: Oklch, t: f64) -> Oklch {
    let (ha, hb) = match (a.c < ACHROMATIC, b.c < ACHROMATIC) {
        (true, false) => (b.h, b.h),
        (false, true) => (a.h, a.h),
        _ => (a.h, b.h),
    };
    let dh = (hb - ha + 540.0).rem_euclid(360.0) - 180.0;
    Oklch::new(
        a.l + (b.l - a.l) * t,
        a.c + (b.c - a.c) * t,
        (ha + dh * t).rem_euclid(360.0),
    )
}

/// The colour at `t` (0–1) along evenly spaced stops, mixed in OKLCH between neighbours.
pub fn ramp(stops: &[Oklch], t: f64) -> Oklch {
    match stops {
        [] => Oklch::new(0.0, 0.0, 0.0),
        [only] => *only,
        _ => {
            let x = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
            let i = (x.floor() as usize).min(stops.len() - 2);
            mix(stops[i], stops[i + 1], x - i as f64)
        }
    }
}

/// WCAG 2 contrast ratio of two colours, 1–21.
pub fn contrast(a: Oklch, b: Oklch) -> f64 {
    let (x, y) = (a.luminance(), b.luminance());
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Euclidean distance in OKLab (CSS Color 4 ΔEOK; a just-noticeable difference is about 0.02).
pub fn delta_e(a: Oklch, b: Oklch) -> f64 {
    let ([l1, a1, b1], [l2, a2, b2]) = (a.lab(), b.lab());
    ((l1 - l2).powi(2) + (a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt()
}

/// The least separation in normal vision and under each supported colour-vision simulation.
pub fn vision_distance(a: Oklch, b: Oklch) -> f64 {
    Vision::ALL.into_iter().fold(delta_e(a, b), |least, vision| {
        least.min(delta_e(vision.simulate(a), vision.simulate(b)))
    })
}

/// Reach a contrast target by mixing towards white or black in OKLCH. Colours already meeting
/// the target are preserved. The surface selects the endpoint; mapping happens at measurement.
pub fn with_contrast(color: Oklch, surface: Oklch, minimum: f64) -> Oklch {
    if contrast(color, surface) >= minimum {
        return color;
    }
    let endpoint = Oklch::new(if surface.luminance() < 0.18 { 1.0 } else { 0.0 }, 0.0, 0.0);
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..20 {
        let mid = (low + high) / 2.0;
        if contrast(mix(color, endpoint, mid), surface) >= minimum {
            high = mid;
        } else {
            low = mid;
        }
    }
    mix(color, endpoint, high)
}

/// A colour-vision deficiency, simulated at full severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vision {
    Protan,
    Deutan,
    Tritan,
}

impl Vision {
    pub const ALL: [Vision; 3] = [Vision::Protan, Vision::Deutan, Vision::Tritan];

    pub fn name(self) -> &'static str {
        match self {
            Vision::Protan => "protan",
            Vision::Deutan => "deutan",
            Vision::Tritan => "tritan",
        }
    }

    fn matrix(self) -> [[f64; 3]; 3] {
        match self {
            Vision::Protan => [
                [0.152286, 1.052583, -0.204868],
                [0.114503, 0.786281, 0.099216],
                [-0.003882, -0.048116, 1.051998],
            ],
            Vision::Deutan => [
                [0.367322, 0.860646, -0.227968],
                [0.280085, 0.672501, 0.047413],
                [-0.011820, 0.042940, 0.968881],
            ],
            Vision::Tritan => [
                [1.255528, -0.076749, -0.178779],
                [-0.078411, 0.930809, 0.147602],
                [0.004733, 0.691367, 0.303900],
            ],
        }
    }

    /// How the gamut-mapped colour appears with this deficiency.
    pub fn simulate(self, color: Oklch) -> Oklch {
        let rgb = color.srgb().map(to_linear);
        let m = self.matrix();
        Oklch::from_linear(std::array::from_fn(|i| {
            (m[i][0] * rgb[0] + m[i][1] * rgb[1] + m[i][2] * rgb[2]).clamp(0.0, 1.0)
        }))
    }
}

/// Least contrast of the first [`CONTRAST_SLOTS`] categorical colours with their surfaces (D25).
pub const MIN_CONTRAST: f64 = 3.0;
pub const CONTRAST_SLOTS: usize = 4;
/// Least OKLab ΔE between neighbouring categorical colours, for normal vision and under each
/// simulated deficiency (D25: ΔE×100 of 15 and 8).
pub const MIN_DELTA_E: f64 = 0.15;
pub const MIN_DELTA_E_CVD: f64 = 0.08;

/// Every way a categorical palette fails D25 on the given surfaces, as readable lines; empty when
/// it passes. Entries that are not colours, or lie outside sRGB, fail too.
pub fn palette_issues(palette: &[&str], surfaces: &[(&str, Oklch)]) -> Vec<String> {
    let mut issues = vec![];
    let mut colors = Vec::with_capacity(palette.len());
    for (i, text) in palette.iter().enumerate() {
        match parse(text) {
            Some(c) if c.in_gamut() => colors.push(c),
            Some(_) => issues.push(format!("series {} {text} lies outside sRGB", i + 1)),
            None => issues.push(format!("series {} {text} is not a colour", i + 1)),
        }
    }
    if issues.is_empty() {
        for (i, c) in colors.iter().take(CONTRAST_SLOTS).enumerate() {
            for (surface, s) in surfaces {
                let ratio = contrast(*c, *s);
                if ratio < MIN_CONTRAST {
                    issues.push(format!(
                        "series {} {} reaches {ratio:.2}:1 on {surface}",
                        i + 1,
                        palette[i]
                    ));
                }
            }
        }
        // Every pair among the first four, and each neighbouring pair after them (STY-22).
        let head = colors.len().min(CONTRAST_SLOTS);
        let mut pairs: Vec<(usize, usize)> = (0..head).flat_map(|i| (i + 1..head).map(move |j| (i, j))).collect();
        pairs.extend((CONTRAST_SLOTS..colors.len()).map(|j| (j - 1, j)));
        for (i, j) in pairs {
            let (a, b) = (colors[i], colors[j]);
            let pair = format!("series {}–{} {}/{}", i + 1, j + 1, palette[i], palette[j]);
            let normal = delta_e(a, b);
            if normal < MIN_DELTA_E {
                issues.push(format!("{pair} differ by ΔE {:.1} for normal vision", normal * 100.0));
            }
            for vision in Vision::ALL {
                let simulated = delta_e(vision.simulate(a), vision.simulate(b));
                if simulated < MIN_DELTA_E_CVD {
                    issues.push(format!(
                        "{pair} differ by ΔE {:.1} under {} simulation",
                        simulated * 100.0,
                        vision.name()
                    ));
                }
            }
        }
    }
    issues
}

/// Every way a diverging pair of arms fails: their middle and high stops differ by the same
/// ΔE as palette neighbours, for normal vision and under each simulated deficiency (STY-22).
pub fn diverging_issues(positive: &[&str], negative: &[&str]) -> Vec<String> {
    let parse_all = |stops: &[&str]| stops.iter().map(|s| parse(s)).collect::<Option<Vec<_>>>();
    let (Some(positive), Some(negative)) = (parse_all(positive), parse_all(negative)) else {
        return vec!["an arm has a stop that is not a colour".into()];
    };
    let mut issues = vec![];
    for (index, stop) in [(1, "middle"), (2, "high")] {
        let (a, b) = (positive[index], negative[index]);
        let normal = delta_e(a, b);
        if normal < MIN_DELTA_E {
            issues.push(format!(
                "the arms' {stop} stops differ by ΔE {:.1} for normal vision",
                normal * 100.0
            ));
        }
        for vision in Vision::ALL {
            let simulated = delta_e(vision.simulate(a), vision.simulate(b));
            if simulated < MIN_DELTA_E_CVD {
                issues.push(format!(
                    "the arms' {stop} stops differ by ΔE {:.1} under {} simulation",
                    simulated * 100.0,
                    vision.name()
                ));
            }
        }
    }
    issues
}

/// Every way a sequential ramp, low to high, fails on the given surfaces: lightness must move
/// monotonically away from each surface, and the high end must stand further from it than the low
/// end, by at least [`MIN_CONTRAST`].
pub fn ramp_issues(stops: &[&str], surfaces: &[(&str, Oklch)]) -> Vec<String> {
    let Some(stops) = stops.iter().map(|s| parse(s)).collect::<Option<Vec<_>>>() else {
        return vec!["the ramp has a stop that is not a colour".into()];
    };
    if stops.len() < 2 {
        return vec!["the ramp needs at least two stops".into()];
    }
    let mut issues = vec![];
    let samples: Vec<Oklch> = (0..=32).map(|k| ramp(&stops, k as f64 / 32.0)).collect();
    for (surface, s) in surfaces {
        let away = |c: &Oklch| (c.l - s.l).abs();
        if samples.windows(2).any(|w| away(&w[1]) <= away(&w[0])) {
            issues.push(format!(
                "the ramp's lightness does not move steadily away from {surface}"
            ));
        }
        let (lo, hi) = (contrast(stops[0], *s), contrast(stops[stops.len() - 1], *s));
        if hi <= lo || hi < MIN_CONTRAST {
            issues.push(format!(
                "the ramp's ends reach {lo:.2}:1 and {hi:.2}:1 on {surface}; the high end must stand out more, by at least {MIN_CONTRAST}:1"
            ));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    fn same(a: &str, b: &str) {
        let (a, b) = (
            parse(a).unwrap_or_else(|| panic!("{a}")),
            parse(b).unwrap_or_else(|| panic!("{b}")),
        );
        assert!(delta_e(a, b) < 0.01, "{a:?} and {b:?} are {} apart", delta_e(a, b));
    }

    #[test]
    fn every_named_colour_reads_as_its_hex() {
        assert_eq!(NAMED.len(), 148);
        assert!(NAMED.windows(2).all(|pair| pair[0].0 < pair[1].0), "sorted and unique");
        for (name, [r, g, b]) in NAMED {
            assert_eq!(named(name), Some([r, g, b]));
            same(name, &format!("#{r:02x}{g:02x}{b:02x}"));
            same(&name.to_ascii_uppercase(), name);
        }
        assert_eq!(named("rebeccapurple"), Some([102, 51, 153]));
        for word in ["transparent", "currentcolor", "notacolor", ""] {
            assert!(named(word).is_none() && parse(word).is_none(), "{word}");
        }
    }

    #[test]
    fn colour_functions_read_as_the_same_colours_in_any_notation() {
        same("rgb(255 0 0)", "red");
        same("rgb(255, 0, 0)", "#f00");
        same("rgba(255, 0, 0, 0.5)", "red");
        same("rgb(100% 0% 0% / 50%)", "red");
        same("rgb(10 20 30)", "#0a141e");
        same("hsl(0 100% 50%)", "red");
        same("hsl(120, 100%, 25%)", "green");
        same("hsla(240, 100%, 50%, 0.3)", "blue");
        same("hsl(210 50 40)", "hsl(210 50% 40%)");
        same("hsl(210 50% 40%)", "#336699");
        // CSS Color 4's own examples: sRGB red and white in CIE Lab and LCH (D50).
        same("lab(54.2905% 80.8049 69.8910)", "red");
        same("lch(54.2905% 106.8390 40.8526)", "red");
        same("lab(100% 0 0)", "white");
        same("lab(0% 0 0)", "black");
        same("lab(50 0 0)", "lab(50% 0 0)");
        same("oklch(0.628 0.2577 29.23)", "oklab(0.628 0.2249 0.1264)");
    }

    #[test]
    fn malformed_colours_read_as_none() {
        for css in [
            "rgb(1 2)",
            "rgb(1 2 3 4 5)",
            "hsl(1 2% x)",
            "lab(1 2)",
            "oklch()",
            "#12345",
            "#1234567",
            "rgb(1 2 3",
            "red green",
        ] {
            assert!(parse(css).is_none(), "{css}");
        }
    }

    #[test]
    fn contrast_adjustment_preserves_valid_colours_and_reaches_both_surfaces() {
        let blue = parse("oklch(0.5464 0.2508 263.51)").unwrap();
        let white = Oklch::new(1.0, 0.0, 0.0);
        assert_eq!(with_contrast(blue, white, 3.0), blue);
        for surface in [white, Oklch::new(0.2, 0.0, 0.0)] {
            let faint = mix(blue, surface, 0.85);
            let adjusted = with_contrast(faint, surface, 3.03);
            assert!(contrast(adjusted, surface) >= 3.03 - 1e-6);
            assert!(delta_e(adjusted, faint) > 0.01);
        }
    }

    #[test]
    fn hex_and_oklch_round_trip() {
        for hex in [
            "#195bff", "#1b6f9e", "#124a8c", "#ffffff", "#000000", "#9a9189", "#0db3a5",
        ] {
            assert_eq!(parse(hex).unwrap().hex(), hex);
            let css = parse(hex).unwrap().css();
            let back = parse(&css).unwrap();
            assert!(delta_e(back, parse(hex).unwrap()) < 0.001, "{hex} → {css}");
        }
        let red = parse("oklch(62.8% 0.2577 29.23deg)").unwrap();
        assert_eq!(red.hex(), "#ff0000");
        assert_eq!(parse("#f00").unwrap().hex(), "#ff0000");
        assert_eq!(parse("#195BFFcc").unwrap().hex(), "#195bff");
        let white = parse("#fff").unwrap();
        assert!(close(white.l, 1.0, 1e-6) && white.c < 1e-4);
        assert_eq!(parse("oklab(0.5 0 0)").unwrap().css(), "oklch(0.5 0 0)");
        assert_eq!(parse("oklch(0.7 none 120 / 50%)").unwrap(), Oklch::new(0.7, 0.0, 120.0));
        assert_eq!(parse("rgb(255 0 0)").unwrap().hex(), "#ff0000");
        assert!(parse("rgb(1 2)").is_none() && parse("#12345").is_none() && parse("oklch(1 2)").is_none());
    }

    #[test]
    fn gamut_mapping_reduces_chroma_and_keeps_lightness_and_hue() {
        let vivid = Oklch::new(0.7, 0.35, 150.0);
        assert!(!vivid.in_gamut());
        let mapped = Oklch::from_srgb(vivid.srgb());
        assert!(mapped.in_gamut());
        assert!(mapped.c < vivid.c);
        assert!(close(mapped.l, vivid.l, 0.02), "{mapped:?}");
        assert!(close(mapped.h, vivid.h, 3.0), "{mapped:?}");
        assert_eq!(Oklch::new(1.2, 0.1, 30.0).hex(), "#ffffff");
        assert_eq!(Oklch::new(-0.1, 0.1, 30.0).hex(), "#000000");
        let inside = Oklch::new(0.6, 0.1, 250.0);
        assert!(inside.in_gamut());
        assert!(delta_e(Oklch::from_srgb(inside.srgb()), inside) < 1e-6);
    }

    #[test]
    fn mixing_runs_in_oklch_along_the_shorter_hue_arc() {
        let mid = mix(Oklch::new(0.2, 0.1, 260.0), Oklch::new(0.8, 0.2, 260.0), 0.5);
        assert!(close(mid.l, 0.5, 1e-12) && close(mid.c, 0.15, 1e-12) && close(mid.h, 260.0, 1e-9));
        assert!(close(
            mix(Oklch::new(0.5, 0.1, 350.0), Oklch::new(0.5, 0.1, 10.0), 0.5).h,
            0.0,
            1e-9
        ));
        let grey = mix(Oklch::new(0.9, 0.0, 0.0), Oklch::new(0.5, 0.2, 250.0), 0.25);
        assert!(close(grey.h, 250.0, 1e-9), "a grey takes the other hue");
        let stops = [
            Oklch::new(0.9, 0.02, 260.0),
            Oklch::new(0.6, 0.12, 260.0),
            Oklch::new(0.3, 0.1, 262.0),
        ];
        assert_eq!(ramp(&stops, 0.0), stops[0]);
        assert_eq!(ramp(&stops, 0.5), stops[1]);
        assert_eq!(ramp(&stops, 1.0), stops[2]);
        assert!(close(ramp(&stops, 0.25).l, 0.75, 1e-12));
    }

    #[test]
    fn contrast_delta_e_and_vision_follow_their_definitions() {
        let (white, black) = (parse("#fff").unwrap(), parse("#000").unwrap());
        assert!(close(contrast(white, black), 21.0, 0.01));
        assert!(close(contrast(parse("#767676").unwrap(), white), 4.54, 0.01));
        assert!(close(delta_e(white, black), 1.0, 1e-6));
        // Red and green merge for protans and deutans, not for tritans.
        let (red, green) = (parse("#d62728").unwrap(), parse("#2ca02c").unwrap());
        assert!(delta_e(red, green) > 0.25);
        assert!(delta_e(Vision::Deutan.simulate(red), Vision::Deutan.simulate(green)) < 0.1);
        assert!(delta_e(Vision::Tritan.simulate(red), Vision::Tritan.simulate(green)) > 0.2);
        let grey = parse("#808080").unwrap();
        for vision in Vision::ALL {
            assert!(delta_e(vision.simulate(grey), grey) < 0.01, "{vision:?} keeps greys");
        }
    }

    #[test]
    fn palette_and_ramp_checks_name_each_failure() {
        let white = [("bg-chart", parse("#fff").unwrap())];
        let single_hue = ["#1B6F9E", "#5B9BD5", "#8FAADC", "#B4C7E7", "#D9E2F3"];
        let issues = palette_issues(&single_hue, &white);
        assert!(
            issues.iter().any(|i| i.contains("series 2 #5B9BD5 reaches 2.96")),
            "{issues:?}"
        );
        assert!(issues.iter().any(|i| i.contains("for normal vision")));
        assert!(issues.iter().any(|i| i.contains("protan")));
        assert_eq!(
            palette_issues(&["oklch(0.7 0.35 150)"], &white),
            ["series 1 oklch(0.7 0.35 150) lies outside sRGB"]
        );
        let good = ["oklch(0.55 0.15 260)", "oklch(0.55 0.13 50)", "oklch(0.4 0.08 160)"];
        assert!(
            palette_issues(&good, &white).is_empty(),
            "{:?}",
            palette_issues(&good, &white)
        );
        // Every pair among the first four is held to the rule, not only neighbours (STY-22).
        let pairwise = [
            "oklch(0.5 0.15 255)",
            "oklch(0.52 0.13 50)",
            "oklch(0.5 0.13 258)",
            "oklch(0.45 0.1 160)",
        ];
        assert!(
            palette_issues(&pairwise, &white)
                .iter()
                .any(|i| i.contains("series 1–3")),
            "{:?}",
            palette_issues(&pairwise, &white)
        );
        let blue = ["oklch(0.95 0.015 235)", "oklch(0.73 0.07 237)", "oklch(0.44 0.095 240)"];
        let close = ["oklch(0.95 0.015 235)", "oklch(0.72 0.07 240)", "oklch(0.45 0.095 238)"];
        let brick = ["oklch(0.95 0.015 25)", "oklch(0.70 0.13 30)", "oklch(0.44 0.095 30)"];
        assert!(diverging_issues(&blue, &brick).is_empty());
        assert!(diverging_issues(&blue, &close).iter().any(|i| i.contains("middle")));
        let rising = ["oklch(0.96 0.02 260)", "oklch(0.72 0.12 260)", "oklch(0.45 0.2 262)"];
        assert!(ramp_issues(&rising, &white).is_empty());
        let falling = ["oklch(0.45 0.2 262)", "oklch(0.72 0.12 260)", "oklch(0.96 0.02 260)"];
        assert_eq!(ramp_issues(&falling, &white).len(), 2);
        let dark = [("bg-chart", parse("oklch(0.19 0.016 260)").unwrap())];
        assert!(ramp_issues(&["oklch(0.25 0.035 262)", "oklch(0.8 0.1 255)"], &dark).is_empty());
    }
}
