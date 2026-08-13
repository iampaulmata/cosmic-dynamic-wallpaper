# Contract: `renderer` `cosmic-config` schema

This crate is a daemon, not a library other specs link against the way schedule-engine and
pack-loader are — its "contract" is the on-disk `cosmic-config` schema that spec 4 (CLI) and
any future GUI write to, and this daemon watches. See data-model.md `RendererConfig` for the
authoritative field list; this file documents the wire shape and watch semantics.

## Schema (RON, `cosmic-config`-managed)

```text
RendererConfig(
    schema_version: 1,
    same_pack_everywhere: Some(Directory("/home/user/.local/share/wallpaper-packs/seasons")),
    overrides: {
        "DP-3": StaticFile("/home/user/Pictures/office-view.jpg"),
    },
)
```

- `same_pack_everywhere: None` means the "same pack on all outputs" toggle (FR-006) is off.
- `overrides` keys are `OutputId` strings (data-model.md — the `xdg-output` connector name,
  e.g. `"eDP-1"`, `"DP-3"`); an output with an entry here always follows it, regardless of
  `same_pack_everywhere` (FR-006).
- `PackSource` values (`Directory(..)` / `StaticFile(..)`) are spec 2's type, reused verbatim
  — this schema never redefines pack identity.

## Watch semantics

- This daemon watches the whole `RendererConfig` entry via `cosmic-config`'s standard
  change-notification mechanism (research.md R4) — no polling.
- On any change, every affected output (an output whose resolved `OutputAssignment` — 
  data-model.md's resolution rule — actually changed as a result) re-evaluates within 2
  seconds of the change being detected (FR-007, spec.md Clarifications).
- If multiple changes to the same output arrive before its re-evaluation runs, they are
  coalesced — only the state as of when re-evaluation actually executes is applied (FR-014,
  data-model.md `PendingChange`). A writer (spec 4's CLI, a future GUI) does not need to
  debounce its own writes; this daemon does that on the read side.
- An output with no entry in `overrides` and `same_pack_everywhere: None` resolves to
  `OutputAssignment::Unassigned` (FR-009) — not an error, and not represented as a distinct
  on-disk value; it's simply the absence of both.

## Who writes this schema

Out of scope for this spec (spec.md Assumptions) — this contract only commits to the shape
and watch behavior, not the UI/CLI surface that produces the writes. Spec 4 (CLI control
surface, FR-21) and any future GUI (FR-22) are expected to write directly to this
`cosmic-config` entry rather than calling into a bespoke IPC method on this daemon, consistent
with constitution Principle IV's "settings live in cosmic-config" model.

## Explicitly not in this contract

- Spec 1's `ScheduleQueryResult`/`next_transition_after` contract (schedule-engine) — this
  crate calls it, doesn't wrap or re-expose it as a separate schema.
- Spec 2's pack manifest schema or registry (pack-loader) — `PackSource` values here are
  opaque identifiers this schema stores, not something this crate re-validates.
- A live D-Bus (or other RPC) surface for "query current/next transition" or "force
  re-evaluation now" (PRD FR-21) — if spec 4 needs a live query/command path beyond writing to
  this config entry and waiting up to 2 seconds, designing that transport is spec 4's own
  decision, not fixed by this contract.
