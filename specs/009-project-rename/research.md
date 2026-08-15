# Phase 0 Research: Rename Project to "Cosmic Dynamic Wallpaper"

No `[NEEDS CLARIFICATION]` markers remain in the Technical Context — this feature reuses
only technology already present in the workspace. The research below resolves the
*decisions* (scope boundaries, naming scheme, migration mechanism) needed before design,
each grounded in a direct inspection of the current repository rather than assumption.

## R1: Which occurrences of the old name are actually in scope?

**Decision**: Only rename occurrences that name *this project*. Three categories of
`dynamic-wallpaper`/`Dynamic Wallpaper` text are explicitly OUT of scope and MUST be
left untouched:

1. **Third-party project names quoted in prose** — README.md's "Cinnamon's **Dynamic
   Wallpaper** extension (which was the true inspiration behind this project)" names a
   *different, unrelated* GNOME/Cinnamon extension that happens to share words with our
   old name. Renaming this would misattribute credit and is factually wrong.
2. **Generic technical terminology** — README.md's "Parsing Apple's `.heic`
   dynamic-wallpaper metadata format directly" refers to Apple's own name for their
   file-format feature, not this project.
3. **Historical spec/plan/tasks documents under `specs/001-008-*`** — these are frozen
   records of already-completed, already-shipped work, written under the name that
   existed at the time. Retroactively editing them is the documentation equivalent of
   rewriting published git history (already ruled out in spec.md's Assumptions) — same
   reasoning applies here: no reader benefit, real risk of silently changing the
   historical record of what US1-scoped decisions actually said when they were made.
   `specs/009-project-rename/` (this feature) and any *future* spec are naturally
   unaffected since they're written after the rename.

Concretely in scope (confirmed via direct grep, 2026-08-15): `README.md`, `.specify/
memory/constitution.md` (title + Governance section's "the `dynamic-wallpaper`
project"), every crate's `README.md` and `Cargo.toml` `description`, `docs/PRD.md`,
`docs/pack-manifest-schema.md`, `packaging/desktop/*.desktop`,
`packaging/systemd/*.service`, and source comments in `crates/*/src/lib.rs` /
`starter_pack.rs` / `surface.rs` / `ip_geolocation.rs` / `main.rs` (×2) /
`tools/generate-starter-pack/src/main.rs` that name the project.

**Rationale**: Matches SC-001's own wording exactly ("...outside of explicitly
historical content") — this decision is what makes that success criterion concretely
checkable rather than aspirational.

**Alternatives considered**: A blind global find-and-replace of the string
`dynamic-wallpaper` / `Dynamic Wallpaper` (rejected — confirmed by direct inspection
that this would incorrectly rewrite the Cinnamon extension's name and Apple's
terminology, producing factually wrong prose); retroactively editing specs/001-008 too
for perfect textual consistency (rejected — unbounded low-value effort, and
constitution's own governance treats past decisions as a record, not living text).

## R2: Binary and Debian package naming scheme

**Decision**: Every compiled binary and the Debian package name get a `cosmic-` prefix:
`wallpaperd` → `cosmic-wallpaperd`, `wallpaperctl` → `cosmic-wallpaperctl`,
`wallpaper-settings` → `cosmic-wallpaper-settings`, Debian package `dynamic-wallpaper` →
`cosmic-dynamic-wallpaper`.

**Rationale**: This is the established, observable convention for every sibling COSMIC
desktop component already installed on this exact dev machine — confirmed directly via
`~/.config/cosmic/` and `dpkg -l`: `cosmic-bg`, `cosmic-comp`, `cosmic-panel`,
`cosmic-files`, `cosmic-term`, `cosmic-edit`, `cosmic-app-library`, `cosmic-applets`.
The constitution itself already refers to `cosmic-bg`/`cosmic-comp`/`cosmic-session` by
these exact prefixed names. Adopting the same scheme is the most COSMIC-native option
available, directly serving FR-001's intent.

**Alternatives considered**: Leave binaries unprefixed, renaming only the `.deb`/D-Bus/
`cosmic-config` identifiers (rejected — a half-measure; a user running `wallpaperctl
--help` on a package literally named `cosmic-dynamic-wallpaper` is the exact
inconsistency FR-001 exists to eliminate). Use the full display-name slug as the binary
prefix (`cosmic-dynamic-wallpaper-daemon`, `cosmic-dynamic-wallpaper-ctl`, ...)
(rejected — no sibling COSMIC binary uses its full app name as a CLI-typed prefix; all
of them use a short `cosmic-<role>` form, and `ExecStart=`/typed-CLI ergonomics favor
brevity).

## R3: D-Bus, `cosmic-config`, and `.desktop` identifier scheme

**Decision**: A direct substring rename, `CosmicWallpaper` → `CosmicDynamicWallpaper`,
preserving every existing suffix and structure exactly:

| Old | New |
|---|---|
| `com.system76.CosmicWallpaper1` (D-Bus bus name) | `com.system76.CosmicDynamicWallpaper1` |
| `com.system76.CosmicWallpaper1.Daemon` (D-Bus interface) | `com.system76.CosmicDynamicWallpaper1.Daemon` |
| `/com/system76/CosmicWallpaper1` (D-Bus object path) | `/com/system76/CosmicDynamicWallpaper1` |
| `com.system76.CosmicWallpaper.Renderer` (`cosmic-config` app ID) | `com.system76.CosmicDynamicWallpaper.Renderer` |
| `com.system76.CosmicWallpaper.Location` | `com.system76.CosmicDynamicWallpaper.Location` |
| `com.system76.CosmicWallpaper.Registry` | `com.system76.CosmicDynamicWallpaper.Registry` |
| `com.system76.CosmicWallpaper.RemovedStarterPacks` | `com.system76.CosmicDynamicWallpaper.RemovedStarterPacks` |
| `com.system76.CosmicWallpaperSettings` (`.desktop` app ID) | `com.system76.CosmicDynamicWallpaperSettings` |

**Rationale**: Confirmed via direct grep that every one of these is a single string
literal constant (in `wallpaper-ipc/src/{dbus_client,renderer_config,location_config}.rs`,
`pack-loader/src/registry.rs`, `renderer/src/dbus_service.rs`,
`wallpaper-settings/src/app.rs`) plus one `.desktop` filename and one packaging
asset-list entry — 12 call sites total. A pure substring rename means every existing
test/contract that asserts a suffix (e.g. `.Daemon`, `.Location`) keeps passing
unmodified once the shared prefix constant changes, minimizing the diff surface against
FR-007's "no functional behavior change."

**Alternatives considered**: A version bump instead of a rename (e.g.
`CosmicWallpaper2`) (rejected — doesn't address the actual ask, which is a name change,
not a protocol version change; would also be misleading since the protocol shape itself
isn't changing). Dropping the numeric `1` suffix on the D-Bus bus/interface name
entirely (rejected — out of scope; that versioning suffix predates this feature and
changing it is an unrelated decision this spec doesn't need to make).

## R4: Config migration mechanism (FR-004a)

**Decision**: Each renamed `cosmic-config` store gets a small `migrate_from_old_id()`
function, colocated with that store's existing module (`wallpaper-ipc::renderer_config`,
`wallpaper-ipc::location_config`, `pack-loader::registry`) — mirroring exactly where
`LocationConfigEntry`'s existing v2→v3 schema migration already lives. Each function:

1. Opens the *old* application ID's `cosmic-config::Config` read-only.
2. If the new application ID's store is not already populated (i.e. `get_entry`
   returns the default because nothing has been written yet — cheap, no separate
   "have we migrated" flag needed), and the old store has real content, write that
   content verbatim under the new application ID.
3. Never deletes or mutates the old store — it's simply left as an inert, unread
   artifact once superseded. Nothing reads through the old ID again after migration.

Called once at startup by whichever process opens that store first — both `wallpaperd`
and `wallpaper-settings` already call `Config::open`/`::load` for every one of these
stores today, so this is a small addition at an existing call site, not new
infrastructure.

**Rationale**: Reuses this project's own established migration precedent (Principle X)
rather than inventing a new mechanism. Checking "is the new store still at its default"
instead of a separate migration-done flag avoids introducing a fifth piece of state
purely to track the other four's migration status. Never deleting the old store makes
the whole operation trivially safe under the race the spec's Edge Cases call out
(daemon and GUI both starting around login and racing to migrate the same store): worst
case is a redundant, idempotent duplicate write of identical content — `cosmic_config`'s
own `Config::write_entry` already does an atomic whole-file replace (the same primitive
every other `.save()` call in this codebase already relies on), so this can't tear or
corrupt data.

**Alternatives considered**: A dedicated manual "migrate" CLI subcommand or standalone
tool (rejected — directly violates FR-004a's "without user action," and breaks from
this project's own precedent of migrations running transparently on load). A separate
"migration completed" marker file/config entry (rejected — unnecessary extra state; the
new store's own default-vs-populated status is already sufficient and self-describing).
Deleting the old store after a successful migration (rejected — no benefit; an
unreferenced directory under the old app ID costs nothing left in place, while deleting
it adds a destructive step that only increases risk for zero gain, and keeps a natural
audit trail if something ever needs to be cross-checked).

## R5: Debian package supersession (FR-004b)

**Decision**: Add `replaces`, `conflicts`, and `breaks` (each set to `"dynamic-wallpaper"`)
to `crates/renderer/Cargo.toml`'s existing `[package.metadata.deb]` table, alongside the
already-present `depends`/`recommends` fields.

**Rationale**: Directly verified, not assumed — inspected the exact pinned `cargo-deb
3.7.0` source (`~/.cargo/registry/.../cargo-deb-3.7.0/src/config.rs`) and confirmed
`conflicts`, `breaks`, `replaces`, and `provides` are all supported
`[package.metadata.deb]` keys that get written straight into the generated `DEBIAN/
control` file's `Conflicts:`/`Breaks:`/`Replaces:` fields — the standard, textbook
Debian mechanism for "this package supersedes that other package," which is exactly
what FR-004b needs: `apt install ./cosmic-dynamic-wallpaper_*.deb` will now remove
`dynamic-wallpaper` first (running its already-existing `prerm`, which disables its
systemd unit) as part of the same transaction.

**Alternatives considered**: A `postinst` script that manually detects and purges the
old package (rejected — reinvents what `dpkg`/`apt` already does correctly and
atomically via control-file relationships; a hand-rolled shell equivalent is strictly
more failure-prone). Requiring users to manually `apt purge dynamic-wallpaper` first
(rejected by the clarification session — this was Option B, explicitly not chosen).

## R6: GitHub repository rename mechanics

**Decision**: Documented as a manual step for the repository owner (GitHub Settings →
repository name, or `gh repo rename` if/when CLI access becomes available) — not
executed by an automated task. After the rename, the local `origin` git remote URL is
updated to match.

**Rationale**: This environment has no `gh` CLI installed and no authenticated GitHub
API write access (confirmed earlier this session) — there is no tool available that can
perform this action. GitHub's own automatic old-URL redirect (noted in spec.md's
Assumptions) means this is low-risk to sequence whenever convenient, not blocking.

**Alternatives considered**: None meaningful — this isn't a technical choice, it's an
access constraint.

## R7: Local folder rename mechanics

**Decision**: A single, deliberate, user-confirmed step late in implementation (moving
`/home/paul/Projects/dynamic-wallpaper` → `/home/paul/Projects/cosmic-dynamic-wallpaper`),
not scripted as an unattended task step.

**Rationale**: This is the live working directory of an active session — an editor, this
very Claude Code session, and `target/`'s build-cache absolute paths are all anchored to
the current path. Moving it out from under an in-progress session without explicit,
immediate confirmation risks breaking open tooling for no benefit over doing it as a
clearly-flagged final step the user is present for.

**Alternatives considered**: Renaming early, before other implementation work
(rejected — every subsequent task in this same working session would then be operating
on a path that no longer matches what's on screen/in the editor, actively confusing
rather than helping); doing it via a fresh clone under the new name instead of an
in-place `mv` (viable alternative, left as an option for the user to choose at that
step rather than deciding for them now — either approach satisfies FR-002 equally).
