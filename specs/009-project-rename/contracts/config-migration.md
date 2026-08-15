# Contract: `cosmic-config` Application-ID Migration

Applies identically to all four stores in `data-model.md`'s Affected Entities table.
Each store's owning module gets one function matching this contract — same shape,
different types, so `/speckit-tasks` can generate one task per store against a single
spec rather than four ad hoc designs.

## Signature (shape, not literal Rust — each store substitutes its own entry type)

```text
fn migrate_from_old_app_id(new_config: &cosmic_config::Config) -> <EntryType>
```

Called in place of a bare `<EntryType>::load(&new_config)` at each existing call site
that currently loads this store (both `wallpaperd` and `wallpaper-settings` call
`RendererConfig::load`/`LocationConfigEntry::load` today; `pack-loader::Registry::open`
is the equivalent entry point for the two pack-loader-owned stores) — this function
*replaces* that call, not adds a second one, so every existing caller gets the migration
for free with a one-line change.

## Behavior contract

1. **Load under the new application ID first.** If the result is not the type's
   `Default`, return it immediately — already migrated (or freshly configured under the
   new ID directly; the two cases are indistinguishable and don't need to be).
2. **Only if still `Default`, attempt to open the *old* application ID.** Opening the
   old store MAY fail (it simply never existed — a fresh install with no prior
   installation at all). Treat that failure as "nothing to migrate," not an error: fall
   through to returning the new store's `Default`.
3. **If the old store opened successfully, load it.** If its content is also the type's
   `Default` (an old install that exists but was never actually configured), there's
   nothing meaningful to carry forward — fall through to returning the new store's
   `Default`.
4. **Otherwise** (the old store opened and holds real, non-default content): write that
   content, verbatim and unmodified, under the *new* application ID via the new store's
   own existing `.save()`, then return it.
5. **Never** write to, delete, or otherwise mutate the *old* application ID's store at
   any point in this function.

## Properties this contract guarantees (and tasks/tests should assert)

- **Idempotent**: calling this function twice in a row (simulating the daemon-and-GUI
  race from spec.md's Edge Cases) produces the same end state as calling it once — the
  second call sees the new store already populated at step 1 and returns immediately.
- **No-op on a genuinely fresh install**: no old store exists at all → step 2's open
  fails → returns `Default`, identical to what `<EntryType>::load` already returns
  today for a fresh install. A fresh install is indistinguishable from "migration ran
  and found nothing," by design.
- **Never destructive**: the old store is only ever read, never written or removed, in
  every branch above.
- **Content-preserving, not content-transforming**: unlike the existing v2→v3 schema
  migration (which reshapes a renamed field), this migration is a pure copy — the value
  written under the new ID is byte-for-byte what `<EntryType>::load` would have returned
  from the old ID.

## Test cases each implementation MUST cover (mirrors `location_config.rs`'s existing
migration test style)

1. Old store has real content, new store is untouched → migration copies it; a second
   call is a no-op (idempotency).
2. Old store never existed (fresh install) → returns `Default`, no panic, no error
   surfaced to the caller.
3. Old store exists but was never configured (still `Default`) → returns `Default`,
   nothing written.
4. New store already has content (already migrated, or configured fresh under the new
   ID) → old store's content, if any, is never read or consulted.
