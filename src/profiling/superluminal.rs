//! Superluminal backend — `scope!` → superluminal event
//! (feature `profile-with-superluminal`, windows-only dep).
//!
//! Mirrors upstream `profiling`'s `SuperluminalGuard`.

/// `0xFFFFFFFF` means "use default color" (upstream value).
const DEFAULT_SUPERLUMINAL_COLOR: u32 = 0xFFFF_FFFF;

/// Begin a superluminal event. `tag` becomes the event's attached data.
#[must_use]
pub fn enter(name: &'static str, tag: Option<&'static str>) -> SuperluminalGuard {
    match tag {
        Some(data) => {
            superluminal_perf::begin_event_with_data(name, data, DEFAULT_SUPERLUMINAL_COLOR)
        }
        None => superluminal_perf::begin_event(name),
    }
    SuperluminalGuard
}

/// Construct a no-op `SuperluminalGuard` (does nothing on drop).
#[must_use]
pub const fn dummy() -> SuperluminalGuard {
    SuperluminalGuard
}

/// Guard — ends the superluminal event on drop.
pub struct SuperluminalGuard;

impl Drop for SuperluminalGuard {
    fn drop(&mut self) {
        superluminal_perf::end_event();
    }
}
