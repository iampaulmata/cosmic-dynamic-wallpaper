//! [`PackImage`], [`WallpaperPack`], and [`ValidatedPack`] — structural pack validation
//! (FR-001, FR-006, FR-006a; data-model.md "WallpaperPack (validated form)").

use crate::anchor::TimeAnchor;
use crate::error::PackError;

/// The maximum number of anchors a pack may contain (FR-001).
pub const MAX_ANCHORS: usize = 64;

/// An opaque, `Eq`-comparable image identifier.
///
/// Spec 2 (pack loading) owns the real identifier shape (e.g. a path-derived id); this
/// crate only needs something it can compare and hand back in query results
/// (data-model.md `PackImage.id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(String);

impl ImageId {
    /// Wrap an identifier string as an opaque [`ImageId`].
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ImageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ImageId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for ImageId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

use core::fmt;

/// A single image in a pack plus the anchor that schedules it.
#[derive(Debug, Clone, PartialEq)]
pub struct PackImage {
    /// Unique (within the pack) identifier for this image.
    pub id: ImageId,
    /// When this image becomes active.
    pub anchor: TimeAnchor,
}

impl PackImage {
    /// Construct a new [`PackImage`].
    pub fn new(id: impl Into<ImageId>, anchor: TimeAnchor) -> Self {
        Self { id: id.into(), anchor }
    }
}

/// Which [`TimeAnchor`] variant a validated pack uses (FR-6: a pack is uniformly one or
/// the other, never mixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorKind {
    /// Every anchor in the pack is [`TimeAnchor::Solar`].
    Solar,
    /// Every anchor in the pack is [`TimeAnchor::Clock`].
    Clock,
}

/// An unvalidated collection of images, as supplied by a caller (e.g. spec 2's pack
/// loader). [`WallpaperPack::validate`] is the only way to obtain a [`ValidatedPack`].
#[derive(Debug, Clone, Default)]
pub struct WallpaperPack;

impl WallpaperPack {
    /// Validate a raw image list into a [`ValidatedPack`] (data-model.md validation
    /// rules 1–3 and 5; rule 4, duplicate-instant detection, is applied separately per
    /// anchor kind — see `solar.rs`/`query.rs` — since solar packs can only be checked
    /// against a resolved date).
    ///
    /// Pure, synchronous, never panics (constitution Principle VIII).
    pub fn validate(images: Vec<PackImage>) -> Result<ValidatedPack, PackError> {
        if images.is_empty() {
            return Err(PackError::Empty);
        }
        if images.len() > MAX_ANCHORS {
            return Err(PackError::TooManyAnchors { count: images.len() });
        }

        let anchor_kind = if images[0].anchor.is_solar() {
            AnchorKind::Solar
        } else {
            AnchorKind::Clock
        };
        let uniform = images.iter().all(|img| match anchor_kind {
            AnchorKind::Solar => img.anchor.is_solar(),
            AnchorKind::Clock => img.anchor.is_clock(),
        });
        if !uniform {
            return Err(PackError::MixedAnchorTypes);
        }

        let mut seen_ids = std::collections::HashSet::with_capacity(images.len());
        for img in &images {
            if !seen_ids.insert(&img.id) {
                return Err(PackError::DuplicateImageId);
            }
        }

        // Clock-pack duplicate-instant check (FR-006a, data-model.md rule 4): unlike
        // solar packs, a `NaiveTime` doesn't depend on a resolved date, so this can be
        // — and is — a static, one-time check right here rather than a separate
        // per-date method (contrast `check_solar_duplicate_instant` below).
        if anchor_kind == AnchorKind::Clock {
            let mut seen_times = std::collections::HashSet::with_capacity(images.len());
            for img in &images {
                if let TimeAnchor::Clock(time) = img.anchor {
                    if !seen_times.insert(time) {
                        return Err(PackError::DuplicateInstant);
                    }
                }
            }
        }

        Ok(ValidatedPack { images, anchor_kind })
    }
}

/// A structurally valid pack, ready to be queried (data-model.md "WallpaperPack
/// (validated form)"; contracts/schedule-engine-api.md's `ValidatedPack`).
///
/// Query/resolution methods (`query`, `next_transition_after`) are added to this type in
/// `query.rs` as later user stories land (US1–US3).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedPack {
    pub(crate) images: Vec<PackImage>,
    pub(crate) anchor_kind: AnchorKind,
}

impl ValidatedPack {
    /// The images in this pack, in the order they were supplied.
    pub fn images(&self) -> &[PackImage] {
        &self.images
    }

    /// Whether this pack is solar- or clock-anchored (FR-6).
    pub fn anchor_kind(&self) -> AnchorKind {
        self.anchor_kind
    }

    /// `true` if this is the degenerate single-image/static-mode pack (Edge Cases,
    /// FR-3): always active, no transition, ever.
    pub fn is_static(&self) -> bool {
        self.images.len() == 1
    }

