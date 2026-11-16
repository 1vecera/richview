//! The five looks of a link-preview card (STY-70): each builtin style's surface, inks, faces and
//! structural voice. The colours are the builtin style files' tokens (`crates/richview-core/assets`)
//! converted from OKLCH to sRGB; richview-share stays free of the core crate.

/// One bundled face: the family name the rasterizer uses and the font file.
pub(crate) struct Face {
    pub(crate) family: &'static str,
    pub(crate) data: &'static [u8],
}

pub(crate) const PLEX: Face = Face {
    family: "IBM Plex Mono",
    data: include_bytes!("../fonts/ibm-plex-mono-400-latin-and-ext.ttf"),
};
const GROTESK: Face = Face {
    family: "Space Grotesk",
    data: include_bytes!("../fonts/space-grotesk-600-latin.ttf"),
};
const GEIST: Face = Face {
    family: "Geist",
    data: include_bytes!("../fonts/geist-400-latin-and-ext.ttf"),
};
const NEWSREADER: Face = Face {
    family: "Newsreader",
    data: include_bytes!("../fonts/newsreader-500-latin-and-ext.ttf"),
};
const SHOULDERS: Face = Face {
    family: "Big Shoulders",
    data: include_bytes!("../fonts/big-shoulders-800-latin.ttf"),
};
const PATRICK: Face = Face {
    family: "Patrick Hand",
    data: include_bytes!("../fonts/patrick-hand-400-latin.ttf"),
};

/// Every bundled face, in the order the rasterizer loads them.
pub(crate) const FACES: [&Face; 6] = [&PLEX, &GROTESK, &GEIST, &NEWSREADER, &SHOULDERS, &PATRICK];

/// The structural mark of the card, the style's `components.thumbnail.rule` (STY-70).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rule {
    /// No mark: the surface and the type carry the style (Sleek, Air).
    None,
    /// Hairlines above and below the text block (Scientific).
    Hairline,
    /// Heavy ink bars and a hard offset shadow under the panel (Brutalist).
    InkBars,
    /// Pencil strokes from a fixed seed (Hand-drawn).
    Pencil,
}

impl Rule {
    /// The token's value: `none`, `hairline`, `ink-bars` or `pencil`.
    pub fn parse(token: &str) -> Option<Rule> {
        match token {
            "none" => Some(Rule::None),
            "hairline" => Some(Rule::Hairline),
            "ink-bars" => Some(Rule::InkBars),
            "pencil" => Some(Rule::Pencil),
            _ => None,
        }
    }
}

pub(crate) struct Look {
    pub(crate) bg: &'static str,
    pub(crate) ink: &'static str,
    pub(crate) muted: &'static str,
    pub(crate) line: &'static str,
    pub(crate) panel: &'static str,
    pub(crate) accent: &'static str,
    pub(crate) title: &'static Face,
    pub(crate) body: &'static Face,
    /// The title is set in capitals (Brutalist's display face is a condensed capital face).
    pub(crate) capitals: bool,
    /// The title's line step, as a multiple of its size.
    pub(crate) leading: f32,
    /// The style's own rule, used when the caller passes none.
    pub(crate) rule: Rule,
    /// The panel's corner radius when the rule leaves it round.
    pub(crate) radius: u32,
}

const SLEEK: Look = Look {
    bg: "#0d1017",
    ink: "#eef0f4",
    muted: "#9aa1ad",
    line: "#2a303b",
    panel: "#ffffff",
    accent: "#2a5bff",
    title: &GROTESK,
    body: &GEIST,
    capitals: false,
    leading: 1.1,
    rule: Rule::None,
    radius: 10,
};
const AIR: Look = Look {
    bg: "#ffffff",
    ink: "#000000",
    muted: "#4f5868",
    line: "#d7dbe8",
    panel: "#ffffff",
    accent: "#2a3fd0",
    title: &GROTESK,
    body: &GEIST,
    capitals: false,
    leading: 1.1,
    rule: Rule::None,
    radius: 12,
};
const SCIENTIFIC: Look = Look {
    bg: "#fcfaf6",
    ink: "#1d1b18",
    muted: "#625f58",
    line: "#d9d6ce",
    panel: "#ffffff",
    accent: "#1c5c8e",
    title: &NEWSREADER,
    body: &GEIST,
    capitals: false,
    leading: 1.16,
    rule: Rule::Hairline,
    radius: 0,
};
const BRUTALIST: Look = Look {
    bg: "#faf8f4",
    ink: "#141210",
    muted: "#55514b",
    line: "#141210",
    panel: "#ffffff",
    accent: "#103f86",
    title: &SHOULDERS,
    body: &PLEX,
    capitals: true,
    leading: 1.0,
    rule: Rule::InkBars,
    radius: 0,
};
const HAND_DRAWN: Look = Look {
    bg: "#fbf6e9",
    ink: "#24221d",
    muted: "#5f5a4c",
    line: "#d9d1bb",
    panel: "#ffffff",
    accent: "#b8442b",
    title: &PATRICK,
    body: &PLEX,
    capitals: false,
    leading: 1.18,
    rule: Rule::Pencil,
    radius: 0,
};

/// The builtin style names, in the order of STY-07.
pub const STYLES: [&str; 5] = ["sleek", "air", "scientific", "brutalist", "hand-drawn"];

/// The look of a builtin style; any other name takes Sleek, the default (STY-06).
pub(crate) fn look(style: &str) -> &'static Look {
    match style {
        "air" => &AIR,
        "scientific" => &SCIENTIFIC,
        "brutalist" => &BRUTALIST,
        "hand-drawn" => &HAND_DRAWN,
        _ => &SLEEK,
    }
}
