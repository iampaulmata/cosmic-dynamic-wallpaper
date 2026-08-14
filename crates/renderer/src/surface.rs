//! Per-output `wlr-layer-shell-unstable-v1` background surface creation (T013),
//! bridged to `wgpu` (T012), and the `wallpaperd` application state that drives it
//! (T020, T021, T025, T037). This is the module that ties every other pure-logic piece
//! in this crate (`scheduler_bridge`, `crossfade`, `output::resolve_assignment`) to a
//! real, on-screen result.
//!
//! Wayland setup pattern (registry/output/compositor/layer-shell/viewporter state,
//! `delegate_*!`/`delegate_noop!` wiring) follows `cosmic-bg`'s own, the project this
//! daemon replaces — same `smithay-client-toolkit` version, same protocol set. The one
//! deliberate divergence is the render path itself: `cosmic-bg` draws into an SHM
//! buffer on the CPU (and has no crossfade at all); this daemon renders via `wgpu`
//! (constitution Principle III: GPU-accelerated crossfade).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputInfo, OutputState},
    reexports::calloop::{
        self,
        timer::{TimeoutAction, Timer},
    },
    reexports::protocols::wp::viewporter::client::{wp_viewport, wp_viewporter},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
};
use wayland_client::{
    delegate_noop,
    protocol::{wl_output, wl_surface},
    Connection, Proxy, QueueHandle,
};

use pack_loader::{Color, LoadedPack, Registry, ScalingMode};
use schedule_engine::{ImageId, Location};

use crate::config::Coalescer;
use crate::crossfade::{CrossfadePipeline, CrossfadeTransition};
use crate::dbus_types::QueryResponse;
use crate::error::RendererError;
use crate::gpu::GpuContext;
use crate::output::{effective_pack, resolve_assignment, OutputId, RendererConfig};
use crate::scheduler_bridge;
use crate::texture::GpuTexture;

/// Fixed crossfade duration (FR-002 default; not yet exposed as user-configurable —
/// spec.md Assumptions explicitly defers that).
pub const CROSSFADE_DURATION: Duration = Duration::from_secs(45);

/// Per-output render state: the layer surface, its `wgpu` bridge, and everything
/// needed to decide what to draw on its next frame.
struct WallpaperOutput {
    id: OutputId,
    wl_output: wl_output::WlOutput,
    layer: LayerSurface,
    viewport: wp_viewport::WpViewport,
    wgpu_surface: Option<wgpu::Surface<'static>>,
    size: Option<(u32, u32)>,
    loaded_pack: Option<LoadedPack>,
    /// Decoded textures for the current pack, keyed by image id — reused across
    /// frames rather than re-decoding every tick.
    textures: HashMap<ImageId, GpuTexture>,
    active_image: Option<ImageId>,
    transition: Option<CrossfadeTransition>,
    frame_callback_pending: bool,
}

/// The `wallpaperd` application state — one instance drives every managed output plus
/// the shared GPU context and config (T020, T025).
pub struct WallpaperDaemon {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor_state: CompositorState,
    layer_shell: LayerShell,
    viewporter: wp_viewporter::WpViewporter,
    qh: QueueHandle<Self>,
    instance: wgpu::Instance,
    gpu: Option<GpuContext>,
    pipeline: Option<CrossfadePipeline>,
    outputs: Vec<WallpaperOutput>,
    /// Spec 2's registry — currently opened at startup for future use (list/reload
    /// tooling); not yet consulted by the render path itself, which loads a pack
    /// directly from its `PackSource` regardless of registry membership.
    pub pack_registry: Registry,
    /// The current output-assignment config (FR-005–FR-007) — reloaded on request via
    /// [`WallpaperDaemon::reload_all_assignments`].
    pub renderer_config: RendererConfig,
    /// The current manually-configured location for solar-anchored packs (FR-015).
    pub location: Option<Location>,
    /// FR-014's change-coalescing debounce state.
    pub coalescer: Coalescer,
    /// Set to request the daemon's main loop exit cleanly.
    pub exit: bool,
    /// Set once at startup ([`WallpaperDaemon::set_loop_handle`]) — lets
    /// [`WallpaperDaemon::reschedule_idle_timer`] insert/remove the idle-wait timer
    /// from any method, not just the timer's own callback. `LoopHandle` is
    /// `Rc`-backed and cheap to clone; calloop's own docs allow inserting sources
    /// from within a source callback, so storing it on the very `Data` it drives is
    /// sound.
    loop_handle: Option<calloop::LoopHandle<'static, WallpaperDaemon>>,
    /// The currently-registered idle-wait timer's token, if any — removed before a
    /// fresh one is inserted at a recomputed deadline.
    idle_timer_token: Option<calloop::RegistrationToken>,
    /// The D-Bus-visible read/write mirror (T053, FR-016) — shared with
    /// [`crate::dbus_service::DaemonInterface`] via [`Self::dbus_state`]. See
    /// `dbus_service`'s module doc for why this is `Arc<Mutex<_>>` rather than
    /// `Rc<RefCell<_>>` despite the daemon staying single-threaded.
    dbus_state: std::sync::Arc<std::sync::Mutex<crate::dbus_service::DbusState>>,
}

