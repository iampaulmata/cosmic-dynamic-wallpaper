//! `wallpaperd` — the wallpaper renderer daemon (T020). Connects to Wayland, loads
//! config, manages every output's crossfade/idle-wait lifecycle via a `calloop` event
//! loop. See `crates/renderer/README.md` for what this binary does and doesn't cover
//! yet (no live config-watch or D-Bus service this pass — see that file).

use std::time::Duration;

use smithay_client_toolkit::reexports::calloop::{self, timer::Timer, EventLoop};
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

    // Idle-wait timer (T021): re-evaluate periodically. A full implementation would
    // compute the exact next-transition instant per output (`WallpaperDaemon::
    // next_wake`) and schedule a single precise timer; this pass uses a bounded
    // periodic tick instead, which is simpler and still correct (just not maximally
    // idle) — see README.md's scope note on what's simplified in this binary
    // specifically vs. the fully event-driven design data-model.md describes.
    let timer_source = Timer::from_duration(Duration::from_secs(5));
    event_loop
        .handle()
        .insert_source(timer_source, |_deadline, _, daemon: &mut WallpaperDaemon| {
            daemon.evaluate_and_draw_all();
            calloop::timer::TimeoutAction::ToDuration(Duration::from_secs(5))
        })
        .map_err(|e| format!("failed to insert idle-wait timer: {e}"))?;

    loop {
        event_loop.dispatch(None, &mut daemon)?;
        if daemon.exit {
            break;
        }
    }

    Ok(())
}
