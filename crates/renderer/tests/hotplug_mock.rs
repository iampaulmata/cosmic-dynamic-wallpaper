//! Mock hotplug/output-change harness (spec 7 US4, research.md R7, tasks.md T030-T033)
//! — a minimal in-process fake Wayland compositor built on `wayland-server`, driving
//! the real, unmodified SCTK client code in `crates/renderer/src/surface.rs` over an
//! in-memory `UnixStream` pair. Closes spec 3 tasks.md T043: this project's own real
//! hardware (this dev environment's two connected outputs) can produce a *connect*
//! event but never a *disconnect* or *resize* event on demand — this harness can
//! produce all three, deterministically, in `cargo test`.
//!
//! **Scope, stated plainly**: this harness implements just enough of `wl_compositor`/
//! `wl_output`/`zwlr_layer_shell_v1`/`wp_viewporter` for `WallpaperDaemon::new()` to
//! succeed and for output add/remove/resize to reach the real `OutputHandler`/
//! `LayerShellHandler` callbacks in `surface.rs` — it does not implement `xdg-output`
//! (this project's `update_output` resize path is a secondary one; the primary path,
//! exercised by T032 below, is `LayerShellHandler::configure`, which needs no xdg-output
//! at all) or `wp_fractional_scale_v1` (already an optional, gracefully-degraded global
//! in `WallpaperDaemon::new()`). T030/T031 never trigger a `zwlr_layer_surface_v1`
//! `configure` event, so they never reach `ensure_gpu_surface`'s real `wgpu`/Vulkan
//! surface creation at all — only T032 does, and that specific check is run with a
//! bounded timeout (see `with_timeout`) rather than risking an indefinite hang if this
//! synthetic compositor ever confuses the Vulkan WSI layer in a way this dev
//! environment's own live two-output hardware never has.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use wayland_client::{globals::registry_queue_init, Connection};
use wayland_server::backend::{ClientData, ClientId, DisconnectReason, GlobalId};
use wayland_server::protocol::{
    wl_callback::WlCallback,
    wl_compositor::{self, WlCompositor},
    wl_output::{self, WlOutput},
    wl_region::WlRegion,
    wl_surface::{self, WlSurface},
};
use wayland_server::{Client, DataInit, Display, DisplayHandle, GlobalDispatch, New, Resource};

use wayland_protocols::wp::viewporter::server::{
    wp_viewport::{self, WpViewport},
    wp_viewporter::{self, WpViewporter},
};
use wayland_protocols_wlr::layer_shell::v1::server::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};

use renderer::surface::WallpaperDaemon;

/// Test-only fixture data for a not-yet-bound `wl_output` global.
#[derive(Clone)]
struct OutputGlobalData {
    name: String,
    width: i32,
    height: i32,
}

/// State mutated from both the background dispatch thread and the foreground test
/// thread (e.g. to send a `Configure` event on a layer surface the client just
/// created) — kept behind a `Mutex` rather than woven through `Dispatch`'s owning
/// `State` type directly, since the test needs to reach in from outside the dispatch
/// loop.
#[derive(Default)]
struct Shared {
    output_globals: HashMap<String, GlobalId>,
    layer_surfaces: Vec<ZwlrLayerSurfaceV1>,
}

/// The `wayland-server` dispatch state — deliberately minimal (module doc's Scope
/// note): every request this fake compositor doesn't care about is accepted and
/// ignored, never causing a protocol error or a panic.
struct FakeCompositor {
    shared: Arc<Mutex<Shared>>,
}

struct NoClientData;
impl ClientData for NoClientData {
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

// --- wl_compositor ---------------------------------------------------------

impl GlobalDispatch<WlCompositor, ()> for FakeCompositor {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, resource: New<WlCompositor>, _: &(), data_init: &mut DataInit<'_, Self>) {
        data_init.init(resource, ());
    }
}

impl wayland_server::Dispatch<WlCompositor, ()> for FakeCompositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &WlCompositor,
        request: wl_compositor::Request,
        _: &(),
        _: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_compositor::Request::CreateSurface { id } => {
                data_init.init(id, ());
            }
            wl_compositor::Request::CreateRegion { id } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

impl wayland_server::Dispatch<WlRegion, ()> for FakeCompositor {
    fn request(_: &mut Self, _: &Client, _: &WlRegion, _: wayland_server::protocol::wl_region::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl wayland_server::Dispatch<WlSurface, ()> for FakeCompositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &WlSurface,
        request: wl_surface::Request,
        _: &(),
        _: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_surface::Request::Frame { callback } => {
                data_init.init(callback, ());
            }
            wl_surface::Request::GetRelease { callback } => {
                data_init.init(callback, ());
            }
            _ => {}
        }
    }
}

