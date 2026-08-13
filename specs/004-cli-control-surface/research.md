# Research: CLI Control Surface

## R1. CLI argument parsing

**Decision**: [`clap`](https://crates.io/crates/clap) (4.6.x, derive macros) for subcommand
parsing (`wallpaperctl register`, `list packs`, `list outputs`, `remove`, `assign`,
`location get|set|clear`, `query`, `reevaluate`).

**Rationale**: The de facto standard Rust CLI argument parser — derive macros generate typed
subcommand structs directly from FR-001–FR-010's command surface, plus `--help`/`--version`
and shell-completion generation (via the companion `clap_complete` crate, not required by any
FR here but essentially free once `clap`'s derive API is in use). No serious competing choice
exists in the Rust ecosystem for a project already committed to Rust everywhere else.

**Alternatives considered**: Hand-rolled `std::env::args()` parsing — rejected; reimplementing
subcommand dispatch, help text, and error messages that `clap` already provides correctly is
pure risk for a spec whose entire point (FR-012) is "every failure produces a specific,
actionable error."

## R2. Machine-readable output mode

**Decision**: [`serde_json`](https://crates.io/crates/serde_json) for the `--json` (or
equivalent) output mode on every data-returning command (FR-013), with a human-readable
formatted-text default.

**Rationale**: JSON is the most universally tool-supported machine-readable format for CLI
output (`jq` and equivalent tooling exist everywhere), which matters specifically because
FR-013's purpose is letting a *scripted caller* consume this CLI's output without parsing
free text — unlike this project's internal persistence layer (RON via `cosmic-config`,
constitution Principle IV), which is an intentionally separate concern: Principle IV governs
what the *daemon* reads at runtime, not what a script consuming this CLI's stdout expects.
`serde` itself is already deep in this workspace's dependency tree (via `toml` in spec 2,
`cosmic-config` in specs 2–3), so `serde_json` adds no new serialization framework, only a new
output format for types that already derive `Serialize`.

**Alternatives considered**: RON output (matching the project's own internal format) —
rejected as the default for scripting; RON tooling outside the Rust ecosystem is far less
common than JSON's, and FR-013's whole point is *scriptability*, which JSON serves better for
this CLI's actual audience (shell scripts, not necessarily other Rust programs).

## R3. Live daemon IPC for list-outputs, query, and force-reevaluation

**Decision**: [`zbus`](https://crates.io/crates/zbus) (5.x, pure-Rust D-Bus implementation, no
`libdbus` dependency, MSRV 1.87) as the client connecting to a small D-Bus interface exposed
by a running `wallpaperd` (spec 3) on the session bus, for FR-005 (list outputs — corrected
into this bucket, see spec.md Assumptions), FR-009 (query current/next transition), and FR-010
(force re-evaluation).

**Rationale**: Every other persisted-state command (register, list *packs*, remove, assign,
location) is deliberately config-only (research.md R4, spec.md FR-011) — matching
`cosmic-bg`'s own confirmed pattern of being purely config-driven and watch-reactive, not
D-Bus-RPC-driven, for *settings*. But "which outputs currently exist," "what image is active
right now," and "recompute now" are all live, in-process daemon state that `cosmic-config`'s
watch model has no way to expose back to a reader — there is nothing to watch for a value that
only exists in the running daemon's memory (an initial draft of this spec missed this
distinction for "list outputs" specifically; corrected before planning). D-Bus is the standard
mechanism other COSMIC daemons already use for exactly this kind of live interaction
(confirmed during research: COSMIC Settings depends on several COSMIC daemons via D-Bus for
live functionality), and `zbus` is the actively-maintained, idiomatic, pure-Rust D-Bus crate
for exactly this — no C `libdbus` linkage needed, unlike the older `dbus` crate.

**Scope note**: This decision requires `wallpaperd` (spec 3) to actually expose this
interface, which it does not today — see plan.md's Cross-Spec Dependencies and
contracts/wallpaperd-dbus-interface.md for the exact shape this spec depends on.

**Alternatives considered**: A Unix domain socket with a bespoke line/JSON protocol —
rejected; D-Bus is the platform-idiomatic choice other COSMIC components already use for
live daemon interaction, and inventing a second, project-specific IPC protocol has no
advantage over reusing the one the desktop session already provides and every other
`cosmic-*` control surface already assumes is present.

## R4. Persisting manual location (FR-008)

