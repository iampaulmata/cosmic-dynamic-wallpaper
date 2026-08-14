//! [`CrossfadeTransition`] and its progress computation (data-model.md
//! `CrossfadeTransition`, FR-001, FR-002, FR-004, FR-011) — pure math, no GPU/Wayland
//! involved. The actual two-texture WGSL blend pipeline and frame-callback draw loop
//! that *consume* this progress value are the unimplemented part (see `README.md`).

use std::time::{Duration, Instant};

/// The active-transition state for one output (data-model.md `CrossfadeTransition`).
/// **Scope note**: `outgoing_texture`/`incoming_texture` in the full data model are GPU
/// texture handles; here they're the image identifiers alone (`schedule_engine::ImageId`)
/// — everything the pure logic needs to know *which* images are involved, without
/// depending on `wgpu`.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfadeTransition {
    /// The image fading out.
    pub outgoing: schedule_engine::ImageId,
    /// The image fading in.
    pub incoming: schedule_engine::ImageId,
    /// Local to this transition; not persisted. `Instant` (monotonic), not
    /// `DateTime<Local>`, since a system clock adjustment mid-transition must not
    /// perturb an already-running animation.
    pub started_at: Instant,
    /// Fixed 45s default (FR-002), configurable.
    pub duration: Duration,
}

impl CrossfadeTransition {
    /// Recompute progress from `started_at`/`duration` at `now` — called once per
    /// frame-callback tick in the real draw loop; deterministic given the same `now`
    /// (FR-004, monotonic non-decreasing as `now` advances, always in `[0.0, 1.0]`).
    ///
    /// A zero-duration transition is immediately complete (`1.0`) rather than
    /// dividing by zero.
    pub fn progress_at(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        (elapsed.as_secs_f64() / self.duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// `true` once `progress_at(now)` has reached `1.0` — the draw loop's cue to
    /// unsubscribe from frame callbacks and return to `IdleWaitState` (FR-004).
    pub fn is_complete_at(&self, now: Instant) -> bool {
        self.progress_at(now) >= 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schedule_engine::ImageId;

    fn transition(duration: Duration) -> CrossfadeTransition {
        CrossfadeTransition {
            outgoing: ImageId::new("a.jpg"),
            incoming: ImageId::new("b.jpg"),
            started_at: Instant::now(),
            duration,
        }
    }

    #[test]
    fn progress_starts_at_zero_and_ends_at_one() {
        let t = transition(Duration::from_secs(45));
        assert_eq!(t.progress_at(t.started_at), 0.0);
        assert_eq!(t.progress_at(t.started_at + Duration::from_secs(45)), 1.0);
        assert!(t.is_complete_at(t.started_at + Duration::from_secs(45)));
    }

    #[test]
    fn progress_is_monotonic_and_clamped() {
        let t = transition(Duration::from_secs(10));
        let p1 = t.progress_at(t.started_at + Duration::from_secs(3));
        let p2 = t.progress_at(t.started_at + Duration::from_secs(6));
        assert!(p1 < p2);
        assert!((0.0..=1.0).contains(&p1));

        // Beyond the duration, progress clamps at 1.0 rather than overshooting.
        let overshoot = t.progress_at(t.started_at + Duration::from_secs(999));
        assert_eq!(overshoot, 1.0);
    }

    #[test]
    fn zero_duration_is_immediately_complete() {
        let t = transition(Duration::ZERO);
        assert_eq!(t.progress_at(t.started_at), 1.0);
        assert!(t.is_complete_at(t.started_at));
    }

    /// FR-011: a new transition triggered while one is already mid-flight is simply a
    /// *new* `CrossfadeTransition` value (fresh `started_at`) replacing the old one —
    /// there's no stacking representation possible in this data shape at all, which is
    /// exactly the "cleanly supersede, never stack" requirement.
    #[test]
    fn a_new_transition_value_cleanly_replaces_an_in_flight_one() {
        let first = transition(Duration::from_secs(45));
        let mid_progress = first.progress_at(first.started_at + Duration::from_secs(10));
        assert!(mid_progress > 0.0 && mid_progress < 1.0);

        let second = CrossfadeTransition {
            outgoing: ImageId::new("b.jpg"),
            incoming: ImageId::new("c.jpg"),
            started_at: Instant::now(),
            duration: Duration::from_secs(45),
        };
        assert_eq!(second.progress_at(second.started_at), 0.0);
    }
}