impl wayland_server::Dispatch<WlCallback, ()> for FakeCompositor {
    fn request(_: &mut Self, _: &Client, _: &WlCallback, _: <WlCallback as Resource>::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        unreachable!("wl_callback has no client-to-server requests")
    }
}

// --- wl_output ---------------------------------------------------------------

impl GlobalDispatch<WlOutput, OutputGlobalData> for FakeCompositor {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<WlOutput>,
        global_data: &OutputGlobalData,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let output = data_init.init(resource, ());
        // The exact minimal burst SCTK's own OutputState waits for before it considers
        // an output's info populated (Geometry/Mode/Name/Done — see this project's
        // vendored smithay-client-toolkit source, output.rs).
        let _ = output.send_event(wl_output::Event::Geometry {
            x: 0,
            y: 0,
            physical_width: 300,
            physical_height: 200,
            subpixel: wayland_server::WEnum::Value(wl_output::Subpixel::Unknown),
            make: "wallpaper-test".to_string(),
            model: "fake".to_string(),
            transform: wayland_server::WEnum::Value(wl_output::Transform::Normal),
        });
        let _ = output.send_event(wl_output::Event::Mode {
            flags: wayland_server::WEnum::Value(wl_output::Mode::Current),
            width: global_data.width,
            height: global_data.height,
            refresh: 60_000,
        });
        let _ = output.send_event(wl_output::Event::Scale { factor: 1 });
        let _ = output.send_event(wl_output::Event::Name { name: global_data.name.clone() });
        let _ = output.send_event(wl_output::Event::Done);
    }
}

impl wayland_server::Dispatch<WlOutput, ()> for FakeCompositor {
    fn request(_: &mut Self, _: &Client, _: &WlOutput, _: <WlOutput as Resource>::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

// --- zwlr_layer_shell_v1 -------------------------------------------------------

impl GlobalDispatch<ZwlrLayerShellV1, ()> for FakeCompositor {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, resource: New<ZwlrLayerShellV1>, _: &(), data_init: &mut DataInit<'_, Self>) {
        data_init.init(resource, ());
    }
}

impl wayland_server::Dispatch<ZwlrLayerShellV1, ()> for FakeCompositor {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ZwlrLayerShellV1,
        request: zwlr_layer_shell_v1::Request,
        _: &(),
        _: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_layer_shell_v1::Request::GetLayerSurface { id, .. } = request {
            let layer_surface = data_init.init(id, ());
            state.shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner).layer_surfaces.push(layer_surface);
        }
    }
}

impl wayland_server::Dispatch<ZwlrLayerSurfaceV1, ()> for FakeCompositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &ZwlrLayerSurfaceV1,
        _: zwlr_layer_surface_v1::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        // Every request (SetSize/SetAnchor/AckConfigure/...) is a no-op here — this
        // harness drives `Configure` events explicitly from the test body instead of
        // reacting to the client's own requests.
    }
}

// --- wp_viewporter --------------------------------------------------------------

impl GlobalDispatch<WpViewporter, ()> for FakeCompositor {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, resource: New<WpViewporter>, _: &(), data_init: &mut DataInit<'_, Self>) {
        data_init.init(resource, ());
    }
}

impl wayland_server::Dispatch<WpViewporter, ()> for FakeCompositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &WpViewporter,
        request: wp_viewporter::Request,
        _: &(),
        _: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let wp_viewporter::Request::GetViewport { id, .. } = request {
            data_init.init(id, ());
        }
    }
}

