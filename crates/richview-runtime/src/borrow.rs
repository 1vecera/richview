//! Releasing a view's borrow before running code that can re-enter it. The browser runs event
//! handlers inside DOM calls: removing a focused element fires its `blur` at once, and a legend
//! entry's `blur` handler borrows the view to end the series' emphasis. A part taken out of the
//! view with `if let Some(old) = view.borrow_mut().legend.take() { old.remove() }` keeps the
//! borrow for the whole block, so the handler's borrow panicked, which traps the Wasm module and
//! leaves every later call on the page trapping too. The helper takes the part, drops the guard
//! and only then hands the part on.

use std::cell::RefCell;

/// Take the part `part` names out of `state`, release the borrow, then give the part to `then`.
pub(crate) fn take_then<S, T>(
    state: &RefCell<S>,
    part: impl FnOnce(&mut S) -> &mut Option<T>,
    then: impl FnOnce(T),
) {
    let taken = part(&mut state.borrow_mut()).take();
    if let Some(taken) = taken {
        then(taken);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a rebuilt legend touches: the part it replaces, and how often a `blur` handler ran.
    struct State {
        legend: Option<&'static str>,
        blurs: u32,
    }

    /// The `blur` handler of a legend entry: it borrows the view to end the series' emphasis.
    fn blur(state: &RefCell<State>) {
        state.borrow_mut().blurs += 1;
    }

    #[test]
    fn the_view_is_free_while_the_old_part_is_removed() {
        // Removing the focused entry fires its blur, which borrows the view.
        let state = RefCell::new(State {
            legend: Some("legend"),
            blurs: 0,
        });
        let mut removed = Vec::new();
        take_then(
            &state,
            |s| &mut s.legend,
            |old| {
                blur(&state);
                removed.push(old);
            },
        );
        assert_eq!(removed, ["legend"]);
        assert_eq!(state.borrow().blurs, 1);
        assert!(
            state.borrow().legend.is_none(),
            "the view no longer holds it"
        );
        // A view with no legend yet removes nothing.
        take_then(&state, |s| &mut s.legend, |_| blur(&state));
        assert_eq!(state.borrow().blurs, 1);
    }

    /// The shape `build_legend` had: the guard of `borrow_mut()` lives to the end of the block,
    /// so the handler's own borrow panics, as it did in the browser (a Wasm trap).
    #[test]
    #[should_panic]
    fn removing_while_the_view_is_borrowed_panics_in_the_blur_handler() {
        let state = RefCell::new(State {
            legend: Some("legend"),
            blurs: 0,
        });
        if let Some(_old) = state.borrow_mut().legend.take() {
            blur(&state);
        }
    }
}
