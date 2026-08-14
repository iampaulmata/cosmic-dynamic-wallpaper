//! `wallpaperd` — the wallpaper renderer daemon (T020). Connects to Wayland, loads
//! config, manages every output's crossfade/idle-wait lifecycle via a `calloop` event
//! loop, live-watches `RendererConfig`/`LocationConfigEntry` for changes (no restart
//! needed), and serves a live D-Bus service for `wallpaperctl query`/`reevaluate`/
//! `list outputs` (FR-016). See `crates/renderer/README.md` for what this binary does
//! and doesn't cover yet.

use cosmic_config::calloop::ConfigWatchSource;
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

use pack_loader::Registry;
use renderer::dbus_service::{self, DaemonInterface};
use renderer::ip_geolocation::{self, IpGeoEvent};
use renderer::portal_location::{self, PortalEvent};
use renderer::starter_pack;
use renderer::surface::WallpaperDaemon;
use renderer::{effective_location, LocationConfigEntry, LocationMode, RendererConfig};

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

    let mut pack_registry = Registry::open().map_err(|e| format!("pack registry: {e}"))?;
    let renderer_config_store = RendererConfig::open()?;
    let mut renderer_config = RendererConfig::load(&renderer_config_store);

    // spec 7 US2 (FR-008/FR-010/FR-011): self-register the bundled starter pack on a
    // genuinely fresh install — see starter_pack.rs's module doc for why this happens
    // here rather than in postinst (a per-user cosmic-config write postinst has no way
    // to make correctly, running as root with no user context).
    if starter_pack::maybe_register(std::path::Path::new(starter_pack::STARTER_PACK_SYSTEM_PATH), &mut pack_registry, &mut renderer_config) {
        if let Err(e) = renderer_config.save(&renderer_config_store) {
            tracing::error!(error = %e, "failed to persist the starter pack's default assignment");
        }
        tracing::info!("registered and assigned the bundled starter pack (fresh install)");
    }

    let location_store = LocationConfigEntry::open()?;
    let initial_location_entry = LocationConfigEntry::load(&location_store);
    // spec 6 Cross-Spec Dependency (plan.md): scheduling reads the *effective* location
    // — the resolved automatic value when automatic mode is active, falling back to the
    // manual value — never `LocationConfigEntry.location` directly, or automatic mode would
    // be silently ignored by actual scheduling even though the config value is
    // correctly persisted.
    let location = effective_location(&initial_location_entry);

    tracing::info!(
        overrides = renderer_config.overrides.len(),
        same_everywhere = renderer_config.same_pack_everywhere.is_some(),
        location_mode = ?initial_location_entry.mode,
        has_location = location.is_some(),
        "wallpaperd starting"
    );

    let mut daemon = WallpaperDaemon::new(&globals, &qh, pack_registry, renderer_config, location)?;
    daemon.set_loop_handle(event_loop.handle());

    // Live config-watch (T028/T033/T050): a `RendererConfig`/`LocationConfigEntry` change
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

    // spec 6 US1/US3: the portal-driving async task (`portal_location::run`) is spawned
    // once automatic mode is (or becomes) active, and reports every resolution/failure
    // over this channel — see `portal_location`'s module doc for the exact write-back
    // contract and the "spawned once, not cancelled on mode toggle" simplification.
    let (portal_events_tx, portal_events_rx) = calloop::channel::channel::<PortalEvent>();
    let (portal_executor, portal_scheduler) =
        calloop::futures::executor::<()>().map_err(|e| format!("failed to create the portal futures executor: {e}"))?;
    event_loop
        .handle()
        .insert_source(portal_executor, |(), _, _: &mut WallpaperDaemon| {})
        .map_err(|e| format!("failed to insert the portal futures executor: {e}"))?;

    let mut portal_task_spawned = false;
    let spawn_portal_task_if_needed = {
        let portal_scheduler = portal_scheduler.clone();
        let portal_events_tx = portal_events_tx.clone();
        move |mode: LocationMode, spawned: &mut bool| {
            if *spawned || mode != LocationMode::Automatic {
                return;
            }
            if portal_scheduler.schedule(portal_location::run(portal_events_tx.clone())).is_err() {
                tracing::error!("failed to schedule the automatic-location resolution task — event loop already gone");
                return;
            }
            *spawned = true;
        }
    };
    spawn_portal_task_if_needed(initial_location_entry.mode, &mut portal_task_spawned);

    event_loop
        .handle()
        .insert_source(portal_events_rx, {
            let location_store = location_store.clone();
            move |event, _, daemon: &mut WallpaperDaemon| {
                let calloop::channel::Event::Msg(portal_event) = event else { return };
                let mut entry = LocationConfigEntry::load(&location_store);
                match portal_event {
                    PortalEvent::Reading(reading) => portal_location::apply_reading(&mut entry, reading),
                    PortalEvent::Failure(reason) => portal_location::apply_failure(&mut entry, reason),
                }
                if let Err(e) = entry.save(&location_store) {
                    tracing::error!(error = %e, "failed to persist an automatic-location resolution");
                }
                // Applied directly (not waited on via `location_watch` below) so
                // scheduling reacts immediately rather than waiting on a filesystem
                // watch round trip; `location_watch` will also observe this same write
                // shortly after — redundant but harmless (module doc's write-back
                // contract: this daemon is the entry's own watcher as well as writer).
                daemon.on_location_changed(effective_location(&entry));
            }
        })
        .map_err(|e| format!("failed to insert the portal event channel: {e}"))?;

    // spec 7 US3: IP-geolocation resolution runs on its own dedicated background OS
    // thread (ip_geolocation.rs's module doc explains why — stunclient's only usable
    // API is synchronous), spawned once IP-geolocation mode is (or becomes) active,
    // same "spawned once" posture as the portal task above.
    let (ip_geo_events_tx, ip_geo_events_rx) = calloop::channel::channel::<IpGeoEvent>();
    let mut ip_geo_task_spawned = false;
    let spawn_ip_geo_task_if_needed = {
        let ip_geo_events_tx = ip_geo_events_tx.clone();
        move |mode: LocationMode, spawned: &mut bool| {
            if *spawned || mode != LocationMode::IpGeolocation {
                return;
            }
            ip_geolocation::spawn(std::path::PathBuf::from(ip_geolocation::MMDB_SYSTEM_PATH), ip_geo_events_tx.clone());
            *spawned = true;
        }
    };
    spawn_ip_geo_task_if_needed(initial_location_entry.mode, &mut ip_geo_task_spawned);

    event_loop
        .handle()
        .insert_source(ip_geo_events_rx, {
            let location_store = location_store.clone();
            move |event, _, daemon: &mut WallpaperDaemon| {
                let calloop::channel::Event::Msg(ip_geo_event) = event else { return };
                let mut entry = LocationConfigEntry::load(&location_store);
                match ip_geo_event {
                    IpGeoEvent::Reading(location) => ip_geolocation::apply_reading(&mut entry, location),
                    IpGeoEvent::Failure(reason) => ip_geolocation::apply_failure(&mut entry, reason),
                }
                if let Err(e) = entry.save(&location_store) {
                    tracing::error!(error = %e, "failed to persist an IP-geolocation resolution");
                }
                daemon.on_location_changed(effective_location(&entry));
            }
        })
        .map_err(|e| format!("failed to insert the IP-geolocation event channel: {e}"))?;

    let location_watch = ConfigWatchSource::new(&location_store).map_err(|e| format!("failed to watch location config: {e}"))?;
    event_loop
        .handle()
        .insert_source(location_watch, move |(config, _changed_keys), _, daemon: &mut WallpaperDaemon| {
            let entry = LocationConfigEntry::load(&config);
            spawn_portal_task_if_needed(entry.mode, &mut portal_task_spawned);
            spawn_ip_geo_task_if_needed(entry.mode, &mut ip_geo_task_spawned);
            daemon.on_location_changed(effective_location(&entry));
        })
        .map_err(|e| format!("failed to insert location watch: {e}"))?;

    // Idle-wait timer (T021): a precise single-shot deadline computed from
    // `WallpaperDaemon::next_wake` (schedule transitions) and any pending coalesced
    // config change, rescheduled after every fire and every watch-triggered change —
    // see `WallpaperDaemon::reschedule_idle_timer`.
    daemon.reschedule_idle_timer();

    // Live D-Bus service (T049/T053/T054, FR-016): `internal_executor(false)` means
    // this connection spawns no driver thread of its own — its executor is ticked
    // forward as a foreign future via `calloop`'s `block_on`, the same pattern zbus's
    // own `Connection::executor` doc example uses (just driven by `calloop` instead of
    // `tokio::spawn`). See `dbus_service`'s module doc for the full integration story.
    let iface = DaemonInterface { state: daemon.dbus_state() };
    let connection = pollster::block_on(
        zbus::connection::Builder::session()?.name(dbus_service::BUS_NAME)?.serve_at(dbus_service::OBJECT_PATH, iface)?.internal_executor(false).build(),
    )
    .map_err(|e| format!("failed to start the D-Bus service: {e}"))?;
    tracing::info!(bus_name = dbus_service::BUS_NAME, "D-Bus service registered");

    let executor = connection.executor().clone();
    let drive_zbus = async move {
        loop {
            executor.tick().await;
        }
    };

    let signal = event_loop.get_signal();
    event_loop.block_on(drive_zbus, &mut daemon, |daemon| {
        daemon.drain_dbus_requests();
        if daemon.exit {
            signal.stop();
        }
    })?;

    Ok(())
}