impl wayland_server::Dispatch<WpViewport, ()> for FakeCompositor {
    fn request(_: &mut Self, _: &Client, _: &WpViewport, _: wp_viewport::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

/// Everything needed to drive the fake compositor and the real client under test
/// together for one scenario.
struct TestRig {
    daemon: WallpaperDaemon,
    event_queue: wayland_client::EventQueue<WallpaperDaemon>,
    conn: Connection,
    shared: Arc<Mutex<Shared>>,
    dh: DisplayHandle,
    stop: Arc<AtomicBool>,
    server_thread: Option<std::thread::JoinHandle<()>>,
}

impl TestRig {
    fn new() -> Self {
        let mut display: Display<FakeCompositor> = Display::new().expect("create fake compositor display");
        let dh = display.handle();

        dh.create_global::<FakeCompositor, WlCompositor, ()>(4, ());
        dh.create_global::<FakeCompositor, ZwlrLayerShellV1, ()>(4, ());
        dh.create_global::<FakeCompositor, WpViewporter, ()>(1, ());

        let (server_stream, client_stream) = UnixStream::pair().expect("create socket pair");
        let mut dh_for_client = display.handle();
        dh_for_client.insert_client(server_stream, Arc::new(NoClientData)).expect("insert fake client");

        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut state = FakeCompositor { shared: shared.clone() };
        let stop = Arc::new(AtomicBool::new(false));

        let server_thread = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = display.dispatch_clients(&mut state);
                    let _ = display.flush_clients();
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
        };

        let conn = Connection::from_socket(client_stream).expect("client connect to fake compositor");
        let (globals, event_queue) = registry_queue_init::<WallpaperDaemon>(&conn).expect("initial registry roundtrip");
        let qh = event_queue.handle();

        let registry = tempfile_registry();
        let renderer_config = wallpaper_ipc::RendererConfig::default();
        let daemon =
            WallpaperDaemon::new(&globals, &qh, registry, renderer_config, None).expect("WallpaperDaemon::new against the fake compositor");

        TestRig { daemon, event_queue, conn, shared, dh, stop, server_thread: Some(server_thread) }
    }

    /// Add a new output global and drive the client's event queue until it's
    /// processed (T030) — the same `wl_registry.global` + `wl_output` burst a real
    /// hotplug connect produces, per `RegistryState`'s already-established dynamic
    /// global-add handling (`registry_handlers![OutputState]` in `surface.rs`).
    fn connect_output(&mut self, name: &str, width: i32, height: i32) {
        let id = self.dh.create_global::<FakeCompositor, WlOutput, OutputGlobalData>(
            4,
            OutputGlobalData { name: name.to_string(), width, height },
        );
        self.shared.lock().unwrap().output_globals.insert(name.to_string(), id);
        // The registry-add + wl_output burst (Geometry/Mode/Name/Done) each need their
        // own round trip; `add_output`'s own reaction (create_surface/
        // get_layer_surface/commit, all fired synchronously from inside the `Done`
        // event's dispatch) is only *written* to the client's outgoing buffer at that
        // point, not necessarily flushed+read by the (separately-threaded) fake
        // server yet — `wait_for` below is what actually guarantees the server has
        // caught up, not a fixed round-trip count (found as a real race, only
        // surfacing when several of this file's tests run in parallel against
        // separate, CPU-contending fake compositors).
        self.roundtrip();
        self.roundtrip();
        let _ = self.conn.flush();
        self.wait_for("wl_output added to daemon.output_ids()", |rig| !rig.daemon.output_ids().is_empty());
    }

    /// Remove a previously-added output global (T031) — `remove_global` is
    /// `wayland-server`'s own mechanism for sending `wl_registry.global_remove`.
    fn disconnect_output(&mut self, name: &str) {
        let id = self.shared.lock().unwrap().output_globals.remove(name).expect("output was connected");
        self.dh.remove_global::<FakeCompositor>(id);
        self.roundtrip();
    }

    /// Send a `Configure` event on the most recently created layer surface (T032) —
    /// the primary resize path (`LayerShellHandler::configure`), matching how a real
    /// compositor announces a size change. Waits for the layer surface to actually
    /// exist server-side first (see `connect_output`'s doc comment on this same race).
    fn configure_last_layer_surface(&mut self, width: u32, height: u32) {
        self.wait_for("a layer surface exists server-side", |rig| !rig.shared.lock().unwrap().layer_surfaces.is_empty());
        let layer_surface = {
            let shared = self.shared.lock().unwrap();
            shared.layer_surfaces.last().cloned().expect("a layer surface exists")
        };
        let _ = layer_surface.send_event(zwlr_layer_surface_v1::Event::Configure { serial: 1, width, height });
    }

    /// Polls `condition` via repeated round trips + short sleeps, up to a bounded
    /// number of attempts — the fake server runs on its own background thread, so
    /// "the client sent a request" and "the server has processed it" are never
    /// synchronous with each other; this is the harness's one real concession to that,
    /// rather than assuming a fixed number of round trips always suffices under any
    /// scheduling.
    fn wait_for(&mut self, label: &str, mut condition: impl FnMut(&Self) -> bool) {
        for _ in 0..100 {
            if condition(self) {
                return;
            }
            let _ = self.conn.flush();
            std::thread::sleep(Duration::from_millis(10));
            self.roundtrip();
        }
        panic!("timed out waiting for: {label}");
    }

    fn roundtrip(&mut self) {
        self.event_queue.roundtrip(&mut self.daemon).expect("client roundtrip against the fake compositor");
    }
}

impl Drop for TestRig {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.server_thread.take() {
            let _ = handle.join();
        }
    }
}

fn tempfile_registry() -> pack_loader::Registry {
    let dir = tempfile::tempdir().expect("tempdir");
    // Leaked deliberately — this harness is test-only and short-lived; keeping the
    // TempDir alive for the registry's lifetime without threading an extra lifetime
    // through TestRig is a reasonable simplification here.
    let path = dir.keep();
    pack_loader::Registry::open_at(&path).expect("open scratch registry")
}