**Decision**: A new `cosmic-config` schema, owned by this crate, for the manual
latitude/longitude (`LocationConfig`) — versioned with its own `schema_version` (constitution
Principle X), separate from spec 2's pack-registry schema and spec 3's `RendererConfig`
schema, following the same "each distinct piece of state gets its own explicit schema" pattern
spec 2 and spec 3 both already established.

**Rationale**: Nothing in specs 1–3 persists a `Location` value anywhere — spec 1's `Location`
type is a pure, in-memory constructor with no I/O (by design, constitution Principle V), and
neither spec 2's registry nor spec 3's `RendererConfig` has a field for it. This spec is the
one that resolves the "location needs a home" gap (spec.md Clarifications), so it also owns
the schema that gives it a durable one.

**Scope note (flagged in plan.md)**: Writing this entry is this spec's job; *reading* it at
schedule-computation time is spec 3's `scheduler_bridge.rs`, which does not do so today — see
plan.md Cross-Spec Dependencies and contracts/location-config-schema.md.

**Alternatives considered**: Adding a `location` field directly to spec 3's existing
`RendererConfig` instead of a new schema — considered, but rejected in favor of a separate
schema for the same reason spec 3 itself gave for not overloading spec 2's registry: keeping
each distinct concern independently versioned avoids one schema's migration story silently
covering a concern it wasn't designed for. `scheduler_bridge.rs` reading two config entries
instead of one is a trivial cost for that isolation.

## R5. The D-Bus interface `wallpaperd` must expose

**Decision**: A minimal, purpose-built D-Bus interface (documented in full in
contracts/wallpaperd-dbus-interface.md) exposing exactly two operations: a per-output
current/next-transition query (`QueryOutput`/`QueryAll`, backing FR-009 — and, by reusing
`QueryAll`'s output-identifier field alone, FR-005's `list outputs` too, avoiding a third
near-duplicate method) and a re-evaluate-now call scoped to one output or all of them
(backing FR-010) — nothing broader (no generic "get full daemon state" or "set config"
methods, since every other command already goes through `cosmic-config` per R4 and spec 3's
existing contract).

**Rationale**: Keeping the D-Bus surface to exactly what FR-005/FR-009/FR-010 need avoids
creating a second, competing configuration-change pathway alongside `cosmic-config` (which
would undermine constitution Principle IV's single-source-of-truth intent) — this interface is
read/trigger-only, never a way to change persisted state. Reusing `QueryAll` for `list outputs`
(rather than adding a dedicated `ListOutputs` method) keeps the interface's surface area
minimal, at the small cost of the CLI discarding fields it doesn't need for that command.

**Alternatives considered**: Exposing D-Bus properties for every piece of `RendererConfig`
state (mirroring it for convenience) — rejected; it would create two ways to read/change the
same state (D-Bus vs. `cosmic-config`), inviting drift between them for no benefit this spec's
FRs actually require.

## R6. Testing strategy

**Decision**: Two tiers, mirroring spec 3's own posture (research.md R6 there). (1) Argument
parsing/validation and output formatting (`command_parsing.rs`) plus the config-only commands
— register, list *packs*, remove, assign, location (`register_list_remove.rs`,
`assign_location.rs`) — are fully `cargo test`-able using `tempfile`-backed real
`pack-loader`/`cosmic-config` instances, the same pattern spec 2's research.md R6 established.
(2) The three D-Bus-dependent commands (`list outputs`, query, force re-evaluation) are
validated primarily via manual QA against a real running `wallpaperd` once spec 3 is
implemented and amended per R5 — with the CLI's own request-construction/response-parsing
logic unit-testable in isolation against a lightweight mock D-Bus service
(`dbus_mock.rs`, using `zbus`'s own testing utilities), stopping short of asserting anything
about the real daemon's behavior.

**Rationale**: This keeps the bulk of this spec's surface (10 of 13 FRs) fully automated and
CI-friendly, consistent with specs 1–2's testing rigor, while being honest that the three
commands depending on spec 3's not-yet-implemented D-Bus service (R5) can't be end-to-end
tested until that exists — the same gap spec 3 itself already acknowledged for its own
Wayland/GPU code.

**Alternatives considered**: Standing up a full mock `wallpaperd` D-Bus service in CI for
end-to-end testing — appealing, but premature before spec 3's real interface (R5) is even
implemented; revisit once contracts/wallpaperd-dbus-interface.md has a real implementation to
test against.
