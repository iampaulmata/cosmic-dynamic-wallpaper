# Contract: `wallpaperd` D-Bus interface (dependency, not yet implemented)

⚠️ **Cross-spec dependency**: this interface is *called* by this spec's `wallpaperctl query`
and `wallpaperctl reevaluate` commands (research.md R3, R5), but must be *implemented* by spec
3's `wallpaperd` binary. Spec 3, as currently tasked, does not run a D-Bus service at all —
see plan.md's Cross-Spec Dependencies section. This file specifies the interface spec 3 needs
to implement, independent of when that implementation lands.

## Bus name and object path

```text
Bus:         session bus
Name:        com.system76.CosmicWallpaper1        (tentative — final name is spec 3's/spec 5's packaging decision)
Object path: /com/system76/CosmicWallpaper1
Interface:   com.system76.CosmicWallpaper1.Daemon
```

## Methods

```text
QueryOutput(output_id: String) -> (assigned: bool, active_image: String, next_transition_at: String)
```
- Backs FR-009. `output_id` is spec 3's `OutputId` string form. If the output has no
  assignment, `assigned` is `false` and the other two fields are empty — matching
  data-model.md's `QueryState::Unassigned` (spec.md US4 Scenario 2).
- If `output_id` names an output the daemon doesn't currently manage, the method returns a
  D-Bus error (`org.freedesktop.DBus.Error.InvalidArgs` or equivalent) — mapped by
  `wallpaperctl` to `CliError::OutputNotFound` (FR-007's posture applied to queries too).

```text
QueryAll() -> Array<(output_id: String, assigned: bool, active_image: String, next_transition_at: String)>
```
- Backs FR-009 when no `--output` is given — one entry per currently-managed output.
- Also backs FR-005 (`wallpaperctl list outputs`) — the CLI calls this same method and
  displays only the `output_id` field of each entry, rather than this interface growing a
  near-duplicate `ListOutputs` method (research.md R5).

```text
Reevaluate(output_id: String) -> ()
```
- Backs FR-010 for a single named output. Triggers spec 3's existing re-evaluation path
  (spec 3 FR-007) without changing any assignment or setting.

```text
ReevaluateAll() -> ()
```
- Backs FR-010 when no `--output` is given.

## Error/unreachable handling (FR-011)

If `wallpaperctl` cannot connect to this bus name at all (no `wallpaperd` running), it MUST
fail fast with `CliError::DaemonUnreachable` — it does not wait, retry, or hang (spec.md US4/
US6 Scenario 3).

## Explicitly not in this contract

- Any method that changes persisted state (assignment, toggle, location) — those go through
  `cosmic-config` exclusively (research.md R4/R5, constitution Principle IV); this interface
  is read/trigger-only, by design, to avoid a second competing configuration pathway.
- Signals/property-change notifications — not required by any FR here (this spec's commands
  are one-shot request/response, not a live-updating watch UI); revisit only if a future GUI
  (FR-22) needs push updates rather than polling.
- Authentication/access control beyond the session bus's own default (same-user) policy — no
  FR here requires anything stricter (spec.md Assumptions).
