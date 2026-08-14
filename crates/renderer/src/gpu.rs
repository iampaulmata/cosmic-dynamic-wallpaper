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

use crate::error::RendererError;

/// Create the shared `wgpu::Instance` — needs no surface, so it's created once at
/// daemon startup and reused to create every output's `wgpu::Surface` (`surface.rs`).
pub fn create_instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN | wgpu::Backends::GL, ..Default::default() })
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
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(compatible_surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| RendererError::GpuDeviceUnavailable {
            reason: "no Vulkan or GL adapter found".to_string(),
        })?;

        tracing::info!(
            adapter = ?adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU adapter selected"
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .map_err(|e| RendererError::GpuDeviceUnavailable { reason: e.to_string() })?;

        Ok(Self { adapter, device, queue })
    }
}
