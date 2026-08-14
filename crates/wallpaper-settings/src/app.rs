//! The `cosmic::Application` implementation — a single window, sidebar navigation
//! between the five pages (contracts/gui-application.md). Every page reads/writes the
//! identical `cosmic-config`/D-Bus state `wallpaperctl`/`wallpaperd` already do, via
//! `wallpaper_ipc`/`pack_loader` directly (FR-007: enforced structurally, not by
//! convention — plan.md Constitution Check finding 1).

use cosmic::app::Task;
use cosmic::widget::nav_bar;
use cosmic::{executor, Core, Element};
use schedule_engine::Location;
use wallpaper_ipc::{LocationConfigEntry, RendererConfig};

use crate::pages;

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
}

#[derive(Debug, Clone)]
pub enum Message {
    Packs(pages::packs::Message),
    Assignment(pages::assignment::Message),
    Location(pages::location::Message),
    Timeline(pages::timeline::Message),
    Crossfade(pages::crossfade::Message),
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
        let renderer_config = RendererConfig::load(&self.renderer_config_store);
        let known_outputs = wallpaper_ipc::DbusClient::connect().and_then(|c| c.query_all()).map(|entries| entries.into_iter().map(|e| e.output).collect()).unwrap_or_default();
        let available_packs = self.pack_registry.known_packs().into_iter().map(|e| e.source).collect();
        self.assignment = pages::assignment::State { known_outputs, available_packs, current_config: renderer_config };
    }

    fn refresh_active_page(&mut self) {
        match self.nav_model.active_data::<Page>().copied() {
            Some(Page::Packs) => self.packs = pages::packs::State::load(&mut self.pack_registry),
            Some(Page::Assignment) => self.refresh_assignment(),
            Some(Page::Location) => self.location = pages::location::State::load(LocationConfigEntry::load(&self.location_config_store)),
            Some(Page::Timeline) => self.timeline = pages::timeline::State::load(),
            Some(Page::Crossfade) => self.crossfade = pages::crossfade::State { current_config: RendererConfig::load(&self.renderer_config_store) },
            None => {}
        }
    }
}

impl cosmic::Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.system76.CosmicWallpaperSettings";

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
        let renderer_config = RendererConfig::load(&renderer_config_store);
        let known_outputs = wallpaper_ipc::DbusClient::connect().and_then(|c| c.query_all()).map(|entries| entries.into_iter().map(|e| e.output).collect()).unwrap_or_default();
        let available_packs = pack_registry.known_packs().into_iter().map(|e| e.source).collect();
        let assignment = pages::assignment::State { known_outputs, available_packs, current_config: renderer_config.clone() };
        let location = pages::location::State::load(LocationConfigEntry::load(&location_config_store));
        let timeline = pages::timeline::State::load();
        let crossfade = pages::crossfade::State { current_config: renderer_config };

        let app = App { core, nav_model, pack_registry, renderer_config_store, location_config_store, packs, assignment, location, timeline, crossfade };
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
            Message::Assignment(pages::assignment::Message::AssignFirstPackToOutput(output)) => {
                if let Some(source) = self.assignment.available_packs.first().cloned() {
                    pages::assignment::apply_assignment(
                        &mut self.assignment.current_config,
                        &pages::assignment::AssignTarget::Output(output),
                        source,
                    );
                    if let Err(e) = self.assignment.current_config.save(&self.renderer_config_store) {
                        tracing::error!(error = %e, "failed to save output assignment");
                    }
                }
            }
            Message::Assignment(pages::assignment::Message::SetFirstPackSameEverywhere) => {
                if let Some(source) = self.assignment.available_packs.first().cloned() {
                    pages::assignment::apply_assignment(&mut self.assignment.current_config, &pages::assignment::AssignTarget::SameEverywhere, source);
                    if let Err(e) = self.assignment.current_config.save(&self.renderer_config_store) {
                        tracing::error!(error = %e, "failed to save same-pack-everywhere assignment");
                    }
                }
            }
            Message::Location(pages::location::Message::SelectMode(mode)) => {
                pages::location::set_mode(&mut self.location.entry, mode);
                if let Err(e) = self.location.entry.save(&self.location_config_store) {
                    tracing::error!(error = %e, "failed to save location mode");
                }
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
                self.timeline = pages::timeline::State::load();
            }
            Message::Crossfade(pages::crossfade::Message::DurationChanged(value)) => {
                pages::crossfade::set_duration(&mut self.crossfade.current_config, value.round() as u32);
                if let Err(e) = self.crossfade.current_config.save(&self.renderer_config_store) {
                    tracing::error!(error = %e, "failed to save crossfade duration");
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self.nav_model.active_data::<Page>().copied() {
            Some(Page::Packs) | None => pages::packs::view(&self.packs).map(Message::Packs),
            Some(Page::Assignment) => pages::assignment::view(&self.assignment).map(Message::Assignment),
            Some(Page::Location) => pages::location::view(&self.location).map(Message::Location),
            Some(Page::Timeline) => pages::timeline::view(&self.timeline).map(Message::Timeline),
            Some(Page::Crossfade) => pages::crossfade::view(&self.crossfade).map(Message::Crossfade),
        }
    }
}
