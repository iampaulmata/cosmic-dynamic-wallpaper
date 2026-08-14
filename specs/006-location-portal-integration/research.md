# Phase 0 Research: Location Portal Integration

## R1: PRD Open Question OQ-1 — does `xdg-desktop-portal-cosmic` implement the Location portal? (live-verified, not assumed)

**Decision**: Yes. `org.freedesktop.portal.Location` is genuinely implemented, not merely
declared.

**Evidence** (gathered live against this project's own dev machine — a real, running COSMIC
session, `cosmic-comp` + `xdg-desktop-portal-cosmic` both active, per this project's established
"check first, don't assume" practice):

```console
$ ps aux | grep xdg-desktop-portal
paul  2253  /usr/libexec/xdg-desktop-portal
paul  2393  /usr/libexec/xdg-desktop-portal-gtk
paul  2667  /usr/libexec/xdg-desktop-portal-cosmic

$ busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop \
    org.freedesktop.portal.Location
NAME              TYPE      SIGNATURE RESULT/VALUE FLAGS
.CreateSession    method    a{sv}     o            -
.Start            method    osa{sv}   o            -
.version          property  u         1            emits-change
.LocationUpdated  signal    oa{sv}    -            -

$ busctl --user call org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop \
    org.freedesktop.portal.Location CreateSession a{sv} 2 \
    session_handle_token s wpspike distinct_name_token s wpspike
Call failed: Location services disabled
```

The interface being *listed* in introspection is not, by itself, proof of a real backend — every
portal interface `xdg-desktop-portal` knows about is listed regardless of what actually answers
it. The proof is the **live `CreateSession` call**: it did not fail with a generic
`org.freedesktop.DBus.Error.UnknownMethod` / `ServiceUnknown`-style error (what an unimplemented
interface produces) — it reached real backend logic and returned a specific, meaningful business
error, `"Location services disabled"`. That string is a deliberate application-level response,
not a D-Bus plumbing failure.

