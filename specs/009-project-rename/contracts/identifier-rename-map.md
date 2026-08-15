# Contract: Identifier Rename Map

The authoritative old→new mapping for every renamed identifier in this feature. Every
task in `/speckit-tasks` that touches a name should point back to a row here rather than
re-deriving it — this is the single source of truth, compiled from a direct inspection
of the current repository (2026-08-15), not from memory or assumption.

## 1. Project display name (prose — FR-001, FR-005, FR-006)

| Context | Old | New |
|---|---|---|
| Project name in any prose | `Dynamic Wallpaper` / `dynamic-wallpaper` | `Cosmic Dynamic Wallpaper` |
| GitHub repo slug | `rust-dynamic-wallpaper` | `cosmic-dynamic-wallpaper` |

**Excluded from this rename** (research.md R1 — do not touch): README.md's reference to
"Cinnamon's **Dynamic Wallpaper** extension" (a different, real, unrelated project) and
"Apple's `.heic` dynamic-wallpaper metadata format" (Apple's own terminology). Also
excluded: any file under `specs/001-project-name-format` ... `specs/008-*` (historical
record of already-completed work).

## 2. Local folder & GitHub repository (FR-002, FR-003)

| Old | New |
|---|---|
| `/home/paul/Projects/dynamic-wallpaper` (local folder) | `/home/paul/Projects/cosmic-dynamic-wallpaper` |
| `github.com/iampaulmata/rust-dynamic-wallpaper` | `github.com/iampaulmata/cosmic-dynamic-wallpaper` |
| local `origin` remote URL | updated to match, after the GitHub-side rename |

Both require manual action outside this environment's tool access (research.md R6, R7).

## 3. Compiled binaries (FR-004, research.md R2)

| Crate | Old binary name | New binary name | How it's declared |
|---|---|---|---|
| `renderer` | `wallpaperd` | `cosmic-wallpaperd` | Implicit — cargo names a `src/bin/*.rs` binary after its filename. Rename `crates/renderer/src/bin/wallpaperd.rs` → `crates/renderer/src/bin/cosmic-wallpaperd.rs`; no `Cargo.toml` edit needed for the binary name itself. |
| `wallpaperctl` | `wallpaperctl` | `cosmic-wallpaperctl` | Explicit `[[bin]] name = "..."` in `crates/wallpaperctl/Cargo.toml` — edit the string. |
| `wallpaper-settings` | `wallpaper-settings` | `cosmic-wallpaper-settings` | Explicit `[[bin]] name = "..."` in `crates/wallpaper-settings/Cargo.toml` — edit the string. |

**Not renamed**: the Cargo *package* names (`renderer`, `wallpaperctl`,
`wallpaper-settings`, `schedule-engine`, `pack-loader`, `wallpaper-ipc`) — see plan.md's
Structure Decision for why.

## 4. D-Bus identifiers (FR-004, research.md R3)

All three constants live in exactly two files, which must stay byte-identical to each
other (a pre-existing invariant, unrelated to this rename — `wallpaper-ipc::dbus_client`
is the client's copy, `renderer::dbus_service` is the server's copy):

| Constant | File(s) | Old value | New value |
|---|---|---|---|
| `BUS_NAME` | `crates/wallpaper-ipc/src/dbus_client.rs`, `crates/renderer/src/dbus_service.rs` | `com.system76.CosmicWallpaper1` | `com.system76.CosmicDynamicWallpaper1` |
| `OBJECT_PATH` | same two files | `/com/system76/CosmicWallpaper1` | `/com/system76/CosmicDynamicWallpaper1` |
| `INTERFACE` | same two files, plus the `#[zbus::interface(interface = "...")]` attribute in `dbus_service.rs` | `com.system76.CosmicWallpaper1.Daemon` | `com.system76.CosmicDynamicWallpaper1.Daemon` |

Also update the human-readable copy in `specs/004-cli-control-surface/contracts/
wallpaperd-dbus-interface.md` and `crates/wallpaperctl/README.md` that quotes these same
values — these ARE living reference docs describing the current interface (not frozen
history like the rest of specs/004), so they fall under FR-005.

## 5. `cosmic-config` application IDs (FR-004, FR-004a, research.md R3–R4)

See `data-model.md`'s Affected Entities table for the full old→new mapping (4 rows) and
`config-migration.md` for the migration contract each one must satisfy.

## 6. `.desktop` entry (FR-004, FR-006)

| Field | Old | New |
|---|---|---|
| Filename (and `renderer/Cargo.toml`'s asset-list path referencing it) | `com.system76.CosmicWallpaperSettings.desktop` | `com.system76.CosmicDynamicWallpaperSettings.desktop` |
| `[package.metadata.deb] assets` install destination | `usr/share/applications/com.system76.CosmicWallpaperSettings.desktop` | `usr/share/applications/com.system76.CosmicDynamicWallpaperSettings.desktop` |
| `Name=` (also the GUI's window title — libcosmic derives it from the `.desktop` entry; there is no separate title string in the Rust source) | `Wallpaper Settings` | `Cosmic Dynamic Wallpaper Settings` |
| `Comment=` | `Configure the dynamic wallpaper daemon` | `Configure the Cosmic Dynamic Wallpaper daemon` |
| `Exec=` | `wallpaper-settings` | `cosmic-wallpaper-settings` (must match §3's binary rename) |
| `APP_ID` constant in `crates/wallpaper-settings/src/app.rs` | `com.system76.CosmicWallpaperSettings` | `com.system76.CosmicDynamicWallpaperSettings` |

## 7. Debian package (FR-004, FR-004b, FR-006, research.md R5)

All in `crates/renderer/Cargo.toml`'s `[package.metadata.deb]` table:

| Field | Old | New |
|---|---|---|
| `name` | `dynamic-wallpaper` | `cosmic-dynamic-wallpaper` |
| `replaces` (new field) | *(absent)* | `"dynamic-wallpaper"` |
| `conflicts` (new field) | *(absent)* | `"dynamic-wallpaper"` |
| `breaks` (new field) | *(absent)* | `"dynamic-wallpaper"` |
| `extended-description` | mentions "dynamic wallpaper" in prose | updated per §1 |
| `revision` | `"3"` (v0.1.0-beta.3) | implementer's choice: reset to `"1"` under the new package name, or continue the existing counter — either satisfies the spec; not worth its own clarification question |

## 8. systemd unit (FR-004, FR-006)

| Field | Old | New |
|---|---|---|
| Unit filename (and `renderer/Cargo.toml`'s asset-list path + `WantedBy=`/install location) | `wallpaperd.service` | `cosmic-wallpaperd.service` |
| `Description=` | `Dynamic wallpaper renderer` | `Cosmic Dynamic Wallpaper renderer` |
| `Documentation=` | `https://github.com/iampaulmata/rust-dynamic-wallpaper` | `https://github.com/iampaulmata/cosmic-dynamic-wallpaper` |
| `ExecStart=` | `/usr/bin/wallpaperd` | `/usr/bin/cosmic-wallpaperd` (must match §3) |
| `packaging/debian/{postinst,prerm}`'s `systemctl --user --global {enable,disable} wallpaperd.service` | `wallpaperd.service` | `cosmic-wallpaperd.service` |

## 9. CLI `--help`/about text (FR-001)

| Location | Old | New |
|---|---|---|
| `crates/wallpaperctl/src/main.rs`'s `#[command(name = ..., about = ...)]` | `name = "wallpaperctl"`, `about = "Control surface for the dynamic wallpaper daemon"` | `name = "cosmic-wallpaperctl"`, `about = "Control surface for the Cosmic Dynamic Wallpaper daemon"` |
