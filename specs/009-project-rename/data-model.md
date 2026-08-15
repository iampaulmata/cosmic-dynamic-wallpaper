# Phase 1 Data Model: Rename Project to "Cosmic Dynamic Wallpaper"

This feature introduces no new business entity (spec.md's Key Entities section is
`N/A`). What it *does* change is the storage identifier of four existing `cosmic-config`
entities, and it introduces one new implicit concept — each store's migration state —
needed to make FR-004a's "zero data loss" requirement concrete and testable.

## Affected Entities (existing, identifier renamed only)

Each row's *shape* (fields, validation rules) is unchanged by this feature — only the
`cosmic-config` application ID it's stored under changes. Shapes are documented in their
owning module already; listed here only to the extent needed to reason about migration.

| Entity | Owning module | Old application ID | New application ID |
|---|---|---|---|
| `RendererConfig` (same-pack-everywhere toggle, per-output overrides, crossfade duration) | `wallpaper-ipc::renderer_config` | `com.system76.CosmicWallpaper.Renderer` | `com.system76.CosmicDynamicWallpaper.Renderer` |
| `LocationConfigEntry` (mode, manual/automatic/IP-resolved location, status) | `wallpaper-ipc::location_config` | `com.system76.CosmicWallpaper.Location` | `com.system76.CosmicDynamicWallpaper.Location` |
| `RegistryConfig` (known pack sources + status) | `pack-loader::registry` | `com.system76.CosmicWallpaper.Registry` | `com.system76.CosmicDynamicWallpaper.Registry` |
| `RemovedStarterPacksConfig` (starter packs the user explicitly removed, so a future reinstall doesn't silently re-add them) | `pack-loader::registry` | `com.system76.CosmicWallpaper.RemovedStarterPacks` | `com.system76.CosmicDynamicWallpaper.RemovedStarterPacks` |

## New Concept: Per-Store Migration State (FR-004a)

Not a persisted field — inferred at read time from whether the *new* application ID's
store already holds non-default content. No fifth piece of state is introduced to track
this (research.md R4).

**States**:

- **Unmigrated**: the new-ID store is still at its type's `Default` (nothing has ever
  been written there).
- **Migrated**: the new-ID store holds real content — either because a migration ran, or
  because this is a genuinely fresh install that has simply been configured under the
  new ID directly (the two are indistinguishable, and don't need to be distinguished).

**Transition** (`Unmigrated → Migrated`, one-directional, per store, run at the startup
of whichever process — `wallpaperd` or `wallpaper-settings` — opens that store first):

```text
new_entry = NewId::load(new_config)
if new_entry is still Default:
    old_entry = OldId::load(old_config)     # OldId::open() may itself fail if the old
                                             # store never existed (fresh install) —
                                             # that's the "nothing to migrate" case,
                                             # not an error (spec Edge Cases).
    if old_entry is NOT Default:
        old_entry.save(new_config)          # write verbatim under the new ID
# else: already migrated (or freshly configured under the new ID) — no-op.
```

**Validation rules**:

- MUST NOT delete, clear, or otherwise mutate the old-ID store. It's left in place,
  permanently unread again, once superseded (research.md R4's rationale).
- MUST be safe to run redundantly from two processes at nearly the same instant (the
  spec's race edge case) — satisfied because the check-then-write above, even if
  duplicated, writes identical content each time, and `cosmic_config::Config`'s own
  `write_entry` is already a whole-file atomic replace.
- MUST NOT treat "old store doesn't exist" as an error — a fresh install has no old
  store at all, and that's the common case going forward, not a failure.
- MUST NOT run any transformation on the migrated content beyond a verbatim copy —
  this migration changes *where* data lives, never *what* it means (unlike the existing
  v2→v3 schema migration, which does reshape a field; this migration is deliberately
  simpler and orthogonal to that one).
