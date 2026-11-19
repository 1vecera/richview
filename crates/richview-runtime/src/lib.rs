//! The report runtime (ARC-06): it finds each chart figure in a report, lays its chart out at the
//! width the figure occupies with the engine, and draws the geometry model with WebGL2 (ARC-05,
//! ARC-18). Reading, inspection and the chart actions run here too. The modules outside `web`
//! are pure and tested natively; `web` holds the browser glue.

pub mod data;
pub mod mesh;
pub mod payload;
pub mod tween;

#[cfg(test)]
#[path = "web/host/contour.rs"]
mod test_contour;
#[cfg(test)]
#[path = "web/host/focus.rs"]
mod test_focus;

#[cfg(test)]
#[path = "web/host/hint_place.rs"]
mod test_hint_place;
#[cfg(test)]
#[path = "web/host/name_target.rs"]
mod test_name_target;
#[cfg(test)]
#[path = "web/host/wedge_readout.rs"]
mod test_wedge_readout;

#[cfg(test)]
#[path = "web/host/readout_place.rs"]
mod test_readout_place;

#[cfg(any(target_arch = "wasm32", test))]
mod readout;

#[cfg(target_arch = "wasm32")]
mod web;
