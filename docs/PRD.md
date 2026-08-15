# PRD — Cosmic Dynamic Wallpaper Daemon

**Status:** Draft, input to `/speckit.specify`
**Governs under:** `constitution-principles.md` (11 principles) — every FR below is
implementable without violating any of them; where a requirement exists *because* of a
specific principle, it's cited inline as `[P#]`.

Requirements are numbered (`FR-x`) so individual specs can cite them directly. Each is
tagged `[MVP]` or `[Future]`. Nothing tagged `[Future]` should be pulled into an early
spec just because it's easy — the point of the tag is to keep the first several specs
small and shippable.

---

## 1. Problem & Inspiration

Static wallpapers don't reflect the passage of the day. Several desktops have addressed
this — macOS's Dynamic Desktop, Windows' WinDynamicDesktop, GNOME's slideshow XML
format, and Cinnamon's **Dynamic Wallpaper** extension (TobiZog) — by rotating through a
set of images tied to sunrise/sunset or fixed clock times, optionally using the user's
location. COSMIC has no equivalent; `cosmic-bg`'s slideshow feature only supports
dumb fixed-interval rotation through an unordered image list, with no concept of time-
of-day or a smooth transition between images.

This project is a from-scratch, COSMIC-native answer to the same problem, differentiated
from the Cinnamon extension primarily by:
- **Smooth GPU crossfade** between images at each transition, not a hard cut `[P1][P3]`
- **Astronomically-anchored periods** (civil/astronomical twilight, solar noon), not just
  raw sunrise/sunset with evenly-spaced slots
- **Native libcosmic UI and cosmic-config persistence**, not a bolted-on toolkit `[P4][P9]`

## 2. Goals

- G1: Wallpapers change automatically across the day, following either real solar
  events for the user's location or a fully custom manual schedule.
- G2: Transitions between images are visually smooth (crossfade), not jarring.
- G3: Works correctly across mixed multi-monitor, multi-scale-factor setups.
- G4: Feels like a native part of COSMIC — install, configure, and forget.
- G5: Idle cost (when no transition is happening) is effectively zero.

## 3. Non-Goals (v1)

- NG1: Video, animated GIF, or GPU-shader wallpapers — that's `cosmic-ext-bg`'s territory;
  this project is specifically about *timed still-image* transitions.
- NG2: A curated wallpaper marketplace/store or image-hosting service.
- NG3: Cross-desktop support (GNOME, KDE, etc.) — COSMIC only.
- NG4: Weather-reactive imagery (cloud cover, precipitation) — solar/time-based only.
- NG5: Parsing Apple's `.heic` dynamic-wallpaper metadata format directly.

## 4. Primary User

A COSMIC desktop user who wants their background to visibly track the time of day,
with either zero configuration (pick a pack, done) or full manual control (exact clock
times, no location shared at all). Not assumed to be a Rust developer or willing to
hand-edit config files, even though that path remains available.

---

## 5. Functional Requirements

### 5.1 Wallpaper Packs & Sources

- **FR-1** [MVP] A **wallpaper pack** is the core content unit: an ordered set of images,
  each tagged with a **time anchor** (see FR-6), plus pack-level metadata (name,
  scaling mode default, author/license note).
- **FR-2** [MVP] A pack is loadable from a local directory containing images plus a
  manifest file (schema owned by this project, versioned per `[P10]`).
- **FR-3** [MVP] A **static mode** exists for a single image with no time anchors at
  all — feature parity with "just set a normal wallpaper," since this daemon takes over
  cosmic-bg's role on managed outputs `[P1][P11]` and users still need that baseline case.
- **FR-4** [MVP] Scaling/fit behavior per pack or per image: Fill, Fit, Stretch, Center,
  with a configurable fallback fill color for letterboxed edges — matching the options
  users already expect from cosmic-bg's own scaling modes.
- **FR-5** [Future] A folder of untagged images can be auto-distributed across a chosen
  period model (e.g., evenly spaced across N slots) as a convenience path for users who
  don't want to hand-build a manifest — mirrors Cinnamon's "just point at a folder"
  option. Deferred because FR-1–FR-4 must be solid before auto-distribution logic has
  anything reliable to sit on top of.
- **FR-6** [MVP] Each image's time anchor is one of:
  - a **solar event** name (`sunrise`, `sunset`, `solar_noon`, `solar_midnight`,
    `civil_dawn`, `civil_dusk`, `astronomical_dawn`, `astronomical_dusk`) with an
    optional signed offset (e.g. `sunset-30m`), or
  - an **absolute clock time** (`HH:MM`, for location-free manual schedules).
  A pack must not mix both anchor types within itself — pick one model per pack.
- **FR-7** [Future] Bundled starter pack(s) shipped with the project. Deferred: art
  sourcing/licensing is a separate workstream from the daemon itself and shouldn't
  block or scope-creep the core specs.

### 5.2 Time & Location Model

- **FR-8** [MVP] Solar event times are computed from a location using a vetted
  astronomical algorithm/crate, per `[P5]` — never hand-rolled trigonometry.
- **FR-9** [MVP] Location can be provided as **manual latitude/longitude** entered by
  the user. This is the baseline path and must work with zero external services.
- **FR-10** [MVP] Location can optionally be provided **automatically** via the
  `org.freedesktop.portal.Location` D-Bus portal (backed by GeoClue2), if available.
  This must degrade gracefully to FR-9 if the portal or backend isn't present —
  see Open Question OQ-1 on `xdg-desktop-portal-cosmic` support.
