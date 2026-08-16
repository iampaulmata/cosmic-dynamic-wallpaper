# Contract Delta: `wallpaperd` D-Bus interface hardening

This is a **delta** against `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md`
(the original interface contract, already implemented). Nothing below changes a method's name,
signature, or success-path behavior for a well-formed caller — every change is additive
validation, a new bound, or a new failure mode for previously-unvalidated/unbounded input.
Existing callers making well-formed calls (the settings GUI, `wallpaperctl` itself) observe no
behavior change.

## `Reevaluate(output_id: String) -> ()`

**Unchanged**: still validates `output_id` against currently-known outputs, still enqueues a
pending request for the main loop to drain.

**New (US4/FR-014)**: if the pending-request queue already holds `MAX_PENDING_DBUS_REQUESTS` (8)
entries, this call now returns `org.freedesktop.DBus.Error.LimitsExceeded` instead of enqueuing a
9th. A legitimate caller (GUI, `wallpaperctl`) issuing normal, human-paced requests never
approaches this bound — 8 comfortably exceeds any realistic simultaneous multi-monitor
reevaluation burst.

**New (US4/FR-017)**: `output_id` is now validated via `wallpaper_ipc::OutputId::validated`
(non-empty, ≤256 bytes) before the existing known-outputs lookup. An invalid `output_id` now
returns `org.freedesktop.DBus.Error.InvalidArgs` with a message describing the specific
validation failure (empty / too long), rather than only "unmanaged output."

## `ReevaluateAll() -> ()`

**Unchanged**: still enqueues a request the main loop drains on its next tick; still always
"succeeds" from the caller's perspective (no output to be invalid about).

**New (US4/FR-014)**: coalesced — if a `ReevaluateAll` request is already pending, a subsequent
call is a silent no-op rather than queuing a second, redundant one. Once the queue (including any
mix of pending `Reevaluate`/`ReevaluateAll` entries) reaches `MAX_PENDING_DBUS_REQUESTS` (8) with
no pending `All` already present, further calls are dropped and logged
(`tracing::warn!`) rather than queued — this method's D-Bus signature returns `()` regardless
(unchanged), so a caller cannot observe the drop via the D-Bus call itself, only via the daemon's
own logs.

**Threat model note**: this is the fix for the audit's "unauthenticated local process can drive
unbounded memory growth and pin CPU/GPU via ReevaluateAll" finding — a same-uid process calling
this method in a tight loop now costs the daemon O(1) additional work after the first call
(coalescing), not unbounded queue growth.

## `QueryOutput(output_id: String) -> (...)`

**New (US4/FR-017)**: `output_id` validated the same way as `Reevaluate` above, before the
existing lookup.

## `QueryAll() -> Array<(...)>`

**Unchanged wire shape.**

**New (US4/FR-016)**: every call is now logged (`tracing::debug!`) so it's visible in the
daemon's log stream (`journalctl` under the shipped systemd unit) — see research.md R12 for why
this, not a caller-identity/consent-prompt mechanism, is this feature's scope for this finding.

## Deployment: session-bus policy file (US4/FR-015)

**New**: `packaging/dbus-1/com.system76.CosmicDynamicWallpaper1.conf`, installed to
`/usr/share/dbus-1/session.d/`. Does not change any method's runtime behavior — documents and
makes explicit the same-uid trust boundary this interface has always relied on implicitly (see
research.md R11 for why a stronger per-method authorization scheme, e.g. polkit, is
out of scope).

## Error/unreachable handling

**Unchanged** from the original contract — `wallpaperctl` still fails fast with
`CliError::DaemonUnreachable` if the bus name isn't owned. See
`contracts/wallpaperctl-cli-hardening.md` in this feature for that error's **exit code**
renumbering (the D-Bus-level behavior is unchanged; only the CLI's mapped exit code moves).

## Explicitly not in this contract

- Any authorization scheme stronger than same-uid isolation + the new policy file above (out of
  scope, research.md R11).
- Caller-identity logging for `QueryAll` (considered and deferred, research.md R12).
- Any change to `QueryOutput`/`QueryAll`'s response *shape* — only new rejection paths for
  malformed input are added, the success-path response is byte-for-byte unchanged.
