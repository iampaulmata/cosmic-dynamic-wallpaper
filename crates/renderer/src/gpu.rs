//! `wgpu` instance/device/adapter setup (T011, research.md R3). Automatic backend
//! selection (Vulkan preferred, GL fallback) via `wgpu::Backends::PRIMARY |
//! wgpu::Backends::GL`.
//!
//! **Verified against a real system** (2026-08-13): a throwaway probe confirmed this
//! exact bridge — SCTK layer-shell surface → raw-window-handle → `wgpu::Surface` →
//! device/adapter → render → present — works end to end against a live `cosmic-comp`
//! session, producing a real `wgpu` adapter (`Intel(R) HD Graphics 630`, Vulkan
//! backend) and successfully presenting frames the compositor accepted. See
//! `README.md`'s "Building on a real system" section for the one system-dependency
//! gotcha found along the way (`libxkbcommon-dev`/`xkbcommon.pc` not present by
//! default on this dev image, worked around with a `pkg-config` shim — a real
//! deployment target would normally already have it as part of a full dev toolchain).

use std::time::Duration;

use crate::error::RendererError;

/// Create the shared `wgpu::Instance` — needs no surface, so it's created once at
/// daemon startup and reused to create every output's `wgpu::Surface` (`surface.rs`).
pub fn create_instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN | wgpu::Backends::GL, ..Default::default() })
}

/// Ceiling on how long a GPU adapter/device request may take (spec 011 US7 FR-033,
/// research.md R28) — a hung/misbehaving driver previously could stall `wallpaperd`'s
/// entire startup indefinitely, violating constitution Principle VIII's "no unbounded
/// wait on an external resource" posture (the same one every other external touchpoint
/// in this daemon — the portal, STUN, D-Bus queueing — already respects). 20s is
/// generous for a real request (this module's own doc records a live request
/// completing well under a second) while still bounding the worst case.
pub const GPU_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Race `future` against `timeout`, returning [`RendererError::GpuRequestTimedOut`] if
/// it elapses first — same `futures_lite::future::or` primitive `portal_location::
/// start_session` already uses for its own resolution timeout. Takes `timeout` as a
/// parameter (rather than hard-coding [`GPU_REQUEST_TIMEOUT`]) purely so this module's
/// own tests can exercise the actual timeout path in milliseconds, not 20 real seconds
/// — [`with_gpu_timeout`] is the production call site that always passes the real
/// constant.
async fn with_timeout<T>(future: impl std::future::Future<Output = Result<T, RendererError>>, timeout: Duration) -> Result<T, RendererError> {
    let timeout_fut = async {
        async_io::Timer::after(timeout).await;
        Err(RendererError::GpuRequestTimedOut)
    };
    futures_lite::future::or(future, timeout_fut).await
}

/// Race `future` against [`GPU_REQUEST_TIMEOUT`] — the production entry point; see
/// [`with_timeout`].
async fn with_gpu_timeout<T>(future: impl std::future::Future<Output = Result<T, RendererError>>) -> Result<T, RendererError> {
    with_timeout(future, GPU_REQUEST_TIMEOUT).await
}

/// The adapter/device/queue, shared across every managed output (constitution
/// Principle VII: outputs don't share *render state*, but the underlying device/queue
/// is one instance per daemon, same as any other Wayland/GPU client).
pub struct GpuContext {
    /// The selected physical/logical GPU adapter (e.g. `Intel(R) HD Graphics 630`).
    pub adapter: wgpu::Adapter,
    /// The logical device used to create every GPU resource (textures, pipelines,
    /// buffers).
    pub device: wgpu::Device,
    /// The command queue every draw/upload is submitted through.
    pub queue: wgpu::Queue,
}

impl GpuContext {
    /// Request an adapter/device compatible with `compatible_surface` (the first
    /// output's surface — `wgpu` adapter compatibility is generally shared across
    /// outputs on one GPU, so a single adapter/device serves every managed output).
    pub fn new(instance: &wgpu::Instance, compatible_surface: &wgpu::Surface<'_>) -> Result<Self, RendererError> {
        let adapter = pollster::block_on(with_gpu_timeout(async {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(compatible_surface),
                    force_fallback_adapter: false,
                })
                .await
                .ok_or_else(|| RendererError::GpuDeviceUnavailable { reason: "no Vulkan or GL adapter found".to_string() })
        }))?;

        tracing::info!(
            adapter = ?adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU adapter selected"
        );

        let (device, queue) = pollster::block_on(with_gpu_timeout(async {
            adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .map_err(|e| RendererError::GpuDeviceUnavailable { reason: e.to_string() })
        }))?;

        Ok(Self { adapter, device, queue })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec 011 US7 FR-033 (research.md R28) — the audit's own reproduction: a
    /// GPU request that never resolves (a hung/misbehaving driver) previously stalled
    /// `wallpaperd`'s entire startup forever. A future that never completes
    /// (`std::future::pending`) proves the pre-fix code would have hung here too; the
    /// timeout now bounds it deterministically instead.
    #[test]
    fn with_timeout_times_out_when_the_future_never_resolves() {
        let result: Result<(), RendererError> = pollster::block_on(with_timeout(std::future::pending(), Duration::from_millis(10)));
        assert!(matches!(result, Err(RendererError::GpuRequestTimedOut)));
    }

    /// A future that resolves well before the deadline is unaffected — the timeout
    /// only ever changes behavior in the hung case above.
    #[test]
    fn with_timeout_returns_the_future_result_when_it_resolves_first() {
        let result = pollster::block_on(with_timeout(async { Ok::<_, RendererError>(42) }, Duration::from_secs(5)));
        assert_eq!(result.unwrap(), 42);
    }
}
