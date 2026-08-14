//! The five settings pages (contracts/gui-application.md) — each holds its own pure
//! view-state/mapping logic (unit-testable, independent of `libcosmic` rendering) plus
//! a `view()` function building the real widgets.

pub mod assignment;
pub mod crossfade;
pub mod location;
pub mod packs;
pub mod timeline;
