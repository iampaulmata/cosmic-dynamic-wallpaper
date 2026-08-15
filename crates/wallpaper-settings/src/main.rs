//! `wallpaper-settings` — standalone libcosmic GUI for the Cosmic Dynamic Wallpaper daemon
//! (spec 7 US1, contracts/gui-application.md). A `cosmic::Application`, not a
//! `cosmic-settings` panel (spec.md Clarifications, research.md R1 — COSMIC has no
//! general third-party settings-panel extension mechanism). See `README.md` for scope.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod app;
mod pack_display;
mod pages;

fn main() {
    tracing_subscriber::fmt::init();

    let settings = cosmic::app::Settings::default().size(cosmic::iced::Size::new(900.0, 700.0));
    if let Err(e) = cosmic::app::run::<app::App>(settings, ()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
