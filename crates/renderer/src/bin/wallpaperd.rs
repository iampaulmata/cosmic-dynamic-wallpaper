//! `wallpaperd` — the wallpaper renderer daemon (T020). Connects to Wayland, loads
//! config, manages every output's crossfade/idle-wait lifecycle via a `calloop` event
//! loop, and live-watches `RendererConfig`/`LocationSource` for changes (no restart
//! needed). See `crates/renderer/README.md` for what this binary does and doesn't
//! cover yet (no live D-Bus service this pass — see that file).

use cosmic_config::calloop::ConfigWatchSource;
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

use pack_loader::Registry;
use renderer::surface::WallpaperDaemon;
use renderer::{LocationSource, RendererConfig};

fn main() {
    tracing_subscriber::fmt::init();

    if let Err(e) = run() {
        tracing::error!(error = %e, "wallpaperd exiting due to a startup error");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init::<WallpaperDaemon>(&conn)?;
    let qh = event_queue.handle();

    let mut event_loop: EventLoop<'static, WallpaperDaemon> = EventLoop::try_new()?;
    WaylandSource::new(conn, event_queue).insert(event_loop.handle()).map_err(|e| e.error)?;

    let pack_registry = Registry::open().map_err(|e| format!("pack registry: {e}"))?;
    let renderer_config_store = RendererConfig::open()?;
    let renderer_config = RendererConfig::load(&renderer_config_store);
    let location_store = LocationSource::open()?;
    let location = LocationSource::load(&location_store).location;

    tracing::info!(
        overrides = renderer_config.overrides.len(),
        same_everywhere = renderer_config.same_pack_everywhere.is_some(),
        has_location = location.is_some(),
        "wallpaperd starting"
    );

    let mut daemon = WallpaperDaemon::new(&globals, &qh, pack_registry, renderer_config, location)?;
    daemon.set_loop_handle(event_loop.handle());

    // Live config-watch (T028/T033/T050): a `RendererConfig`/`LocationSource` change
    // written by `wallpaperctl` is picked up without restarting this daemon — each
    // watch feeds `Coalescer` (FR-014's 2s debounce) via `on_renderer_config_changed`/
    // `on_location_changed`, which also reschedules the idle-wait timer below so the
    // coalesced deadline is honored even if it's sooner than the next transition.
    let renderer_watch = ConfigWatchSource::new(&renderer_config_store).map_err(|e| format!("failed to watch renderer config: {e}"))?;
    event_loop
        .handle()
        .insert_source(renderer_watch, |(config, _changed_keys), _, daemon: &mut WallpaperDaemon| {
            daemon.on_renderer_config_changed(RendererConfig::load(&config));
        })
        .map_err(|e| format!("failed to insert renderer-config watch: {e}"))?;

    let location_watch = ConfigWatchSource::new(&location_store).map_err(|e| format!("failed to watch location config: {e}"))?;
    event_loop
        .handle()
        .insert_source(location_watch, |(config, _changed_keys), _, daemon: &mut WallpaperDaemon| {
            daemon.on_location_changed(LocationSource::load(&config).location);
        })
        .map_err(|e| format!("failed to insert location watch: {e}"))?;

    // Idle-wait timer (T021): a precise single-shot deadline computed from
    // `WallpaperDaemon::next_wake` (schedule transitions) and any pending coalesced
    // config change, rescheduled after every fire and every watch-triggered change —
    // see `WallpaperDaemon::reschedule_idle_timer`.
    daemon.reschedule_idle_timer();

    loop {
        event_loop.dispatch(None, &mut daemon)?;
        if daemon.exit {
            break;
        }
    }

    Ok(())
}
