//! [`RendererError`] — every way this crate's logic can fail. Never panics on a
//! per-output or config condition; a failure is contained to the one affected output,
//! never the whole daemon.

use std::fmt;
use std::path::PathBuf;

use crate::output::OutputId;

/// Every way this crate's logic can fail.
#[derive(Debug)]
pub enum RendererError {
    /// Layer-shell or GPU surface setup failed for one output.
    SurfaceCreationFailed {
        /// The output whose surface setup failed.
        output: OutputId,
        /// Why it failed.
        reason: String,
    },
    /// No working `wgpu` backend found at startup.
    GpuDeviceUnavailable {
        /// Why no backend was found.
        reason: String,
    },
    /// Full pixel decode/upload failed for an image the pack loader already
    /// header-validated.
    TextureUploadFailed {
        /// The image file that failed to decode/upload.
        path: PathBuf,
        /// Why it failed.
        reason: String,
    },
    /// An image's declared (header-only, undecoded) dimensions exceed the GPU's
    /// `max_texture_dimension_2d` or the crate's own decoded-byte ceiling — rejected
    /// before the expensive full decode, distinct from the pack loader's cheap
    /// header-only readability check.
    TextureTooLarge {
        /// The oversized image file.
        path: PathBuf,
        /// Its declared width in pixels.
        width: u32,
        /// Its declared height in pixels.
        height: u32,
    },
    /// A `cosmic-config` read/write failed (`RendererConfig` or `LocationConfigEntry`).
    ConfigError {
        /// The underlying storage error's message.
        reason: String,
    },
    /// The compositor doesn't support a required protocol (e.g. no
    /// `wlr-layer-shell-unstable-v1`) — a startup-time, whole-daemon condition.
    OutputProtocolError {
        /// Which protocol/capability was missing.
        reason: String,
    },
    /// A D-Bus `QueryOutput`/`Reevaluate` call named an output this daemon doesn't
    /// currently manage.
    OutputNotManaged {
        /// The unmanaged output that was named.
        id: OutputId,
    },
    /// A solar-anchored pack is assigned to an output but no manual location has been
    /// provided. Added as its own variant rather than force-fitting an ill-matched
    /// existing one (all others are Wayland/GPU/config/D-Bus specific) — the
    /// alternative, silently calling the scheduling engine's `ValidatedPack::query`
    /// with `location: None` on a solar pack, would *panic* (a caller contract
    /// violation, not a graceful degrade) and crash the whole daemon.
    /// `scheduler_bridge.rs` checks for this *before* ever calling `query`.
    LocationRequired {
        /// The output whose assigned pack needs a location that isn't set.
        output: OutputId,
    },
    /// A GPU adapter or device request took longer than
    /// [`crate::gpu::GPU_REQUEST_TIMEOUT`] — without this, a hung/misbehaving driver
    /// could stall `wallpaperd`'s entire startup indefinitely.
    GpuRequestTimedOut,
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
            RendererError::TextureTooLarge { path, width, height } => {
                write!(f, "image {} is {width}x{height}, too large to upload (dimension or decoded-size limit)", path.display())
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
            RendererError::GpuRequestTimedOut => {
                write!(f, "GPU adapter/device request timed out after {:?}", crate::gpu::GPU_REQUEST_TIMEOUT)
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
        assert!(RendererError::GpuRequestTimedOut.to_string().contains("timed out"));
    }
}