impl WallpaperDaemon {
    /// Bind every required Wayland global and construct the daemon's initial state
    /// (no outputs managed yet — those arrive via [`OutputHandler::new_output`]).
    pub fn new(
        globals: &wayland_client::globals::GlobalList,
        qh: &QueueHandle<Self>,
        pack_registry: Registry,
        renderer_config: RendererConfig,
        location: Option<Location>,
    ) -> Result<Self, RendererError> {
        let compositor_state = CompositorState::bind(globals, qh)
            .map_err(|e| RendererError::OutputProtocolError { reason: format!("wl_compositor: {e}") })?;
        let layer_shell = LayerShell::bind(globals, qh)
            .map_err(|e| RendererError::OutputProtocolError { reason: format!("wlr-layer-shell: {e}") })?;
        let viewporter: wp_viewporter::WpViewporter = globals
            .bind(qh, 1..=1, ())
            .map_err(|e| RendererError::OutputProtocolError { reason: format!("wp_viewporter: {e}") })?;

        Ok(Self {
            registry_state: RegistryState::new(globals),
            output_state: OutputState::new(globals, qh),
            compositor_state,
            layer_shell,
            viewporter,
            qh: qh.clone(),
            instance: crate::gpu::create_instance(),
            gpu: None,
            pipeline: None,
            outputs: Vec::new(),
            pack_registry,
            renderer_config,
            location,
            coalescer: Coalescer::new(),
            exit: false,
            loop_handle: None,
            idle_timer_token: None,
            dbus_state: std::sync::Arc::new(std::sync::Mutex::new(crate::dbus_service::DbusState::default())),
        })
    }

    /// Create a managed background layer surface for `wl_output` (T013, T037/T038).
    fn add_output(&mut self, wl_output: wl_output::WlOutput, info: OutputInfo) {
        let id = OutputId::new(info.name.clone().unwrap_or_else(|| format!("output-{}", info.id)));
        tracing::info!(output = %id, "new output");

        let wl_surface = self.compositor_state.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            wl_surface.clone(),
            Layer::Background,
            Some("dynamic-wallpaper"),
            Some(&wl_output),
        );
        layer.set_anchor(Anchor::all());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();

        let viewport = self.viewporter.get_viewport(&wl_surface, &self.qh, ());