**Conclusion for this spec**: OQ-1 is resolved factually, not just designed around. Spec.md's
Clarifications section already chose to write FR-005 "portal-implementation-agnostic" so the spec
wouldn't have to wait on this answer — that turned out to be the right call for a different
reason than expected: the portal **is** implemented, but it has its own gating (a "Location
services" enabled/disabled state, R2) independent of whether any individual app has permission.
FR-005's degrade path still needs to exist and still needs to trigger cleanly on this exact error
shape.

## R2: Is GeoClue2 itself present? (live-verified)

**Decision**: No — not on this dev machine, and this matters for how FR-005's degrade path gets
exercised in practice, not just in principle.

**Evidence**:

```console
$ busctl call org.freedesktop.GeoClue2 /org/freedesktop/GeoClue2/Manager \
    org.freedesktop.DBus.Introspectable Introspect
Call failed: The name is not activatable

$ systemctl status geoclue
Unit geoclue.service could not be found.

$ find / -iname "*geoclue*"
# only locale .mo files and a Flatpak SDK runtime's dev headers/libs — no system geoclue
# package, no geoclue-2.0 binary, no D-Bus service-activation file
```

**Interpretation**: `xdg-desktop-portal-cosmic`'s `"Location services disabled"` response came
back *before* it would have needed to reach GeoClue2 at all — it's most likely a portal-level
(or system settings-level) toggle checked first, independent of backend presence. Either way, the
net effect for this project is identical: **on a real, live COSMIC system, enabling automatic
location today produces exactly the FR-005 degrade path**, not a hang or a crash. This is the
single most valuable finding from this research pass — the failure path this spec spends real
requirement text on (FR-005, User Story 2) is not a hypothetical edge case, it's the default
outcome on at least this reference machine.

**Cross-spec note (not applied here)**: spec 5 (session integration & packaging, already
planned) does not list a GeoClue dependency in its Debian packaging metadata. A `Recommends:
geoclue-2.0` (soft dependency — never a hard `Depends`, since FR-005 requires working correctly
without it) would let a fresh install offer automatic location out of the box on distros that
carry it, without breaking installs that don't. Flagged for visibility per this project's
established practice of surfacing cross-spec findings explicitly rather than silently absorbing
or ignoring them (spec 3 FR-16, spec 4's two cross-spec gaps) — not applied to spec 5's already-
written artifacts by this plan.

## R3: Rust crate for the portal client

**Decision**: [`ashpd`](https://docs.rs/ashpd) 0.13.13, added to `crates/renderer` only, with
`default-features = false, features = ["location", "async-io"]`.

**Rationale**: `ashpd` is the de-facto standard Rust wrapper for XDG desktop portals (used by
GNOME/COSMIC-adjacent Rust applications generally) — it already implements the
`CreateSession`/`Start`/session-handle-token/request-object dance the raw portal protocol
requires, including the async request/response correlation pattern every portal call uses. Its
`location` feature gates exactly the module needed
(`ashpd::desktop::location::{LocationProxy, CreateSessionOptions, Accuracy}`, confirmed present
via docs.rs for 0.13.13). Critically, its `async-io` feature switches its internal `zbus`
dependency (itself `zbus ^5.13`, compatible with this workspace's already-pinned `zbus = "5"`) to
the same non-`tokio` executor backend `wallpaperd`/`wallpaperctl` already standardize on
(`crates/renderer/Cargo.toml`, `crates/wallpaperctl/Cargo.toml` both use
`features = ["async-io", ...]` already) — `default-features = false` is required specifically to
drop `ashpd`'s own `default = ["tokio"]` feature, confirmed via its docs.rs feature-flag listing.

**Alternatives considered**:
- **Hand-rolled `zbus::Proxy` against `org.freedesktop.portal.Location` directly.** Rejected:
  would require re-implementing the generic portal request/session-handle-token protocol
  `ashpd` already gets right, for no benefit — this project already made the opposite call for
  its own D-Bus *server* (spec 3 gap 3 chose `zbus` over nothing) and for portal *client* work
  specifically, a portal-aware wrapper is the more direct analogue, not a step back to raw
  D-Bus.
- **Bypass the portal, talk to GeoClue2 directly** (e.g. via `geoclue-rs` bindings or a hand-
  rolled `org.freedesktop.GeoClue2` client). Rejected: this defeats the entire reason FR-10 names
  the *portal* specifically rather than GeoClue — the portal is what presents the permission/
  consent flow (spec.md FR-003: "this project MUST NOT implement its own separate consent UI").
  Going straight to GeoClue2 would also fail identically on this dev machine (R2), so it buys
  nothing even as a fallback.

## R4: Requested accuracy level

**Decision**: `Accuracy::City` (from `ashpd::desktop::location::Accuracy`, which also offers
`None`, `Country`, `Neighborhood`, `Street`, `Exact` per its docs.rs listing).

**Rationale**: spec.md's Assumptions call for "city/neighborhood-level accuracy... not GPS-exact
precision." `Accuracy::City` is the closest single named variant and is deliberately the coarser
of the two named in the spec (erring toward less precision, consistent with the spec's own
data-minimization framing in FR-004). Sub-city precision doesn't meaningfully change sunrise/
sunset timing beyond spec 1's already-accepted ~3-minute accuracy band (spec 1's SC-002).

## R5: Event-loop integration model

**Decision**: Drive the portal session (session creation, the initial `Start` call, and the
ongoing `LocationUpdated` signal stream) inside `wallpaperd`'s existing single `calloop` event
loop, using the same `internal_executor(false)` + `calloop`'s `block_on` pattern already
established for the D-Bus service (spec 3 gap 3, `dbus_service.rs`) — not a dedicated OS thread.

**Rationale**: `ashpd`'s `LocationProxy` opens its own `zbus::Connection` internally (a second
D-Bus connection alongside the existing server connection — normal and expected; multiple
connections to the same session bus are routine), but since it's built on the same `async-io`-
backed `zbus`, its futures can be polled by the same foreign-future-driving mechanism
`dbus_service.rs` already uses, rather than introducing a second concurrency model. Concretely:
`portal_location.rs` exposes an async function that creates the session, calls `Start`, and
returns the `LocationUpdated` stream; `wallpaperd.rs` advances that stream (alongside the D-Bus
server's own executor and the existing Wayland/frame/timer `calloop` sources) on every loop tick.

**Alternative rejected**: a dedicated background `std::thread` running its own executor, forwarding
resolved locations to `WallpaperDaemon` via a `calloop::channel`. Simpler to write in isolation,
but this project has deliberately kept `wallpaperd` single-threaded and single-loop through spec
3's entire live-verified implementation (including its D-Bus service) — introducing a second
concurrency model here for a similarly-shaped async I/O source has no offsetting benefit.

## R6: Resolution timeout and retry/backoff policy

**Decision**: A 5-second timeout wraps each resolution attempt (initial, or any attempt following
a prior failure); on timeout or error, FR-005's immediate-degrade applies (already resolved, no
grace period). Background retry uses exponential backoff starting at 30 seconds, capped at 5
minutes, so a transient failure recovers automatically without the user needing to manually
disable/re-enable automatic mode — satisfying spec.md's Edge Cases bullet ("retries with a sane
backoff rather than looping tightly") concretely.

**Rationale**: The 5-second resolution timeout is a distinct concern from spec 3 FR-007's
2-second *reaction* bound (how fast a config change is picked up) — this is "how long to wait for
an external service before giving up," a new bound this spec owns. 5 seconds is generous enough
for a real GeoClue lookup (which can involve a Wi-Fi/cell-based resolution taking a few seconds
on first call) without leaving a solar-anchored pack in limbo indefinitely. The backoff range
(30s–5min) avoids hammering a portal that has already said "disabled" or errored, while still
being frequent enough that a user who fixes the underlying issue (enables location services,
installs GeoClue) sees automatic mode self-recover within a few minutes rather than needing to
notice and manually retry.

**Alternatives considered**: a fixed retry interval (e.g. always 60s) — rejected, doesn't back
off from a persistently-disabled state the way exponential backoff does, and this project has no
existing precedent for a fixed-interval retry loop to match instead. Retry-on-next-daemon-
restart only (no background retry at all) — rejected, would make "install GeoClue and enable
location services" require a manual `wallpaperd` restart or CLI toggle to notice, a worse
experience than automatic recovery for no simplicity gain (the backoff loop itself is a single
`calloop` timer, not meaningfully more complex).

## R7: v1→v2 migration mechanics (verified against vendored `cosmic-config` source)

**Decision**: No hand-written migration function is needed. Bump `#[version = 2]` on
`LocationConfigEntry` and give it a `Default` impl matching data-model.md's Migration mapping
(`mode: Manual`, `automatic_location: None`, `automatic_status: Unresolved`) — `cosmic-config`'s
own versioned-directory mechanism does the rest.

**Evidence** (read directly from the vendored source at
`~/.cargo/git/checkouts/libcosmic-*/cosmic-config/src/lib.rs`, not assumed from the derive
macro's surface alone): `Config::new(name, version)` builds a `previous: Option<Box<Config>>`
chain down through every earlier version's directory (`new_inner`'s `look_for_previous`), and
`ConfigGet::get_local` falls back to `self.previous.get_local(key)` whenever the current
version's directory doesn't have that key's file. Since the derive macro
(`cosmic-config-derive/src/lib.rs`) reads each field by its own name as an independent key, this
means: `location` (same name, same type, in v1 and v2) is read from the v2 store first and
**automatically** falls back to the existing v1 value with zero new code, while `mode`/
`automatic_location`/`automatic_status` (new in v2, absent from v1) simply aren't found anywhere
in the chain and fall back to `Default::default()`'s value on that field — exactly this spec's
intended migration mapping, for free.

This also matches every existing `CosmicConfigEntry` type in this workspace (`RendererConfig`,
v1 `LocationSource`) already relying on the same `Self::get_entry(config).unwrap_or_else(|(_errs,
default), default)` (`::load()`) convention (`crates/renderer/src/config.rs`) to tolerate
missing/errored individual keys — this spec's v2 type continues that exact convention rather than
introducing new error-handling.

**Correction to earlier framing**: plan.md/data-model.md/contracts/location-config-schema-v2.md
originally described this as requiring "a documented migration function" (constitution Principle
X's literal wording). Corrected here to "a documented migration **behavior**" — Principle X's
actual requirement (a schema change MUST NOT silently misinterpret an old value, and MUST be
documented) is still met; it's just met by `cosmic-config`'s existing built-in mechanism plus a
correct `Default` impl, not new imperative migration code this spec has to write and test as a
separate function.

## R8: `wallpaperctl location` CLI surface shape

**Decision**: Two new subcommands, `location auto` (enable automatic mode, idempotent — FR-001/
002/003) and `location manual` (switch back to manual using whatever value is already stored, no
re-entry — FR-007/009), alongside the existing `get`/`set`/`clear`. `get`'s output gains `mode`
and `status` (Location Availability Status, spec.md Key Entities) fields alongside the existing
`location`, remaining fully daemon-optional (reads only `cosmic-config`, per the spec's
Clarifications decision to persist the resolved automatic value — FR-010/FR-012).

**Rationale**: Matches spec 4's existing subcommand-per-action shape (`get`/`set`/`clear`) rather
than inventing a single `--mode` flag on an existing command, keeping each action independently
discoverable via `--help` (spec 4's own established CLI idiom). `set <lat> <lon>` continues to
both persist the manual value *and* switch mode to `Manual` — documented as a deliberate default
(data-model.md): setting a manual value while remaining in automatic mode would silently do
nothing observable, which is a worse default than the explicit switch. `clear` continues to only
remove the manual value, leaving `mode` untouched — since spec 4 already established `clear`'s
meaning as "no manual location," expanding its scope to also flip automatic mode would be a
surprising side effect for existing users of that command.

**Alternative considered**: a single `location set --mode auto|manual [lat] [lon]` command.
Rejected: forces every invocation through one large flag surface instead of two small, clearly-
named ones, and doesn't match this project's existing `wallpaperctl` subcommand granularity
(register/list/remove/assign/location/query/reevaluate are all already separate subcommands per
action, not flag-differentiated).