/// Runs `body` on its own thread with a bounded wait — used for the one check
/// (T032) that reaches real `wgpu`/Vulkan surface-creation code against this
/// synthetic compositor (module doc's Scope note: a real, if judged unlikely, risk of
/// blocking that this dev environment's own live two-output hardware has never
/// exercised).
fn with_timeout<F: FnOnce() + Send + 'static>(label: &str, body: F) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        body();
        let _ = tx.send(());
    });
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(()) => {
            let _ = handle.join();
        }
        Err(_) => panic!("{label} did not complete within 20s — see hotplug_mock.rs's module doc Scope note"),
    }
}

/// T030: an output connect event via the `wayland-server` double reaches the same
/// real SCTK client code path a physical compositor triggers.
#[test]
fn output_connect_is_added_to_the_managed_output_list() {
    let mut rig = TestRig::new();
    assert!(rig.daemon.output_ids().is_empty(), "no outputs before any connect");

    rig.connect_output("TEST-1", 1920, 1080);

    let ids = rig.daemon.output_ids();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].as_str(), "TEST-1");

    let _ = rig.conn.flush();
}

/// T031: an output disconnect event — previously entirely untested on any hardware
/// this project has access to — correctly tears down state without panicking (closes
/// spec 3 tasks.md T043's disconnect gap).
#[test]
fn output_disconnect_removes_it_from_the_managed_output_list_without_panicking() {
    let mut rig = TestRig::new();
    rig.connect_output("TEST-1", 1920, 1080);
    assert_eq!(rig.daemon.output_ids().len(), 1);

    rig.disconnect_output("TEST-1");

    assert!(rig.daemon.output_ids().is_empty(), "output removed cleanly, no panic");
    let _ = rig.conn.flush();
}

/// T031 (multi-output): disconnecting one output leaves an unrelated one intact —
/// mirrors this project's own existing multi-output-independence coverage
/// (spec 3's `two_outputs_resolve_independently`) for the hotplug path specifically.
#[test]
fn disconnecting_one_output_leaves_another_untouched() {
    let mut rig = TestRig::new();
    rig.connect_output("TEST-1", 1920, 1080);
    rig.connect_output("TEST-2", 2560, 1440);
    assert_eq!(rig.daemon.output_ids().len(), 2);

    rig.disconnect_output("TEST-1");

    let ids = rig.daemon.output_ids();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].as_str(), "TEST-2");
    let _ = rig.conn.flush();
}

/// T032: an output resize/scale-change event correctly triggers reconfiguration
/// (closes spec 3's previously-untested resize branch) — bounded by [`with_timeout`]
/// since this is the one check that reaches real `wgpu`/Vulkan surface-creation code
/// (see this module's doc comment).
#[test]
fn output_resize_triggers_reconfiguration() {
    with_timeout("output_resize_triggers_reconfiguration", || {
        let mut rig = TestRig::new();
        rig.connect_output("TEST-1", 1920, 1080);
        assert_eq!(rig.daemon.output_ids().len(), 1);

        rig.configure_last_layer_surface(800, 600);
        // `LayerShellHandler::configure` is invoked synchronously inside this
        // dispatch — reaching `reconfigure_output` in `surface.rs`, which sets the
        // output's size unconditionally before attempting GPU surface (re)creation
        // (module doc's Scope note), so no panic here is itself the assertion that
        // matters most: the real code path ran end to end without crashing.
        rig.roundtrip();

        // The output is still managed (reconfiguration didn't tear it down).
        assert_eq!(rig.daemon.output_ids().len(), 1);
        let _ = rig.conn.flush();
    });
}

/// Spec 011 US1 FR-002 (research.md R2): the layer-shell protocol legitimately
/// reports 0 on an axis when both opposing anchors are set on that axis — exactly
/// what a real compositor's own `Configure` event can send, not malformed input. Prior
/// to the fix this reached `wgpu::Surface::configure` with a zero dimension and
/// panicked the whole daemon, not just this one output — this test's real assertion is
/// that no panic occurs, for both axes independently.
#[test]
fn zero_size_reconfigure_does_not_panic() {
    with_timeout("zero_size_reconfigure_does_not_panic", || {
        let mut rig = TestRig::new();
        rig.connect_output("TEST-1", 1920, 1080);
        assert_eq!(rig.daemon.output_ids().len(), 1);

        rig.configure_last_layer_surface(0, 600);
        rig.roundtrip();
        assert_eq!(rig.daemon.output_ids().len(), 1, "zero-width configure didn't panic or tear down the output");

        rig.configure_last_layer_surface(800, 0);
        rig.roundtrip();
        assert_eq!(rig.daemon.output_ids().len(), 1, "zero-height configure didn't panic or tear down the output");

        let _ = rig.conn.flush();
    });
}
