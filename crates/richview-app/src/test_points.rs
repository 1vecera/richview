//! Named places where one of this crate's tests can hold a request and let it go, so a test
//! decides the order of two requests instead of sleeping. Compiled into the crate's own tests
//! only; a point nobody holds costs one map lookup and does nothing.
// The tests that hold points render reports, so they exist only with the runtime compiled in.
#![cfg_attr(not(feature = "embedded-runtime"), allow(dead_code))]
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Point {
    /// (reached, released)
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

fn points() -> &'static Mutex<HashMap<String, Arc<Point>>> {
    static POINTS: OnceLock<Mutex<HashMap<String, Arc<Point>>>> = OnceLock::new();
    POINTS.get_or_init(Mutex::default)
}

/// A held point: whoever reaches it waits until `release` (or until this is dropped).
pub(crate) struct Held {
    id: String,
    point: Arc<Point>,
}

/// Hold `name` for the report `key`; tests use a key of their own, so they do not meet.
pub(crate) fn hold(name: &str, key: &str) -> Held {
    let id = format!("{name} {key}");
    let point = Arc::<Point>::default();
    points().lock().unwrap().insert(id.clone(), point.clone());
    Held { id, point }
}

impl Held {
    /// Block until a request stands at the point.
    pub(crate) fn reached(&self) {
        let state = self.point.state.lock().unwrap();
        let (state, timeout) = self
            .point
            .changed
            .wait_timeout_while(state, PATIENCE, |(reached, _)| !*reached)
            .unwrap();
        assert!(!timeout.timed_out(), "nothing reached {}", self.id);
        drop(state);
    }

    pub(crate) fn release(&self) {
        self.point.state.lock().unwrap().1 = true;
        self.point.changed.notify_all();
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.release();
        points().lock().unwrap().remove(&self.id);
    }
}

/// Called by the code under test: waits here while a test holds the point.
pub(crate) fn reach(name: &str, key: &str) {
    // The first to arrive is the one held; whoever comes later, or again, passes.
    let point = points().lock().unwrap().remove(&format!("{name} {key}"));
    let Some(point) = point else {
        return;
    };
    let mut state = point.state.lock().unwrap();
    state.0 = true;
    point.changed.notify_all();
    let (_state, timeout) = point
        .changed
        .wait_timeout_while(state, PATIENCE, |(_, released)| !*released)
        .unwrap();
    assert!(!timeout.timed_out(), "{name} {key} was never released");
}
