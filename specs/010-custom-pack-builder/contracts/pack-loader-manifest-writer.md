# Contract: `pack-loader` manifest write-side (new)

Amends `specs/002-pack-format-loading/contracts/pack-loader-api.md` — that contract's `load_pack`
and `Registry` surfaces are unchanged. This documents only the new write-side addition
(data-model.md), symmetric to that spec's existing read-side `parse`.

## `render`

```text
pub fn render(draft: &ManifestDraft) -> String
```

- Always emits `schema_version = 1`.
- Emits `author = "..."` only when `draft.author.is_some()`; omitted entirely otherwise (matches
  the documented schema's own "author is optional" rule — `docs/pack-manifest-schema.md`).
- Every string value (`name`, `author`, each image's `file`) is TOML-escaped by the `toml` crate's
  own `Serialize` machinery — quotes, backslashes, and non-ASCII text in any of these fields
  round-trip correctly (spec.md Edge Cases).
- `[[images]]` entries are emitted in `draft.images` order — the same order the wizard's rows
  were shown in, so a hand-edit afterward finds images where the user last saw them.
- Never panics; `ManifestDraft`'s shape makes every field already well-typed (a `ScalingMode`, a
  `Color`, a `TimeAnchor`), so there is no "invalid input" case for `render` itself to reject —
  invalidity (empty pack, too many images, conflicting instants) is caught earlier, before a
  `ManifestDraft` is even constructed (data-model.md's four validation rules).

## `format_anchor`

```text
pub fn format_anchor(anchor: &schedule_engine::TimeAnchor) -> String
```

- `TimeAnchor::Clock(t)` → `"HH:MM"` (24-hour, matching `parse_anchor`'s accepted input exactly;
  never emits the also-accepted `HH:MM:SS` form — seconds are always `:00` for a wizard-authored
  clock anchor, so the shorter form is both sufficient and the more readable of the two accepted
  encodings).
- `TimeAnchor::Solar { event, offset: None }` → the bare event name (`"sunrise"`,
  `"solar_noon"`, …).
- `TimeAnchor::Solar { event, offset: Some(d) }` → `"<event><sign><duration>"`, e.g.
  `"civil_dawn-30m"`, `"sunset+1h15m"` — `humantime`-compatible, the same grammar
  `docs/pack-manifest-schema.md` documents and `parse_anchor` already accepts.
- **Round-trip contract**: for every `TimeAnchor` value constructible by this feature's UI
  (offsets clamped to ±12h, research.md R6), `parse_anchor(&format_anchor(&a)) == Ok(a)`. This is
  the property the unit tests for `format_anchor` assert directly, not just example-by-example.

## Consumer flow (what `wallpaper-settings` does with these two functions)

```text
1. draft = build_draft(rows, mode, folder_name, author)   // wallpaper-settings, pure
2. text  = pack_loader::manifest::render(&draft)           // this contract
3. std::fs::write(source_dir.join("manifest.toml"), text)
4. pack_loader::load_pack(&source_dir)                     // self-validation, FR-012
     -> Err: surface the specific error, delete the just-written manifest.toml, stay on the
        configuration screen with all rows/author intact (FR-017) — this should not happen in
        practice, since step 1's inputs were already validated (data-model.md rules), but the
        write is only ever treated as committed after this call succeeds, not before.
     -> Ok:  proceed to the move-vs-keep prompt (FR-013)
```

Step 4 is not optional — it is what makes FR-012 ("the generated manifest MUST be immediately
valid and loadable") a checked postcondition rather than an assumption.