        self.outputs.push(WallpaperOutput {
            id,
            wl_output,
            layer,
            viewport,
            wgpu_surface: None,
            size: None,
            loaded_pack: None,
            textures: HashMap::new(),
            active_image: None,
            transition: None,
            frame_callback_pending: false,
        });
    }

    fn remove_output(&mut self, wl_output: &wl_output::WlOutput) {
        if let Some(pos) = self.outputs.iter().position(|o| &o.wl_output == wl_output) {
            let removed = self.outputs.remove(pos);
            tracing::info!(output = %removed.id, "output removed");
        }
    }

    /// Bridge a newly-configured layer surface's `wl_surface` to a `wgpu::Surface`,
    /// creating the shared `GpuContext`/`CrossfadePipeline` on first use (T011, T012).
    fn ensure_gpu_surface(&mut self, conn: &Connection, index: usize) -> Result<(), RendererError> {
        if self.outputs[index].wgpu_surface.is_some() {
            return Ok(());
        }

        let wl_display = conn.backend().display_ptr();
        let wl_surface_ptr = self.outputs[index].layer.wl_surface().id().as_ptr();
        let raw_display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            std::ptr::NonNull::new(wl_display.cast())
                .ok_or_else(|| RendererError::GpuDeviceUnavailable { reason: "null Wayland display pointer".into() })?,
        ));
        let raw_window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            std::ptr::NonNull::new(wl_surface_ptr.cast())
                .ok_or_else(|| RendererError::GpuDeviceUnavailable { reason: "null Wayland surface pointer".into() })?,
        ));

        let wgpu_surface = unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle { raw_display_handle: raw_display, raw_window_handle: raw_window })
        }
        .map_err(|e| RendererError::SurfaceCreationFailed { output: self.outputs[index].id.clone(), reason: e.to_string() })?;

        if self.gpu.is_none() {
            self.gpu = Some(GpuContext::new(&self.instance, &wgpu_surface)?);
        }

        self.outputs[index].wgpu_surface = Some(wgpu_surface);
        Ok(())
    }

    /// Load (or reload) the pack assigned to output `index` and reset its texture
    /// cache — called on assignment change or first configure (T038).
    fn load_pack_for(&mut self, index: usize) {
        let id = self.outputs[index].id.clone();
        let assignment = resolve_assignment(&id, &self.renderer_config);
        let Some(source) = effective_pack(&assignment, &self.renderer_config) else {
            self.outputs[index].loaded_pack = None;
            self.outputs[index].textures.clear();
            return;
        };

        match pack_loader::load_pack(source.path()) {
            Ok(loaded) => {
                self.outputs[index].textures.clear();
                self.outputs[index].loaded_pack = Some(loaded);
                self.outputs[index].active_image = None;
                self.outputs[index].transition = None;
            }
            Err(e) => {
                tracing::error!(output = %id, error = %e, "failed to load assigned pack — output degrades, holding last-good state");
                // FR-013: contained to this output; `loaded_pack` stays whatever it
                // was (last-known-good), not cleared.
            }
        }
    }

    /// Evaluate the schedule for output `index` right now and update its render
    /// state (T017, T021). Loads any newly-needed textures on demand.
    fn evaluate_output(&mut self, index: usize, at: chrono::DateTime<chrono::Local>) {
        let id = self.outputs[index].id.clone();
        // Cloned once upfront (LoadedPack is cheap-ish and this isn't a hot path —
        // a periodic re-evaluation tick, not per-frame) so every use below is a plain
        // `&pack` with no further `Option`-unwrapping needed (constitution Principle
        // VIII: no `unwrap()`/`expect()` outside tests).
        let Some(pack) = self.outputs[index].loaded_pack.clone() else { return };
        let Some(gpu) = self.gpu.as_ref() else { return };

        let result = scheduler_bridge::evaluate(&id, Some(&pack), self.location.as_ref(), at, chrono::TimeDelta::seconds(CROSSFADE_DURATION.as_secs() as i64));

        match result {
            Ok(Some(result)) => {
                let output = &mut self.outputs[index];
                if let Some(t) = &result.transition {
                    let now = Instant::now();
                    let already_this_pair =
                        output.transition.as_ref().is_some_and(|existing| existing.outgoing == t.outgoing && existing.incoming == t.incoming);
                    if !already_this_pair {
                        // A fresh transition (FR-011: supersedes cleanly — a new value
                        // simply replaces the old one, see crossfade.rs's own doc).
                        let started_at = now - Duration::from_secs_f64(t.progress * CROSSFADE_DURATION.as_secs_f64());
                        output.transition =
                            Some(CrossfadeTransition { outgoing: t.outgoing.clone(), incoming: t.incoming.clone(), started_at, duration: CROSSFADE_DURATION });
                    }
                    Self::ensure_texture(gpu, &mut output.textures, &pack, &t.outgoing);
                    Self::ensure_texture(gpu, &mut output.textures, &pack, &t.incoming);
                } else {
                    output.transition = None;
                    output.active_image = Some(result.active_before.clone());
                    Self::ensure_texture(gpu, &mut output.textures, &pack, &result.active_before);
                }
            }
            Ok(None) => {
                self.outputs[index].transition = None;
                self.outputs[index].active_image = None;
            }
            Err(e) => {
                tracing::warn!(output = %id, error = %e, "schedule evaluation degraded this output");
                self.outputs[index].transition = None;
            }
        }
    }

    fn ensure_texture(gpu: &GpuContext, cache: &mut HashMap<ImageId, GpuTexture>, pack: &LoadedPack, id: &ImageId) {
        if cache.contains_key(id) {
            return;
        }
        if let Some(path) = pack.image_paths.get(id) {
            match GpuTexture::load(&gpu.device, &gpu.queue, path) {
                Ok(tex) => {
                    cache.insert(id.clone(), tex);
                }
                Err(e) => tracing::error!(image = %id, error = %e, "texture upload failed"),
            }
        }
    }

    /// The scaling mode + fallback color to render image `id` with on output `index`
    /// (FR-005): per-image override if the loaded pack has one, else the pack's
    /// default. Falls back to `Fill`/opaque-black if no pack is loaded at all — should
    /// never actually trigger (an `outgoing`/`incoming` id only ever comes from a
    /// loaded pack's own schedule evaluation), but avoids a wrong-looking crash if a
    /// future refactor ever breaks that invariant (constitution Principle VIII).
    fn image_scaling_for(&self, index: usize, id: &ImageId) -> crate::crossfade::ImageScaling {
        match self.outputs[index].loaded_pack.as_ref() {
            Some(pack) => {
                crate::crossfade::ImageScaling { mode: pack.image_scaling.get(id).copied().unwrap_or(pack.default_scaling), fallback_color: pack.fallback_color }
            }
            None => crate::crossfade::ImageScaling { mode: ScalingMode::Fill, fallback_color: Color { r: 0, g: 0, b: 0, a: 255 } },
        }
    }

    /// Draw output `index`'s current state (static image or in-progress crossfade) and
    /// present it (T016, T017, T018).
    fn draw(&mut self, index: usize) {
        let Some(size) = self.outputs[index].size else { return };
        let Some(gpu) = self.gpu.as_ref() else { return };
        let Some(pipeline) = self.pipeline.as_ref() else { return };
        let Some(wgpu_surface) = self.outputs[index].wgpu_surface.as_ref() else { return };

        let now = Instant::now();
        let (outgoing_id, incoming_id, progress) = if let Some(t) = &self.outputs[index].transition {
            (t.outgoing.clone(), t.incoming.clone(), t.progress_at(now) as f32)
        } else if let Some(id) = &self.outputs[index].active_image {
            (id.clone(), id.clone(), 1.0)
        } else {
            return; // Unassigned or not yet evaluated — nothing to draw yet.
        };

        let (Some(outgoing), Some(incoming)) =
            (self.outputs[index].textures.get(&outgoing_id), self.outputs[index].textures.get(&incoming_id))
        else {
            return; // Texture still loading/failed — hold whatever was last presented.
        };

        let frame = match wgpu_surface.get_current_texture() {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(output = %self.outputs[index].id, error = %e, "get_current_texture failed, skipping frame");
                return;
            }
        };
        let outgoing_scaling = self.image_scaling_for(index, &outgoing_id);
        let incoming_scaling = self.image_scaling_for(index, &incoming_id);
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        pipeline.render(&gpu.device, &gpu.queue, &view, outgoing, outgoing_scaling, incoming, incoming_scaling, progress, size);

        let wl_surface = self.outputs[index].layer.wl_surface().clone();
        wl_surface.damage_buffer(0, 0, size.0 as i32, size.1 as i32);
        self.outputs[index].viewport.set_destination(size.0 as i32, size.1 as i32);

        let still_animating = self.outputs[index].transition.as_ref().is_some_and(|t| !t.is_complete_at(now));
        if still_animating && !self.outputs[index].frame_callback_pending {
            wl_surface.frame(&self.qh, wl_surface.clone());
            self.outputs[index].frame_callback_pending = true;
        }
        if !still_animating {
            self.outputs[index].transition = None;
        }

        wl_surface.commit();
        frame.present();
    }

    /// Re-evaluate and draw every currently-managed output right now — the entry
    /// point a `calloop` timer (per-output idle-wait wake, T021) or a config-change
    /// coalescer deadline (T029/T033) calls into.
    pub fn evaluate_and_draw_all(&mut self) {
        let at = chrono::Local::now();
        for index in 0..self.outputs.len() {
            self.evaluate_output(index, at);
            self.draw(index);
        }
    }

    /// Re-evaluate and draw one named output, if currently managed (backs
    /// `wallpaperctl reevaluate --output`, T053).
    pub fn evaluate_and_draw(&mut self, id: &OutputId) -> Result<(), RendererError> {
        let index = self.outputs.iter().position(|o| &o.id == id).ok_or_else(|| RendererError::OutputNotManaged { id: id.clone() })?;
        self.evaluate_output(index, chrono::Local::now());
        self.draw(index);
        Ok(())
    }

    /// Every currently-managed output's id — backs `list outputs`/`QueryAll` (T028).
    pub fn output_ids(&self) -> Vec<OutputId> {
        self.outputs.iter().map(|o| o.id.clone()).collect()
    }

    /// A clone of the shared `Arc` backing the D-Bus-visible state mirror — handed to
    /// [`crate::dbus_service::DaemonInterface`] once at startup (T054).
    pub fn dbus_state(&self) -> std::sync::Arc<std::sync::Mutex<crate::dbus_service::DbusState>> {
        self.dbus_state.clone()
    }

    /// The pure "what would we answer right now" for one output (T052/T053) — no draw,
    /// no GPU touched at all, just spec 1's schedule math over an already-loaded pack.
    /// Backs `QueryOutput`/`QueryAll` (FR-016).
    pub fn query_output(&self, id: &OutputId) -> Result<QueryResponse, RendererError> {
        let index = self.outputs.iter().position(|o| &o.id == id).ok_or_else(|| RendererError::OutputNotManaged { id: id.clone() })?;
        let pack = self.outputs[index].loaded_pack.as_ref();
        match scheduler_bridge::evaluate(id, pack, self.location.as_ref(), chrono::Local::now(), chrono::TimeDelta::seconds(CROSSFADE_DURATION.as_secs() as i64)) {
            Ok(Some(result)) => Ok(QueryResponse::from_schedule_result(id.clone(), &result)),
            Ok(None) => Ok(QueryResponse::unassigned(id.clone())),
            // A solar-anchored pack with no location configured yet: genuinely
            // assigned (per the field's literal meaning), just not yet resolvable.
            // Reported the same shape as a static/degenerate pack (no wire-incompatible
            // fourth state) rather than surfaced as a D-Bus error — matches FR-013's
            // "degrade, don't error" containment posture.
            Err(RendererError::LocationRequired { .. }) => {
                Ok(QueryResponse { output: id.clone(), assigned: true, active_image: String::new(), next_transition_at: None })
            }
            Err(e) => Err(e),
        }
    }

    /// Every currently-managed output's [`QueryResponse`] — backs `QueryAll` (FR-016).
    /// Silently skips an output `query_output` can't resolve for reasons other than
    /// the two handled above (there currently are none, but this stays defensive
    /// rather than propagating a single output's failure into a whole-request error).
    pub fn query_all(&self) -> Vec<QueryResponse> {
        self.output_ids().iter().filter_map(|id| self.query_output(id).ok()).collect()
    }

    /// Refresh the D-Bus-visible snapshot from current state — cheap (pure schedule
    /// math over already-loaded packs, no I/O), safe to call unconditionally after any
    /// evaluation.
    pub fn refresh_dbus_snapshot(&mut self) {
        let responses = self.query_all();
        let known = self.output_ids();
        let mut state = self.dbus_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.refresh(responses, known);
    }

    /// Drain every `Reevaluate`/`ReevaluateAll` request the D-Bus service enqueued
    /// since the last tick, then refresh the snapshot — called once per event-loop
    /// iteration from `wallpaperd.rs`'s `block_on` callback (T054).
    pub fn drain_dbus_requests(&mut self) {
        let requests = self.dbus_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).drain();
        for request in requests {
            match request {
                crate::dbus_service::ReevaluateRequest::One(id) => {
                    // The id was already validated against `known_outputs` when the
                    // request was enqueued — a failure here would mean the output was
                    // removed (hotplug disconnect) in between, which is a legitimate
                    // race, not a bug; contained silently, matching `evaluate_and_draw`'s
                    // own `Result` being ignorable by any other caller in this file.
                    let _ = self.evaluate_and_draw(&id);
                }
                crate::dbus_service::ReevaluateRequest::All => self.evaluate_and_draw_all(),
            }
        }
        self.refresh_dbus_snapshot();
    }

    /// The next instant any managed output needs re-evaluating — the idle-wait timer
    /// deadline (T021, FR-003).
    pub fn next_wake(&self) -> Option<chrono::DateTime<chrono::Local>> {
        self.outputs
            .iter()
            .filter_map(|o| o.loaded_pack.as_ref().map(|p| (o, p)))
            .filter_map(|(o, pack)| pack.pack.next_transition_after(self.location.as_ref(), chrono::Local::now()).map(|t| (o.id.clone(), t)))
            .map(|(_, t)| t)
            .min()
    }

    /// Reload every output's assignment/pack from the current `renderer_config` — the
    /// live-reconfiguration entry point (T028/T033, FR-007).
    pub fn reload_all_assignments(&mut self) {
        for index in 0..self.outputs.len() {
            self.load_pack_for(index);
        }
        self.evaluate_and_draw_all();
    }

    /// Record the `calloop` handle driving this daemon, so any method (not just a
    /// timer's own callback) can reschedule the idle-wait timer or insert other
    /// sources. Call once, right after construction.
    pub fn set_loop_handle(&mut self, handle: calloop::LoopHandle<'static, Self>) {
        self.loop_handle = Some(handle);
    }

    /// `min(schedule-driven next_wake, earliest pending coalesced deadline)`,
    /// converted from `DateTime<Local>`/`Instant` (calendar) domain into `Instant`
    /// (monotonic) domain relative to *now* — `Instant` has no calendar epoch, so a
    /// `DateTime` can't be compared to it directly. Falls back to a 60s ceiling if
    /// nothing is pending at all, so the daemon still wakes periodically rather than
    /// sleeping forever (e.g. to notice a system clock jump).
    fn next_wake_instant(&self) -> Instant {
        let now_local = chrono::Local::now();
        let from_schedule = self.next_wake().map(|target| {
            let delta = (target - now_local).to_std().unwrap_or(Duration::ZERO);
            Instant::now() + delta
        });
        let from_coalescer = self.coalescer.earliest_pending();

        match (from_schedule, from_coalescer) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => Instant::now() + Duration::from_secs(60),
        }
    }

    /// Replace the idle-wait timer with a fresh single-shot deadline computed from
    /// [`Self::next_wake_instant`] (T021, FR-003). Call this from every place that can
    /// change what "next wake" should be: the timer's own callback, a config/location
    /// watch firing, or a coalesced-change drain.
    pub fn reschedule_idle_timer(&mut self) {
        let Some(handle) = self.loop_handle.clone() else { return };
        if let Some(token) = self.idle_timer_token.take() {
            handle.remove(token);
        }
        let deadline = self.next_wake_instant();
        tracing::debug!(?deadline, "idle-wait timer rescheduled");
        let timer = Timer::from_deadline(deadline);
        let result = handle.insert_source(timer, |_deadline, _, daemon: &mut WallpaperDaemon| {
            daemon.on_idle_timer_fire();
            // Always fully replaced by the next `reschedule_idle_timer()` call —
            // never self-renews.
            TimeoutAction::Drop
        });
        match result {
            Ok(token) => self.idle_timer_token = Some(token),
            Err(e) => tracing::error!(error = %e, "failed to reschedule idle-wait timer"),
        }
    }

    fn on_idle_timer_fire(&mut self) {
        self.evaluate_and_draw_all();
        self.drain_coalescer();
        self.reschedule_idle_timer();
    }

    /// Re-evaluate+draw every output if any coalesced change is due (FR-014). Cheap
    /// to call unconditionally: skips `reload_all_assignments`'s O(outputs) work
    /// entirely when nothing's actually due.
    fn drain_coalescer(&mut self) {
        if !self.coalescer.due(Instant::now()).is_empty() {
            // Reload+draw *all* outputs rather than filtering to just the due ones —
            // cheap, idempotent, and the accepted posture per tasks.md T031/T036
            // (targeted per-output re-evaluation is a stretch goal, not required).
            self.reload_all_assignments();
        }
    }

    /// A live `RendererConfig` change was detected ([`cosmic_config::calloop::
    /// ConfigWatchSource`] firing in `wallpaperd.rs`) — record every managed output as
    /// changed and reschedule the idle timer so FR-014's 2s coalescing deadline is
    /// honored even if it's sooner than the next scheduled transition.
    pub fn on_renderer_config_changed(&mut self, new_config: RendererConfig) {
        self.renderer_config = new_config;
        let now = Instant::now();
        for id in self.output_ids() {
            self.coalescer.record_change(id, now);
        }
        self.reschedule_idle_timer();
    }

    /// Same shape as [`Self::on_renderer_config_changed`] for spec 4's
    /// `LocationConfig` (FR-015). Coalesces every managed output rather than only
    /// solar-anchored ones — the accepted first cut per tasks.md T050's own note.
    pub fn on_location_changed(&mut self, new_location: Option<Location>) {
        self.location = new_location;
        let now = Instant::now();
        for id in self.output_ids() {
            self.coalescer.record_change(id, now);
        }
        self.reschedule_idle_timer();
    }
}

