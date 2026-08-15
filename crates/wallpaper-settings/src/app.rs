//! The `cosmic::Application` implementation — a single window, sidebar navigation
//! between the five pages (contracts/gui-application.md). Every page reads/writes the
//! identical `cosmic-config`/D-Bus state `wallpaperctl`/`wallpaperd` already do, via
//! `wallpaper_ipc`/`pack_loader` directly (FR-007: enforced structurally, not by
//! convention — plan.md Constitution Check finding 1).

use cosmic::app::Task;
use cosmic::widget::{self, nav_bar};
use cosmic::{executor, Core, Element};
use schedule_engine::Location;
use wallpaper_ipc::{effective_location, LocationConfigEntry, RendererConfig};

use crate::{pack_display, pages};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Packs,
    Assignment,
    Location,
    Timeline,
    Crossfade,
}

pub struct App {
    core: Core,
    nav_model: nav_bar::Model,
    pack_registry: pack_loader::Registry,
    renderer_config_store: cosmic_config::Config,
    location_config_store: cosmic_config::Config,
    packs: pages::packs::State,
    assignment: pages::assignment::State,
    location: pages::location::State,
    timeline: pages::timeline::State,
    crossfade: pages::crossfade::State,
    /// The Custom Pack Builder wizard (spec 010), when open — `Some` takes over the
    /// main view in place of whichever nav page is selected (research.md R9), the same
    /// "one extra `Option<T>` field gates an alternate view" shape `packs::State.
    /// pending_removal` already uses, just page-sized rather than modal-sized.
    pack_builder: Option<pages::pack_builder::State>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Packs(pages::packs::Message),
    Assignment(pages::assignment::Message),
    Location(pages::location::Message),
    Timeline(pages::timeline::Message),
    Crossfade(pages::crossfade::Message),
    PackBuilder(pages::pack_builder::Message),
}

/// Fatal startup failures are reported clearly and exit non-zero — the same posture
/// `wallpaperd`/`wallpaperctl`'s own `main()` functions use (constitution Principle
/// VIII: contained, never a silent panic; `std::process::exit` here, not
/// `unwrap`/`expect`, satisfies this crate's own `deny(unwrap_used, expect_used)`).
fn open_registry_or_exit() -> pack_loader::Registry {
    pack_loader::Registry::open().unwrap_or_else(|e| {
        eprintln!("error: failed to open the pack registry: {e}");
        std::process::exit(1);
    })
}

fn open_renderer_config_or_exit() -> cosmic_config::Config {
    RendererConfig::open().unwrap_or_else(|e| {
        eprintln!("error: failed to open the renderer config: {e}");
        std::process::exit(1);
    })
}

fn open_location_config_or_exit() -> cosmic_config::Config {
    LocationConfigEntry::open().unwrap_or_else(|e| {
        eprintln!("error: failed to open the location config: {e}");
        std::process::exit(1);
    })
}

impl App {
    fn refresh_assignment(&mut self) {
        let renderer_config = RendererConfig::migrate_from_old_app_id(&self.renderer_config_store);
        let known_outputs = wallpaper_ipc::DbusClient::connect().and_then(|c| c.query_all()).map(|entries| entries.into_iter().map(|e| e.output).collect()).unwrap_or_default();
        let available_packs = self.pack_registry.known_packs().into_iter().map(|e| e.source).collect();
        self.assignment = pages::assignment::State { known_outputs, available_packs, current_config: renderer_config };
    }

    /// The current/next thumbnails on the Timeline page (`pack_display::
    /// resolve_schedule_snapshot`) need the same renderer config and effective
    /// location every other page already loads independently — read fresh here rather
    /// than threading a stale copy through.
    fn load_timeline(&self) -> pages::timeline::State {
        let renderer_config = RendererConfig::migrate_from_old_app_id(&self.renderer_config_store);
        let location = effective_location(&LocationConfigEntry::migrate_from_old_app_id(&self.location_config_store));
        pages::timeline::State::load(renderer_config, location)
    }

    /// The effective location (spec 6), read fresh — used by the pack-builder wizard's
    /// solar-mode conflict check (research.md R4), the same source `pages::location`/
    /// `pages::timeline` already read independently rather than threading a stale copy.
    fn current_location(&self) -> Option<Location> {
        effective_location(&LocationConfigEntry::migrate_from_old_app_id(&self.location_config_store))
    }

