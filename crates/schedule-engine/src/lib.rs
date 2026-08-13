//! Pure, deterministic solar/clock scheduling logic for the dynamic wallpaper daemon.
//!
//! Given a validated pack and a query instant, this crate answers "which image is
//! active, and how far through a crossfade are we" — for either a solar-event-anchored
//! pack (with a manually-entered [`Location`]) or a fully location-free clock-anchored
//! pack. No I/O, no rendering, no persistence: see `README.md` for full scope and
//! non-scope, and `contracts/schedule-engine-api.md` for the committed public API shape.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod anchor;
mod error;
mod location;
mod pack;
mod query;
mod solar;

pub use anchor::{SolarEventKind, TimeAnchor};
pub use error::{CoordinateField, LocationError, PackError};
pub use location::Location;
pub use pack::{AnchorKind, ImageId, PackImage, ValidatedPack, WallpaperPack, MAX_ANCHORS};
pub use query::{ScheduleQueryResult, TransitionState};

/// Test-only internal accessors. **Not** part of the committed public contract
/// (contracts/schedule-engine-api.md) — exists solely so `tests/solar_accuracy.rs` can
/// check individual solar event computations directly (SC-002) without constructing a
/// full pack per event. Hidden from generated docs.
#[doc(hidden)]
pub mod testing {
    /// Resolve a single solar event's instant with no offset — thin wrapper over the
    /// crate-private `solar::resolve_solar_anchor` for accuracy testing.
    pub fn solar_event_instant(
        location: &crate::Location,
        date: chrono::NaiveDate,
        event: crate::SolarEventKind,
    ) -> Option<chrono::DateTime<chrono::Local>> {
        crate::solar::resolve_solar_anchor(location, date, event, None)
    }
}
