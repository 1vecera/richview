//! Presets (STY-73). The five main presets are the builtin styles; the extended presets are
//! ordinary style files in the author syntax, each a main preset with colours and steps, so they
//! pass the same checks any author's file passes. Menus show the main five; the extended ones
//! appear under "More presets" (STY-05).

/// The extended presets, in the order "More presets" lists them.
pub const EXTENDED_PRESETS: [&str; 5] = ["midnight", "ledger", "meadow", "signal", "chalk"];

/// The style file an extended preset stands for.
pub fn recipe(name: &str) -> Option<&'static str> {
    Some(match name {
        "midnight" => include_str!("../../assets/presets/midnight.rstyle.yaml"),
        "ledger" => include_str!("../../assets/presets/ledger.rstyle.yaml"),
        "meadow" => include_str!("../../assets/presets/meadow.rstyle.yaml"),
        "signal" => include_str!("../../assets/presets/signal.rstyle.yaml"),
        "chalk" => include_str!("../../assets/presets/chalk.rstyle.yaml"),
        _ => return None,
    })
}

/// Every preset name: the five main presets, then the extended ones.
pub fn preset_names() -> Vec<&'static str> {
    super::builtin_style_names()
        .iter()
        .copied()
        .chain(EXTENDED_PRESETS)
        .collect()
}

/// Whether `name` names a preset, main or extended.
pub fn is_preset(name: &str) -> bool {
    super::builtin_style_names().contains(&name) || recipe(name).is_some()
}