    fn refresh_active_page(&mut self) {
        match self.nav_model.active_data::<Page>().copied() {
            Some(Page::Packs) => self.packs = pages::packs::State::load(&mut self.pack_registry),
            Some(Page::Assignment) => self.refresh_assignment(),
            Some(Page::Location) => self.location = pages::location::State::load(LocationConfigEntry::migrate_from_old_app_id(&self.location_config_store)),
            Some(Page::Timeline) => self.timeline = self.load_timeline(),
            Some(Page::Crossfade) => self.crossfade = pages::crossfade::State { current_config: RendererConfig::migrate_from_old_app_id(&self.renderer_config_store) },
            None => {}
        }
    }
}

impl cosmic::Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.system76.CosmicDynamicWallpaperSettings";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let mut nav_model = nav_bar::Model::default();
        nav_model.insert().text("Packs").data(Page::Packs);
        nav_model.insert().text("Assignment").data(Page::Assignment);
        nav_model.insert().text("Location").data(Page::Location);
        nav_model.insert().text("Timeline").data(Page::Timeline);
        nav_model.insert().text("Crossfade").data(Page::Crossfade);
        nav_model.activate_position(0);

        let mut pack_registry = open_registry_or_exit();
        let renderer_config_store = open_renderer_config_or_exit();
        let location_config_store = open_location_config_or_exit();

        let packs = pages::packs::State::load(&mut pack_registry);
        let renderer_config = RendererConfig::migrate_from_old_app_id(&renderer_config_store);
        let known_outputs = wallpaper_ipc::DbusClient::connect().and_then(|c| c.query_all()).map(|entries| entries.into_iter().map(|e| e.output).collect()).unwrap_or_default();
        let available_packs = pack_registry.known_packs().into_iter().map(|e| e.source).collect();
        let assignment = pages::assignment::State { known_outputs, available_packs, current_config: renderer_config.clone() };
        let location = pages::location::State::load(LocationConfigEntry::migrate_from_old_app_id(&location_config_store));
        let timeline_location = effective_location(&LocationConfigEntry::migrate_from_old_app_id(&location_config_store));
        let timeline = pages::timeline::State::load(renderer_config.clone(), timeline_location);
        let crossfade = pages::crossfade::State { current_config: renderer_config };

        let app = App {
            core,
            nav_model,
            pack_registry,
            renderer_config_store,
            location_config_store,
            packs,
            assignment,
            location,
            timeline,
            crossfade,
            pack_builder: None,
        };
        (app, Task::none())
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        self.nav_model.activate(id);
        self.refresh_active_page();
        Task::none()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Packs(pages::packs::Message::Refresh) => {
                self.packs = pages::packs::State::load(&mut self.pack_registry);
            }
            Message::Packs(pages::packs::Message::AddFolderRequested) => {
                pages::packs::request_add(&mut self.packs);
                return cosmic::task::future(async move {
                    use cosmic::dialog::file_chooser;
                    match file_chooser::open::Dialog::new().title("Choose a pack folder").open_folder().await {
                        Ok(response) => match response.url().to_file_path() {
                            Ok(path) => Message::Packs(pages::packs::Message::AddResult(Ok(path))),
                            Err(()) => Message::Packs(pages::packs::Message::AddResult(Err(
                                "the selected folder has no local file path".to_string(),
                            ))),
                        },
                        Err(file_chooser::Error::Cancelled) => Message::Packs(pages::packs::Message::AddCancelled),
                        Err(e) => Message::Packs(pages::packs::Message::AddResult(Err(e.to_string()))),
                    }
                });
            }
            Message::Packs(pages::packs::Message::AddFileRequested) => {
                pages::packs::request_add(&mut self.packs);
                return cosmic::task::future(async move {
                    use cosmic::dialog::file_chooser;
                    match file_chooser::open::Dialog::new().title("Choose an image file").open_file().await {
                        Ok(response) => match response.url().to_file_path() {
                            Ok(path) => Message::Packs(pages::packs::Message::AddResult(Ok(path))),
                            Err(()) => Message::Packs(pages::packs::Message::AddResult(Err(
                                "the selected file has no local file path".to_string(),
                            ))),
                        },
                        Err(file_chooser::Error::Cancelled) => Message::Packs(pages::packs::Message::AddCancelled),
                        Err(e) => Message::Packs(pages::packs::Message::AddResult(Err(e.to_string()))),
                    }
                });
            }
            Message::Packs(pages::packs::Message::AddCancelled) => {
                // A cancelled file-chooser dialog is a no-op, not an error
                // (research.md R1) — nothing to do.
            }
            Message::Packs(pages::packs::Message::AddResult(result)) => {
                // Spec 010 (Custom Pack Builder) research.md R1: a picked directory
                // that fails specifically with `ManifestNotFound` opens the wizard
                // instead of registering (today's `apply_add_result` never actually
                // validated a directory before registering it — `should_open_for`'s
                // `load_pack` call is a real, deliberate tightening, not just a wizard
                // trigger: it stops a manifest-free folder from silently becoming a
                // broken registered pack). Every other outcome (success, or any other
                // load/registration error) is unchanged.
                match result {
                    Ok(path) if pages::pack_builder::should_open_for(&path) => {
                        self.pack_builder = Some(pages::pack_builder::open(path));
                    }
                    other => {
                        pages::packs::apply_add_result(&mut self.packs, &mut self.pack_registry, other);
                    }
                }
            }
            Message::Packs(pages::packs::Message::RemoveRequested(source)) => {
                pages::packs::request_removal(&mut self.packs, source);
            }
            Message::Packs(pages::packs::Message::RemoveConfirmed) => {
                pages::packs::confirm_removal(&mut self.packs, &mut self.pack_registry);
            }
            Message::Packs(pages::packs::Message::RemoveCancelled) => {
                pages::packs::cancel_removal(&mut self.packs);
            }
            Message::Assignment(pages::assignment::Message::ToggleSameEverywhere(toggled_on)) => {
                let default_pack = self.assignment.available_packs.first().cloned();
                pages::assignment::set_same_everywhere_enabled(&mut self.assignment.current_config, toggled_on, default_pack);
                if let Err(e) = self.assignment.current_config.save(&self.renderer_config_store) {
                    tracing::error!(error = %e, "failed to save same-pack-everywhere toggle");
                }
            }
            Message::Assignment(pages::assignment::Message::SameEverywherePackSelected(index)) => {
                if let Some(source) = self.assignment.available_packs.get(index).cloned() {
                    pages::assignment::apply_assignment(&mut self.assignment.current_config, &pages::assignment::AssignTarget::SameEverywhere, source);
                    if let Err(e) = self.assignment.current_config.save(&self.renderer_config_store) {
                        tracing::error!(error = %e, "failed to save same-pack-everywhere assignment");
                    }
                }
            }
            Message::Assignment(pages::assignment::Message::OutputPackSelected(output, index)) => {
                if let Some(source) = self.assignment.available_packs.get(index).cloned() {
                    pages::assignment::apply_assignment(&mut self.assignment.current_config, &pages::assignment::AssignTarget::Output(output), source);
                    if let Err(e) = self.assignment.current_config.save(&self.renderer_config_store) {
                        tracing::error!(error = %e, "failed to save output assignment");
                    }
                }
            }
            Message::Location(pages::location::Message::SelectMode(mode)) => {
                pages::location::set_mode(&mut self.location.entry, mode);
                if let Err(e) = self.location.entry.save(&self.location_config_store) {
                    tracing::error!(error = %e, "failed to save location mode");
                }
            }
            Message::Location(pages::location::Message::ToggleIpDisclosure) => {
                self.location.show_ip_disclosure = !self.location.show_ip_disclosure;
            }
            Message::Location(pages::location::Message::LatitudeChanged(v)) => {
                self.location.latitude_input = v;
            }
            Message::Location(pages::location::Message::LongitudeChanged(v)) => {
                self.location.longitude_input = v;
            }
            Message::Location(pages::location::Message::SetManualLocation) => {
                let parsed = self.location.latitude_input.parse::<f64>().and_then(|lat| self.location.longitude_input.parse::<f64>().map(|lon| (lat, lon)));
                if let Ok((lat, lon)) = parsed {
                    match Location::new(lat, lon) {
                        Ok(location) => {
                            pages::location::set_manual_location(&mut self.location.entry, location);
                            if let Err(e) = self.location.entry.save(&self.location_config_store) {
                                tracing::error!(error = %e, "failed to save manual location");
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "invalid manual location input"),
                    }
                }
            }
            Message::Timeline(pages::timeline::Message::Refresh) => {
                self.timeline = self.load_timeline();
            }
            Message::Crossfade(pages::crossfade::Message::DurationChanged(value)) => {
                pages::crossfade::set_duration(&mut self.crossfade.current_config, value.round() as u32);
                if let Err(e) = self.crossfade.current_config.save(&self.renderer_config_store) {
                    tracing::error!(error = %e, "failed to save crossfade duration");
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::ModeChosen(mode)) => {
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_mode(state, mode);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::Cancelled) => {
                // FR-019: dropping the wizard state is the entire cancel operation —
                // nothing was ever written to the source folder before Generate runs.
                self.pack_builder = None;
            }
            Message::PackBuilder(pages::pack_builder::Message::SolarEventSelected(row, index)) => {
                let location = self.current_location();
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_solar_event_by_index(state, row, index, location);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::SolarOffsetSignToggled(row)) => {
                let location = self.current_location();
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::toggle_solar_offset_sign(state, row, location);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::SolarOffsetHoursChanged(row, hours)) => {
                let location = self.current_location();
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_solar_offset_hours(state, row, hours, location);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::SolarOffsetMinutesChanged(row, minutes)) => {
                let location = self.current_location();
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_solar_offset_minutes(state, row, minutes, location);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::TimeHourChanged(row, hour)) => {
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_time_hour(state, row, hour);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::TimeMinuteChanged(row, minute)) => {
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_time_minute(state, row, minute);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::AuthorChanged(author)) => {
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_author(state, author);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::GenerateRequested) => {
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::generate(state);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::MoveRequested) => {
                let closed = self
                    .pack_builder
                    .as_mut()
                    .map(|state| pages::pack_builder::confirm_move(state, &mut self.pack_registry))
                    .unwrap_or(false);
                if closed {
                    self.pack_builder = None;
                    self.packs = pages::packs::State::load(&mut self.pack_registry);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::KeepRequested) => {
                let closed = self
                    .pack_builder
                    .as_mut()
                    .map(|state| pages::pack_builder::confirm_keep(state, &mut self.pack_registry))
                    .unwrap_or(false);
                if closed {
                    self.pack_builder = None;
                    self.packs = pages::packs::State::load(&mut self.pack_registry);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::CollisionNameChanged(name)) => {
                if let Some(state) = self.pack_builder.as_mut() {
                    pages::pack_builder::set_collision_name(state, name);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::CollisionConfirmed) => {
                let closed = self
                    .pack_builder
                    .as_mut()
                    .map(|state| pages::pack_builder::confirm_collision_move(state, &mut self.pack_registry))
                    .unwrap_or(false);
                if closed {
                    self.pack_builder = None;
                    self.packs = pages::packs::State::load(&mut self.pack_registry);
                }
            }
            Message::PackBuilder(pages::pack_builder::Message::CollisionCancelled) => {
                let closed = self
                    .pack_builder
                    .as_mut()
                    .map(|state| pages::pack_builder::cancel_collision_to_keep(state, &mut self.pack_registry))
                    .unwrap_or(false);
                if closed {
                    self.pack_builder = None;
                    self.packs = pages::packs::State::load(&mut self.pack_registry);
                }
            }
        }
        Task::none()
    }

    /// The removal confirmation dialog (spec 008 US1, research.md R3) — rendered as a
    /// modal overlay exactly when `packs.pending_removal.is_some()`, titled with the
    /// pack's resolved name so the confirmation itself never shows a raw path either.
    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        // Spec 010 (Custom Pack Builder): the placement/collision modal takes priority
        // — it can only be showing while the wizard itself owns the main view (`view`
        // below), so there's no real ordering ambiguity with the removal dialog below.
        if let Some(state) = &self.pack_builder {
            if let Some(dialog) = pages::pack_builder::placement_dialog(state) {
                return Some(dialog.map(Message::PackBuilder));
            }
        }
        let source = self.packs.pending_removal.as_ref()?;
        let name = pack_display::resolve_pack_name(source).unwrap_or_else(|| "(unnamed pack)".to_string());
        Some(
            widget::dialog()
                .title("Remove pack?")
                .body(format!("Remove {name}? This cannot be undone."))
                .primary_action(
                    widget::button::destructive("Remove")
                        .on_press(Message::Packs(pages::packs::Message::RemoveConfirmed)),
                )
                .secondary_action(
                    widget::button::standard("Cancel")
                        .on_press(Message::Packs(pages::packs::Message::RemoveCancelled)),
                )
                .into(),
        )
    }

    fn view(&self) -> Element<'_, Self::Message> {
        if let Some(state) = &self.pack_builder {
            return pages::pack_builder::view(state).map(Message::PackBuilder);
        }
        match self.nav_model.active_data::<Page>().copied() {
            Some(Page::Packs) | None => pages::packs::view(&self.packs).map(Message::Packs),
            Some(Page::Assignment) => pages::assignment::view(&self.assignment).map(Message::Assignment),
            Some(Page::Location) => pages::location::view(&self.location).map(Message::Location),
            Some(Page::Timeline) => pages::timeline::view(&self.timeline).map(Message::Timeline),
            Some(Page::Crossfade) => pages::crossfade::view(&self.crossfade).map(Message::Crossfade),
        }
    }
}
