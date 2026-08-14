# Contract: Pack registry `origin` extension (spec 2 `pack-loader`, v-bump)

## Schema addition

`PackRegistryEntry` (spec 2) gains one field:

```text
PackRegistryEntry(
    source: Directory("/usr/share/dynamic-wallpaper/starter-pack"),
    origin: Package,
    // ...existing fields unchanged
)
```

`origin` defaults to `User` — every pack registered via `wallpaperctl register` or the GUI's
equivalent (spec.md FR-002's browser implies packs already got there somehow; registration itself
is unchanged from spec 4) is `User`-origin, same as every pack in every registry that exists
today, read forward with no behavior change (data-model.md Migration).

## New, separate small schema: `RemovedStarterPacks`

```text
RemovedStarterPacks(
    schema_version: 1,
    removed: [
        Directory("/usr/share/dynamic-wallpaper/starter-pack"),
    ],
)
```

A minimal `cosmic-config` entry, own application id, holding only the list of `PackSource`s a user
has explicitly removed while they were `Package`-origin. Deliberately not folded into
`PackRegistryEntry` itself, since a removed pack's registry entry no longer exists at all —
there's nothing to attach an `origin` to once it's gone.

## Who writes these schemas

- `postinst` (spec 5, amended) writes the initial `Package`-origin `PackRegistryEntry` for
  `assets/starter-pack/` on install, **after** checking `RemovedStarterPacks` doesn't already list
  it (FR-010) — an upgrade's `postinst` run is idempotent against a prior explicit removal.
- `wallpaperctl remove` / the GUI's equivalent — when removing an entry whose `origin` is
  `Package`, additionally appends its `source` to `RemovedStarterPacks` (spec.md FR-010).
  Removing a `User`-origin entry is unchanged from spec 4 — no write to `RemovedStarterPacks`.

## Who must read these schemas

- `postinst`, as above — the only reader of `RemovedStarterPacks`.
- Nothing else needs `origin` at read time beyond spec 4's existing `list packs` display, which
  MAY show origin as an informational field (spec.md doesn't require this, left as a task-level
  nicety, not a contract requirement).
