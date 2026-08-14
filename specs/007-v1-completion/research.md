# Phase 0 Research: V1 Completion

Organized by the spec's own four areas (GUI, starter pack, IP-geolocation, gap closure). Two
findings are flagged prominently rather than buried — R2 (a real architecture correction to
already-shipped code) and R6 (a real tension between the user's own privacy-motivated
Clarification choice and what's technically possible) — matching this project's established
practice of surfacing real findings rather than absorbing them silently (spec 3's FR-16, spec
5's cosmic-bg finding, spec 6's OQ-1 spike).

## R1: GUI application framework

**Decision**: `libcosmic` (the `pop-os/libcosmic` git repository, same pin already used for
`cosmic-config`), using its `cosmic::app::Application` trait and `cosmic::app::run` entry point
— the same framework `cosmic-files`/`cosmic-edit`/`cosmic-settings` itself are built with.

**Evidence**: the workspace's existing git checkout of `pop-os/libcosmic`
(`~/.cargo/git/checkouts/libcosmic-*/`, already fetched for `cosmic-config`) is the same
monorepo containing the full `libcosmic` UI crate at its root (`src/app/`, `src/applet/`,
`iced/` — a vendored/patched `iced` fork), confirming this project can pin the new GUI crate to
the exact same commit already used elsewhere in the workspace, with no new external repository
to trust.

**Rationale**: this is the only choice compatible with constitution Principle IX ("any settings
GUI MUST be built with libcosmic widgets and the shared COSMIC theme tokens — not GTK, Qt, or a
raw web view").

## R2: ⚠️ Shared schema/IPC crate — a real architecture correction to already-shipped code

**Decision**: extract a new, lightweight workspace crate, `crates/wallpaper-ipc`, holding the
`cosmic-config` schema types currently *independently defined* in two places
(`RendererConfig`/`OutputAssignment` in `crates/renderer/src/output.rs`+`config.rs`;
`LocationConfigEntry`/`LocationMode`/`AutomaticStatus` independently mirrored in both
`crates/renderer/src/config.rs` and `crates/wallpaperctl/src/config.rs`) plus the D-Bus client
currently living only in `crates/wallpaperctl/src/dbus_client.rs`. `crates/renderer`,
`crates/wallpaperctl`, and this spec's new GUI crate all depend on `wallpaper-ipc` instead of
each defining or re-deriving their own copy.

**Why this matters enough to flag**: this project has already been bitten by exactly this class
of bug once — `crates/renderer/src/config.rs`'s own doc comment records a real, previously-live
issue where `RendererConfig.overrides` being independently typed in two crates (`HashMap<OutputId,
PackSource>` vs. `wallpaperctl`'s `HashMap<String, PackSource>`) silently produced an **empty
map** at runtime, caught only by a live round-trip test against a real `wallpaperctl`-written
config. Spec.md's own FR-007 ("The GUI and the existing CLI MUST remain interchangeable,
never-conflicting control surfaces over the same persisted state") is exactly the property a
third independently-typed copy would put at risk again — introducing a GUI is the natural moment
to fix this at the root rather than add a third copy for the same bug class to eventually find.

**Rationale for a new crate, not just "renderer exports its types"**: `crates/renderer`'s
`Cargo.toml` unconditionally depends on `wgpu`, `smithay-client-toolkit`, `wayland-client`,
`calloop-wayland-source` — real, heavy GPU/Wayland dependencies. Spec 4's plan.md *explicitly*
chose to keep `wallpaperctl` free of exactly these dependencies ("this crate deliberately does
not depend on spec 3's `renderer` crate as a Rust library"). A new, minimal crate (`serde`,
`cosmic-config`, `zbus`, path deps on `schedule-engine`/`pack-loader` only) preserves that
property for both `wallpaperctl` and the new GUI, while `renderer` itself becomes a *consumer* of
`wallpaper-ipc` rather than the sole definer.

**Alternatives considered**: leaving three independently-typed copies (status quo, rejected —
the exact bug class above, now with three chances instead of two to drift); making `renderer`'s
existing config module a default-off Cargo feature other crates enable (rejected — still forces
`wallpaperctl`/GUI's `Cargo.toml` to list `renderer` as a dependency at all, which is fragile
against someone later making an already-optional heavy dependency accidentally unconditional).

## R3: IP-geolocation dataset and reader crate

**Decision**: [DB-IP Lite](https://db-ip.com/db/lite.php) (CC-BY-4.0 licensed, monthly-updated,
published in the MaxMind-compatible `.mmdb` binary format), read via the
[`maxminddb`](https://docs.rs/maxminddb) crate (0.30.x — format-compatible with both MaxMind's
and DB-IP's `.mmdb` files, confirmed via its docs.rs listing).

**Rationale**: resolves this spec's Clarification (a bundled, periodically-updated offline
database, never a live third-party API call) concretely. DB-IP Lite was chosen over MaxMind's own
GeoLite2 specifically because GeoLite2 requires a free account and a license key just to
*download* the database (a real friction/dependency for a build pipeline that shouldn't require
a third-party account to produce a package), while DB-IP Lite is a plain CC-BY-4.0 download with
attribution as the only obligation — a materially better fit for "bundled, no account needed."
"Periodically-updated" (per the Clarification's own wording) means this project's release process
re-downloads a fresh DB-IP Lite snapshot before cutting a packaging build — not a runtime update
mechanism inside `wallpaperd` itself (out of scope; see data-model.md).

**Alternatives considered**: MaxMind GeoLite2 (rejected — account/license-key friction above);
a live third-party API such as ip-api.com (rejected — this is exactly what the Clarification
chose against); IP2Location LITE (a comparable free offering, not chosen only because DB-IP's
`.mmdb`-format compatibility with the already-selected `maxminddb` crate is more direct — noted
as a reasonable alternative if licensing terms ever change).

## R4: ⚠️ Discovering your own public IP address — the real tension with "no third-party call"

**Real technical finding, flagged prominently rather than silently resolved**: a bundled offline
database (R3) fully removes the need to send a live *geolocation* query anywhere — but it does
**not**, by itself, tell `wallpaperd` what its own public-facing IP address is. Most home/office
users sit behind NAT; the machine's own network interface only knows its private
(`192.168.x.x`-style) address, which the offline database cannot geolocate at all. Discovering a
NAT'd machine's actual public IP conventionally requires *some* form of external touchpoint —
there is no way around this that works for the general case, only ways to make that touchpoint
smaller.

**Decision**: use [STUN](https://www.rfc-editor.org/rfc/rfc5389) (RFC 5389), via the
[`stunclient`](https://docs.rs/stunclient) crate (0.4.2, confirmed available) — not a "what's my
IP" HTTP API. A STUN request's entire purpose is "what address did this packet appear to come
from," and a well-behaved STUN server learns nothing else (no request path, no headers, no
apparent intent) — a narrower, more purpose-built exception than any HTTP-based "geolocation" or
"my IP" service, and a widely-used pattern in other privacy-conscious networking tools (most
VPN/NAT-traversal tooling already does exactly this). The result is cached with a long TTL (this
spec proposes 24 hours, or invalidated earlier by an explicit network-change signal if one is
cheaply available) rather than re-queried per solar-event resolution, so this touchpoint happens
at most a few times a day, not on every scheduling decision.

**This is presented to the user, not silently built**: FR-014 already requires the resolution
mechanism to be "documented to the user before they opt in" — the plan is to state this
distinction plainly in the GUI/CLI's own IP-geolocation opt-in copy ("uses a bundled offline
database for the actual location lookup; briefly asks a STUN server what your public IP address
is, since that's not something a bundled database can tell you on its own"), so a user opting in
understands the one remaining external touchpoint rather than assuming zero network activity
ever occurs. **This finding is called out explicitly in this plan's completion report** for the
user to weigh in on, since "bundled offline database" was chosen specifically to avoid an
external call, and this is the one narrow exception discovered during planning, not during
implementation.

**Alternatives considered**: skip public-IP discovery, use only the local interface address
(rejected — geolocates a private address to nothing useful for the large majority of NAT'd home
users, defeating the feature); a plain HTTPS "what's my IP" endpoint (rejected — a full HTTP
request discloses more than a STUN packet: TLS handshake metadata, a User-Agent if not
deliberately stripped, and is indistinguishable in principle from a "call an API" pattern the
Clarification specifically chose against); IPv6-only (rejected as the *sole* mechanism — some
ISPs assign a globally-routable IPv6 address a machine can read directly with zero external
touchpoint at all, which is worth using as a preferred fast-path when available, but doesn't work
for the many networks that are IPv4-only or use IPv6 with privacy extensions that rotate
addresses — kept as a documented optimization in data-model.md, not the only path).

## R5: Starter pack content generation

**Decision**: a small, **not shipped**, one-time-run internal generator tool
(`tools/generate-starter-pack`, a plain Rust binary using the `image` crate already used
elsewhere in this workspace) produces a fixed set of static PNG images (a gradient sky sequence
spanning a solar-anchored day cycle) checked into the repository under `assets/starter-pack/`
alongside a hand/script-authored `manifest.toml` (spec 2's existing pack format, reused
unchanged). The generator is a build-time/maintainer tool, never a runtime dependency of
`wallpaperd`/`wallpaperctl`/the GUI.

**Rationale**: keeps every shipped binary free of image-generation code and dependencies it would
otherwise carry forever for a one-time task; the resulting pack is exercised by spec 2's already-
existing loader/validator exactly like any user-authored pack — no new pack-format code needed
anywhere. "Procedurally generated" (the Clarification's own wording) describes how the *content*
was produced, not that generation must happen live on every user's machine.

**Alternatives considered**: generate at install time via a `postinst` script (rejected — adds a
runtime image-generation dependency to the packaging layer for a fixed, deterministic output that
gains nothing from being regenerated per install); generate at `wallpaperd` first-run (rejected,
same reasoning, and would also delay "immediately visible" in SC-002).

## R6: Starter pack registration and permanent-removal tracking

**Decision**: extend spec 2's pack registry schema (`pack-loader`'s `PackRegistryEntry`) with a
new `origin: PackOrigin` field (`User` | `Package`), defaulting to `User` for full backward
compatibility with every existing registry entry (same no-hand-written-migration-needed pattern
already established and verified in spec 6 research.md R7 — a new field with a safe default,
carried automatically by `cosmic-config`'s per-key version fallback). `postinst` (spec 5) registers
the starter pack with `origin: Package`; `wallpaperctl remove`/the GUI's equivalent, when removing
a `Package`-origin entry, additionally records that removal so a later `postinst` run (package
upgrade) checks before re-registering (FR-010).

**Rationale**: this is the minimal schema addition that satisfies FR-010/FR-011/SC-006 without
inventing a parallel "starter pack state" store — it reuses spec 2's existing registry as the
single source of truth, consistent with constitution Principle IV.

## R7: Mock hotplug/output-change test harness

**Decision**: a minimal in-process fake Wayland compositor built on the
[`wayland-server`](https://docs.rs/wayland-server) crate (0.31.x — the same version series as
the `wayland-client` 0.31 `crates/renderer` already depends on), implementing just enough of
`wl_registry`/`wl_output`/`xdg_output`/`wp_fractional_scale_v1` to emit synthetic global-add,
global-remove, and geometry/scale-change events over an in-memory socket pair connected to the
real, unmodified SCTK client code under test (`crates/renderer/src/surface.rs`'s existing output-
handling). Lives as a `crates/renderer/tests/`-only harness gated behind `dev-dependencies`, never
shipped in the `wallpaperd` binary.

**Rationale**: this is a standard, well-established pattern for testing Wayland *client* code
(pairing it against a minimal `wayland-server`-based test double rather than a full compositor)
and closes spec 3 tasks.md's own previously-unimplemented T043 directly, using the exact client
code path production `wallpaperd` runs — not a simulation of the logic, a real exercise of it.

**Alternatives considered**: a hand-rolled fake using raw `wayland-client::backend::Backend`
socket plumbing without `wayland-server` (rejected — reimplements what `wayland-server` already
provides correctly, including protocol (de)serialization); continuing to rely solely on real
hardware for this coverage (status quo, rejected — it's the exact gap this spec exists to close,
and this dev environment's own two real outputs still can't produce a disconnect or resize event
on demand).

## R8: `Recommends: geoclue-2.0` packaging metadata

**Decision**: add `recommends = "geoclue-2.0"` to `crates/renderer/Cargo.toml`'s
`[package.metadata.deb]` section (spec 5's packaging target — `wallpaperd`/`renderer` is where
automatic portal-based location resolution actually happens, spec 6).

**Evidence**: `cargo-deb`'s own documentation confirms `[package.metadata.deb]` supports a
`recommends` field directly, mapping to Debian's `Recommends:` control-file field — exactly the
soft (never hard) dependency spec 6 research.md R2 originally flagged and left unapplied.

## R9: `AutomaticStatus` generalization for a third location mode

**Decision**: rename spec 6's `AutomaticStatus` (`Unresolved`/`Resolved`/`Unavailable { reason }`)
to a mode-agnostic `ResolutionStatus`, reused for both the portal (`automatic_status`) and the new
IP-geolocation (`ip_status`) fields, rather than defining a second, structurally-identical enum.
A small, mechanical rename applied to already-shipped spec 6 code — see data-model.md.