- **FR-11** [MVP] A **fully manual, location-free schedule** is supported: the user
  assigns absolute clock times per image (FR-6's clock-time anchor) with no solar
  calculation involved at all, for users unwilling to share location by any means.
- **FR-12** [Future] IP-geolocation fallback (Cinnamon's default method) when no portal
  is available and the user hasn't entered coordinates. Deferred in favor of FR-10/FR-9
  because it's a weaker privacy story and shouldn't be the default; revisit only if
  user feedback shows FR-9's manual entry is too much friction.

### 5.3 Scheduling & Transitions

- **FR-13** [MVP] At any moment, the daemon can deterministically answer "which image
  is active, and what fraction of the way through the current crossfade (if any) are
  we" — this is the pure, testable core per `[P5]`.
- **FR-14** [MVP] Transitions crossfade over a configurable duration (sane default,
  e.g. 30–60s) using GPU compositing per `[P3]`.
- **FR-15** [MVP] Outside of an active transition, the daemon holds no render loop and
  wakes only at the next scheduled instant, per `[P6]`.
- **FR-16** [MVP] A config or output change (pack swapped, location edited, output
  hotplugged) immediately re-evaluates the current/next transition rather than waiting
  for the next natural wake.

### 5.4 Multi-Output Behavior

- **FR-17** [MVP] Each Wayland output can be assigned an independent pack (or static
  image), independent of other outputs, per `[P7]`.
- **FR-18** [MVP] A "same pack on all outputs" convenience toggle exists, but per-output
  override remains possible underneath it.
- **FR-19** [MVP] Output hotplug (connect/disconnect/resize) and fractional scaling are
  handled without crashing or requiring a daemon restart, per `[P7]`.

### 5.5 Configuration & Control Surfaces

- **FR-20** [MVP] All state is persisted via `cosmic-config`, versioned per `[P10]`.
- **FR-21** [MVP] A CLI control binary exists for scripting: list packs, assign a pack
  to an output, query current/next transition, force an immediate re-evaluation.
- **FR-22** [Future] A libcosmic-native GUI settings app: pack browser with preview,
  per-output assignment, location entry, crossfade duration, a timeline visualization
  of today's schedule, per `[P9]`. Deferred behind the CLI (FR-21) so the underlying
  daemon/config contract is proven before a GUI is built against it.

### 5.6 Import & Compatibility

- **FR-23** [Future] Importer that converts GNOME's time/sun-position background XML
  format into this project's native pack manifest (FR-2), per `[P4]`'s import-only
  clause. One-time conversion, not a runtime-read format.

### 5.7 Packaging & Session Integration

- **FR-24** [MVP] Ships as a systemd user unit / cosmic-session autostart component.
- **FR-25** [MVP] Install flow disables cosmic-bg's background role on outputs this
  daemon takes over; uninstall flow restores a sane default rather than leaving a black
  screen, per `[P1][P11]`.

---

## 6. Non-Functional Requirements

| NFR | Requirement | Constitution ref |
|---|---|---|
| NFR-1 | No panics/`unwrap()` on malformed packs, missing images, or bad config | `[P8]` |
| NFR-2 | Idle CPU/GPU usage effectively zero between transitions | `[P6]` |
| NFR-3 | Crossfade is GPU-composited, tested on integrated graphics (not just dev-machine dGPU) | `[P3]` |
| NFR-4 | Multi-output + mixed-scale-factor scenario covered in CI or documented manual QA before release | `[P7]` |
| NFR-5 | Config schema changes are versioned with a migration path | `[P10]` |
| NFR-6 | No X11 code paths anywhere in the renderer | `[P2]` |

---

## 7. Open Questions / Risks

- **OQ-1:** Does `xdg-desktop-portal-cosmic` currently implement the Location portal
  backend (`org.freedesktop.portal.Location`)? This wasn't confirmed during research —
  it may fall through to no backend at all on COSMIC today. **Action: spike this early**
  (a throwaway D-Bus call against a real COSMIC session) before FR-10 is specced in
  detail, since the fallback-to-manual path (FR-9) needs to be the default assumption
  if the portal isn't there yet, not a fallback bolted on after the fact.
- **OQ-2:** Default crossfade duration and whether it should scale with the length of
  the *next* period (a transition at civil dawn arguably wants to be shorter than a
  slow afternoon-to-evening fade) — needs a decision before FR-14 is specced precisely.
- **OQ-3:** Manifest format specifics (RON vs. TOML vs. JSON for the pack manifest) —
  RON is the natural choice for consistency with cosmic-config, but packs are meant to
  be user-authorable/shareable, and RON is less familiar outside the Rust ecosystem
  than TOML. Worth deciding before FR-2 is specced, since it's annoying to migrate
  pack manifests later on top of already having a config-schema migration story (NFR-5)
  to maintain.

---

## 8. Suggested Spec Breakdown

For feeding into `/speckit.specify`, section 5 groups map reasonably onto separate specs
rather than one monolithic one:

1. **Core scheduling engine** — FR-6, FR-8, FR-9, FR-11, FR-13 (the pure solar/time
   logic, `[P5]`, buildable and fully unit-tested with no Wayland/rendering involved)
2. **Pack format & loading** — FR-1–FR-4, FR-20 (manifest schema, cosmic-config
   integration, resolve OQ-3 first)
3. **Renderer** — FR-14, FR-15, FR-17–FR-19 (the layer-shell client, crossfade,
   multi-output — the highest-risk spec, depends on spec 1 and 2 existing first)
4. **CLI control surface** — FR-21
5. **Session integration & packaging** — FR-24, FR-25
6. **Location portal integration** — FR-10 (kept separate from spec 1 specifically
   because of OQ-1 — don't let an unresolved portal question block the core scheduler)

Everything tagged `[Future]` (FR-5, FR-7, FR-12, FR-22, FR-23) is intentionally left out
of this list — revisit once 1–6 are shipped and real usage tells you which of them
actually matter.
