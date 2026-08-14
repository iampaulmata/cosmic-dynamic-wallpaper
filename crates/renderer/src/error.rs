//! [`RendererError`] (data-model.md `RendererError`) — every way this crate's logic
//! can fail. Never panics on a per-output or config condition (constitution Principle
//! VIII); a failure is contained to the one affected output, never the whole daemon.
//!
//! **Scope note**: this implementation pass covers only the pure logic (assignment
//! resolution, crossfade-progress math, config reading/coalescing, the scheduler
//! bridge) — see `README.md`. Several variants below (`SurfaceCreationFailed`,
//! `GpuDeviceUnavailable`, `TextureUploadFailed`, `OutputProtocolError`) belong to the
//! Wayland/GPU integration this pass doesn't implement; they're defined here anyway,
//! matching data-model.md's full contract, so the type is ready for that code to
//! construct them later without an API change. They are never constructed by anything
//! in this crate today.

use std::fmt;
use std::path::PathBuf;

use crate::output::OutputId;

/// Every way this crate's logic can fail (data-model.md `RendererError`).
#[derive(Debug)]
pub enum RendererError {
    /// Layer-shell or GPU surface setup failed for one output. **Not constructed by
    /// this implementation pass** — belongs to the unimplemented Wayland/GPU surface
    /// code (see module doc).
    SurfaceCreationFailed {
        /// The output whose surface setup failed.
        output: OutputId,
        /// Why it failed.
        reason: String,
    },
    /// No working `wgpu` backend found at startup. **Not constructed by this
    /// implementation pass** — belongs to the unimplemented GPU setup code.
    GpuDeviceUnavailable {
        /// Why no backend was found.
        reason: String,
    },
    /// Full pixel decode/upload failed for an image spec 2 already header-validated.
    /// **Not constructed by this implementation pass** — belongs to the unimplemented
    /// texture-upload code.
    TextureUploadFailed {
        /// The image file that failed to decode/upload.
        path: PathBuf,
        /// Why it failed.
        reason: String,
    },
    /// A `cosmic-config` read/write failed (`RendererConfig` or `LocationConfigEntry`).
    ConfigError {
        /// The underlying storage error's message.
        reason: String,
    },
    /// The compositor doesn't support a required protocol (e.g. no
    /// `wlr-layer-shell-unstable-v1`) — a startup-time, whole-daemon condition. **Not
    /// constructed by this implementation pass** — belongs to the unimplemented
    /// Wayland integration code.
    OutputProtocolError {
        /// Which protocol/capability was missing.
        reason: String,
    },
    /// A D-Bus `QueryOutput`/`Reevaluate` call named an output this daemon doesn't
    /// currently manage (FR-016, Amendment 2026-08-13).
    OutputNotManaged {
        /// The unmanaged output that was named.
        id: OutputId,
    },
    /// A solar-anchored pack is assigned to an output but no manual location has been
    /// provided (FR-015). **Resolved gap**: data-model.md's LocationConfigEntry section says
    /// this "degrades... per `RendererError`'s existing containment posture..., not a
    /// new error variant" — but no existing variant actually describes this condition
    /// (all others are Wayland/GPU/config/D-Bus specific). Added here rather than
    /// force-fitting an ill-matched existing variant, since the alternative — silently
    /// calling spec 1's `ValidatedPack::query` with `location: None` on a solar pack —
    /// would *panic* (a caller contract violation per spec 1's own contract, not a
    /// graceful degrade) and crash the whole daemon, exactly what FR-013 forbids.
    /// `scheduler_bridge.rs` checks for this *before* ever calling `query`.
    LocationRequired {
        /// The output whose assigned pack needs a location that isn't set.
        output: OutputId,
    },
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::SurfaceCreationFailed { output, reason } => {
                write!(f, "failed to create surface for output {output}: {reason}")
            }
            RendererError::GpuDeviceUnavailable { reason } => {
                write!(f, "no working GPU backend available: {reason}")
            }
            RendererError::TextureUploadFailed { path, reason } => {
                write!(f, "failed to upload texture for {}: {reason}", path.display())
            }
            RendererError::ConfigError { reason } => write!(f, "configuration storage error: {reason}"),
            RendererError::OutputProtocolError { reason } => {
                write!(f, "compositor is missing a required protocol: {reason}")
            }
            RendererError::OutputNotManaged { id } => {
                write!(f, "output {id} is not currently managed by this daemon")
            }
            RendererError::LocationRequired { output } => {
                write!(f, "output {output} has a solar-anchored pack assigned but no location is set")
            }
        }
    }
}

impl std::error::Error for RendererError {}

impl From<cosmic_config::Error> for RendererError {
    fn from(e: cosmic_config::Error) -> Self {
        RendererError::ConfigError { reason: e.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_name_the_specific_output_or_reason() {
        let id = OutputId::new("DP-3");
        assert!(RendererError::OutputNotManaged { id: id.clone() }.to_string().contains("DP-3"));
        assert!(RendererError::LocationRequired { output: id }.to_string().contains("DP-3"));
        assert!(RendererError::ConfigError { reason: "disk full".into() }.to_string().contains("disk full"));
    }
}