impl CompositorHandler for WallpaperDaemon {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if let Some(index) = self.outputs.iter().position(|o| o.layer.wl_surface() == surface) {
            self.outputs[index].frame_callback_pending = false;
            self.evaluate_output(index, chrono::Local::now());
            self.draw(index);
        }
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for WallpaperDaemon {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, wl_output: wl_output::WlOutput) {
        if let Some(info) = self.output_state.info(&wl_output) {
            self.add_output(wl_output, info);
        }
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        // T040 (resize/rescale reconfiguration): not implemented this pass — see README.md.
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, wl_output: wl_output::WlOutput) {
        self.remove_output(&wl_output);
    }
}

impl LayerShellHandler for WallpaperDaemon {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.outputs.retain(|o| &o.layer != layer);
    }

    fn configure(&mut self, conn: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        let Some(index) = self.outputs.iter().position(|o| &o.layer == layer) else { return };
        let (w, h) = configure.new_size;
        self.outputs[index].size = Some((w, h));

        if let Err(e) = self.ensure_gpu_surface(conn, index) {
            tracing::error!(output = %self.outputs[index].id, error = %e, "GPU surface setup failed for this output");
            return;
        }
        // `ensure_gpu_surface` succeeding guarantees both of these are `Some` — but
        // matched with `let-else` rather than `unwrap()` (constitution Principle VIII)
        // so a future refactor that breaks that invariant fails closed, not by panic.
        let (Some(gpu), Some(wgpu_surface)) = (self.gpu.as_ref(), self.outputs[index].wgpu_surface.as_ref()) else {
            tracing::error!(output = %self.outputs[index].id, "GPU surface setup reported success but state is missing — skipping configure");
            return;
        };

        let caps = wgpu_surface.get_capabilities(&gpu.adapter);
        let format = caps.formats[0];
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        wgpu_surface.configure(&gpu.device, &surface_config);

        if self.pipeline.is_none() {
            if let Some(gpu) = self.gpu.as_ref() {
                self.pipeline = Some(CrossfadePipeline::new(&gpu.device, format));
            }
        }

        if self.outputs[index].loaded_pack.is_none() {
            self.load_pack_for(index);
        }
        self.evaluate_output(index, chrono::Local::now());
        self.draw(index);
        // This output's pack (and therefore its real next-transition instant) may
        // have just become known for the first time — resync the idle-wait timer so
        // it doesn't keep running on the startup fallback deadline (`next_wake_instant`
        // computed before any output existed) until that fallback happens to fire.
        self.reschedule_idle_timer();
    }
}

delegate_compositor!(WallpaperDaemon);
delegate_output!(WallpaperDaemon);
delegate_layer!(WallpaperDaemon);
delegate_registry!(WallpaperDaemon);
delegate_noop!(WallpaperDaemon: wp_viewporter::WpViewporter);
delegate_noop!(WallpaperDaemon: wp_viewport::WpViewport);

impl ProvidesRegistryState for WallpaperDaemon {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}