    /// Check a solar-anchored pack for two-or-more anchors resolving to the exact same
    /// instant on `date` (FR-006a, data-model.md validation rule 4).
    ///
    /// Unlike the structural checks in [`WallpaperPack::validate`], this can't run once
    /// at pack-load time for solar packs — solar event instants shift day to day, so a
    /// collision on one date doesn't imply a collision on another (data-model.md rule 4's
    /// own note). Callers (pack loading/registration in spec 2, or a periodic re-check as
    /// the daemon crosses into a new date) are expected to call this for the date(s) that
    /// matter to them; [`ValidatedPack::query`]/`next_transition_after` intentionally stay
    /// infallible (contracts/schedule-engine-api.md) and don't call it internally.
    ///
    /// A no-op returning `Ok(())` for clock-anchored packs — their duplicate-instant check
    /// is static and already applied once in `validate` (T019).
    pub fn check_solar_duplicate_instant(
        &self,
        location: &crate::Location,
        date: chrono::NaiveDate,
    ) -> Result<(), PackError> {
        if self.anchor_kind != AnchorKind::Solar {
            return Ok(());
        }
        let mut seen = Vec::with_capacity(self.images.len());
        for img in &self.images {
            let TimeAnchor::Solar { event, offset } = img.anchor else {
                continue;
            };
            if let Some(instant) = crate::solar::resolve_solar_anchor(location, date, event, offset) {
                if seen.contains(&instant) {
                    return Err(PackError::DuplicateInstant);
                }
                seen.push(instant);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::SolarEventKind;
    use chrono::NaiveTime;

    fn solar_image(id: &str) -> PackImage {
        PackImage::new(id, TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None })
    }

    fn clock_image(id: &str, hh: u32, mm: u32) -> PackImage {
        #[allow(clippy::unwrap_used)]
        PackImage::new(id, TimeAnchor::Clock(NaiveTime::from_hms_opt(hh, mm, 0).unwrap()))
    }

    #[test]
    fn rejects_empty_pack() {
        assert_eq!(WallpaperPack::validate(vec![]), Err(PackError::Empty));
    }

    #[test]
    fn rejects_too_many_anchors() {
        let images: Vec<_> = (0..65).map(|i| solar_image(&i.to_string())).collect();
        assert_eq!(
            WallpaperPack::validate(images),
            Err(PackError::TooManyAnchors { count: 65 })
        );
    }

    #[test]
    fn accepts_exactly_max_anchors() {
        let images: Vec<_> = (0..64).map(|i| solar_image(&i.to_string())).collect();
        assert!(WallpaperPack::validate(images).is_ok());
    }

    #[test]
    fn rejects_mixed_anchor_types() {
        let images = vec![solar_image("a"), clock_image("b", 8, 0)];
        assert_eq!(WallpaperPack::validate(images), Err(PackError::MixedAnchorTypes));
    }

    #[test]
    fn rejects_duplicate_image_id() {
        // Same anchor kind on both images, so this isolates the duplicate-id path from
        // the mixed-anchor-type check above.
        assert_eq!(
            WallpaperPack::validate(vec![solar_image("dup"), solar_image("dup")]),
            Err(PackError::DuplicateImageId)
        );
    }

    #[test]
    fn accepts_valid_solar_pack() {
        let images = vec![solar_image("a"), solar_image("b")];
        let pack = WallpaperPack::validate(images).expect("valid pack");
        assert_eq!(pack.anchor_kind(), AnchorKind::Solar);
        assert!(!pack.is_static());
    }

    #[test]
    fn accepts_valid_clock_pack() {
        let images = vec![clock_image("a", 6, 0), clock_image("b", 18, 0)];
        let pack = WallpaperPack::validate(images).expect("valid pack");
        assert_eq!(pack.anchor_kind(), AnchorKind::Clock);
    }

    #[test]
    fn single_image_pack_is_static() {
        let pack = WallpaperPack::validate(vec![solar_image("only")]).expect("valid pack");
        assert!(pack.is_static());
    }

    #[test]
    fn image_id_wraps_and_displays_a_string() {
        let id = ImageId::new("sunset-01");
        assert_eq!(id.as_str(), "sunset-01");
        assert_eq!(id.to_string(), "sunset-01");
    }

    #[test]
    fn check_solar_duplicate_instant_is_a_noop_for_clock_packs() {
        let pack = WallpaperPack::validate(vec![clock_image("a", 6, 0)]).expect("valid pack");
        let loc = crate::Location::new(43.6532, -79.3832).unwrap();
        let date = chrono::NaiveDate::from_ymd_opt(2016, 1, 1).unwrap();
        assert_eq!(pack.check_solar_duplicate_instant(&loc, date), Ok(()));
    }

    #[test]
    fn check_solar_duplicate_instant_is_ok_when_no_collision() {
        let pack = WallpaperPack::validate(vec![solar_image("a"), solar_image_sunset("b")])
            .expect("valid pack");
        let loc = crate::Location::new(43.6532, -79.3832).unwrap();
        let date = chrono::NaiveDate::from_ymd_opt(2016, 1, 1).unwrap();
        assert_eq!(pack.check_solar_duplicate_instant(&loc, date), Ok(()));
    }

    /// A solar-anchored image tied to a different event than `solar_image` (which is
    /// always `Sunrise`), so the two never collide.
    fn solar_image_sunset(id: &str) -> PackImage {
        PackImage::new(id, TimeAnchor::Solar { event: SolarEventKind::Sunset, offset: None })
    }
}
