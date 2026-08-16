# Feature Specification: Fix Adversarial Audit Findings

**Feature Branch**: `011-fix-audit-findings`

**Created**: 2026-08-16

**Status**: Draft

**Input**: User description: "let's take our full adversarial report available at https://claude.ai/code/artifact/1fb099ab-2860-4a0a-ac03-f7523d6ffd1c and develop a plan to fix the issues that were found"

**Source**: Full-codebase adversarial review ("Cosmic Wallpaper Audit"), three personas (Saboteur, Security Auditor, New Hire) run independently across six subsystems, reviewed against `main` @ `50ca3f6` (2026-08-16). Verdict: **BLOCK** — 11 critical, 25 warning, 16 note findings (52 total). Several were verified by compiling and reproducing the failure, not just reading the source.

## Clarifications

### Session 2026-08-16

- Q: How large should a pack manifest.toml be allowed to get before the loader rejects it outright (FR-011), rather than reading it fully into memory first? → A: 512 KB
- Q: What decoded-image byte ceiling should gate GPU texture upload (FR-012), on top of checking against the device's own max_texture_dimension_2d? → A: 256 MB decoded
- Q: How many pending Reevaluate/ReevaluateAll requests should the daemon's D-Bus queue hold before rate-limiting or rejecting more (FR-014)? → A: 8 pending

## User Scenarios & Testing *(mandatory)*

<!--
  Each story groups audit findings that share a failure mode and can be fixed and
  regression-tested as one independently-shippable slice. Priorities follow the audit's
  own severity split and its "priority fixes" list: crash-proofing and the one direct
  arbitrary-write vector come first, then resource/trust-boundary hardening, then
  silent-failure and diagnostic cleanup, then code-health notes.
-->

### User Story 1 - The daemon never crashes on malformed or hostile pack data (Priority: P1)

A pack the user installs — whether hand-edited, downloaded from a stranger, or just buggy — must never be able to take down the wallpaper daemon. Today, four independent reachable panics violate the project's own "never crashes" contract (Constitution Principle VIII), three of them confirmed by compiling and running a minimal reproduction.

**Why this priority**: A crashed daemon means every managed output silently loses its wallpaper renderer until the user notices and manually restarts it — the single worst outcome for an unattended background service, and the theme the audit calls out as recurring across three separate crates.

**Independent Test**: Feed each of the four documented inputs to the affected code path in isolation (a manifest with a non-ASCII hex color, a layer-shell surface configured with a zero-length axis, an out-of-range crossfade progress value, and a pack manifest with an unbounded solar-anchor offset) and confirm the process logs an error and continues instead of panicking.

**Acceptance Scenarios**:

1. **Given** a pack manifest containing `fallback_color = "#€AAA"`, **When** the pack is loaded, **Then** loading fails with a logged, contained error instead of panicking on a non-char-boundary byte slice (`manifest.rs:68-84`).
2. **Given** a layer-shell output where opposing anchors are both set (compositor legitimately reports 0 on that axis), **When** `reconfigure_output` runs, **Then** the renderer picks a usable size for that output instead of passing 0 into `wgpu::Surface::configure` and panicking the whole daemon (`surface.rs:646-691`).
3. **Given** a crossfade progress value that is negative, infinite, NaN, or otherwise out of `[0.0, 1.0]`, **When** it reaches the frame-timing calculation, **Then** the value is clamped before use instead of panicking `Duration::from_secs_f64` or the subsequent `Instant - Duration` subtraction (`surface.rs:318`).
4. **Given** a pack manifest with `TimeAnchor::solar(SolarEventKind::Sunrise, Some(TimeDelta::MAX))`, **When** the anchor is validated and later queried, **Then** validation rejects the out-of-range offset instead of passing and panicking on the first `query()` call with a `DateTime + TimeDelta` overflow (`solar.rs:65`).
5. **Given** each of the four reproductions above, **When** it is added as a regression test, **Then** the test suite fails on the pre-fix code and passes after the fix, so the specific panic cannot silently regress.

---

### User Story 2 - The pack builder can never write outside its intended destination (Priority: P1)

The custom pack builder's collision-rename field is free text that gets joined directly onto a filesystem path with no validation. The audit reproduced a working path-traversal-to-arbitrary-write: typing `../../../.config/autostart` or an absolute path into the rename box and clicking Move copies pack contents outside the sandbox and deletes the original source folder.

**Why this priority**: This is the one finding in the entire report with a direct, reproduced arbitrary-write outcome — every other critical finding causes a crash or resource exhaustion, this one causes attacker-directed file placement.

**Independent Test**: Run the wizard's collision-rename flow with `../../../.config/autostart`, an absolute path such as `/home/user/.ssh`, and an empty string, and confirm each is rejected (or safely normalized to a name inside the destination root) rather than escaping it.

**Acceptance Scenarios**:

1. **Given** the user types a rename value containing a path separator, `..`, or an absolute-path prefix, **When** they click Move, **Then** the app rejects the value with a clear message and performs no filesystem operation outside the configured packs root (`pack_builder.rs:147,495-517,586-590`).
2. **Given** the user leaves the rename value empty, **When** they click Move, **Then** the app rejects the empty value instead of letting `destination_root.join("")` collapse to the packs root itself and merge the pack's contents into it (`pack_builder.rs:495-517`).
3. **Given** a rejected rename value, **When** the error is shown, **Then** the original source folder is left completely untouched — no partial copy, no deletion.
4. **Given** a validated, in-bounds rename value, **When** the move proceeds, **Then** the destination-existence check and the write happen close enough together (or are made atomic) that a concurrent change to the destination between check and write cannot resurrect the traversal risk (`pack_builder.rs:495-517`, TOCTOU note).

---

### User Story 3 - Untrusted pack content cannot exhaust daemon or GPU resources (Priority: P2)

Nothing in the current pipeline stops a pack from making the daemon do unbounded, expensive work before rejecting it: images are decoded and uploaded to the GPU with no dimension or byte ceiling, manifests are read fully into memory with no size cap, and the 64-anchor limit is enforced only after every declared image has already been canonicalized and opened.

**Why this priority**: These are resource-exhaustion / potential-crash vectors reachable from the same untrusted-pack surface as User Story 1, but require sustained or oversized input rather than a single malformed value — real but slightly less immediately catastrophic than a one-shot panic.

**Independent Test**: Register a pack with a manifest declaring 500,000 image entries, a multi-gigabyte `manifest.toml`, and an oversized/high-dimension image, and confirm each is rejected quickly (before the expensive work it currently triggers) rather than after.

**Acceptance Scenarios**:

1. **Given** a manifest declaring more than `MAX_ANCHORS` (64) image entries, **When** the pack is loaded, **Then** the anchor-count cap is checked before any per-image canonicalization, containment check, or image-header read — not after (`load.rs:76-95`, `pack.rs:92-93`).
2. **Given** a `manifest.toml` larger than 512 KB, **When** the loader reads it, **Then** it is rejected before being read fully into memory (`load.rs:72-73`).
3. **Given** a decoded pack image whose dimensions exceed `device.limits().max_texture_dimension_2d` or whose decoded size exceeds 256 MB, **When** `GpuTexture::load` runs, **Then** the image is rejected before `create_texture` is called, instead of the only-untrusted-content gate being absent (`texture.rs:30-45`).
4. **Given** pack-loader's documented boundary that dimension/decode-bomb limits are the renderer's responsibility, **When** the renderer fix above lands, **Then** a test exists that exercises the full pack-loader → renderer path to confirm the boundary is actually enforced, not just documented (`image_check.rs:14-24`).

---

### User Story 4 - The local D-Bus interface can't be abused by another process on the same machine (Priority: P2)

The daemon's session-bus methods have no authorization or rate limiting, and no `dbus-1` policy file exists anywhere in packaging. Any co-located, same-uid process can flood `ReevaluateAll` to drive unbounded memory growth and pin CPU/GPU, enumerate active wallpaper images and precise solar-transition timestamps via `QueryAll` with no allow-list, and the client side trusts whichever process currently owns the well-known bus name.

**Why this priority**: A real trust-boundary gap, but it requires a second local process already running as the same user — a narrower attack surface than untrusted pack content, so it follows the resource-exhaustion story.

**Independent Test**: From a separate local process, call `ReevaluateAll` in a tight loop and confirm the daemon's memory/CPU stay bounded; call `QueryAll` and confirm the response is limited to what an authorized caller should see; pass an oversized/malformed `output_id` and confirm it's rejected before use.

**Acceptance Scenarios**:

1. **Given** a co-located process calling `ReevaluateAll` repeatedly with no delay, **When** requests arrive faster than they can be processed, **Then** the pending queue is bounded to at most 8 entries and excess requests are rate-limited or rejected rather than growing without limit (`dbus_service.rs:58-62,136-150`).
2. **Given** the project ships packaging for the daemon's D-Bus service, **When** it is installed, **Then** a `dbus-1` policy file constrains who may call the daemon's methods, rather than relying solely on same-uid isolation.
3. **Given** a call to `QueryAll`, **When** the daemon responds, **Then** the exposed data is scoped so an arbitrary local process cannot silently harvest precise geographic/timezone-revealing solar-transition timestamps with no user-visible record of the access (`dbus_service.rs`, whole-file finding).
4. **Given** an `output_id` argument of unbounded length or unexpected format, **When** a D-Bus method receives it, **Then** it is validated before use, bounded by an explicit limit rather than only the D-Bus message-size ceiling (`dbus_service.rs:101-112`).

---

### User Story 5 - User- and pack-supplied strings can't inject output or bypass validation (Priority: P2)

Three different sinks accept attacker- or user-controlled strings with no validation: a pack's `name` field flows unescaped into `wallpaperctl list`'s tab-delimited output (reproduced: a name containing tabs/newlines renders fake extra rows), the CLI's `--output` flag is stored with zero validation (reproduced: empty strings and strings containing shell metacharacters both succeed silently), and manifest entries with an absolute path only avoid escaping the pack directory by incidental ordering rather than explicit rejection.

**Why this priority**: Concrete, reproduced correctness/spoofing bugs, but lower impact than the crash and arbitrary-write findings — they corrupt output or silently store garbage rather than crashing the daemon or writing outside a sandbox.

**Independent Test**: Register a pack whose manifest `name` contains tabs and newlines and confirm `wallpaperctl list`'s human-readable output cannot be made to show fabricated rows; run `wallpaperctl assign --output ""` and `--output "DP-3;rm -rf /"` and confirm both are rejected with a validation error instead of silently succeeding; add a manifest entry with an absolute-path `file` value and confirm it is explicitly rejected rather than passing only because of `starts_with` ordering.

**Acceptance Scenarios**:

1. **Given** a pack manifest with a `name` field containing tab or newline characters, **When** `wallpaperctl list` renders its default (non-JSON) output, **Then** those characters are escaped or stripped so the name cannot be rendered as extra fake rows; `--json` output continues to carry the raw value (`commands/list.rs:43`).
2. **Given** an empty or malformed `--output` value, **When** `wallpaperctl assign` runs, **Then** the command is rejected with a validation error instead of writing an override key that can never match a real output (`main.rs:132`).
3. **Given** a manifest `[[images]]` entry whose `file` value is an absolute path, **When** the pack is loaded, **Then** it is explicitly rejected with a clear error, not merely caught as a side effect of a later `starts_with` containment check (`path_safety.rs:18`).
4. **Given** the absolute-path rejection above, **When** the test suite runs, **Then** a fixture exercises exactly that case, closing the gap where only `../` traversal and symlink-escape were previously tested (`path_safety.rs` tests / `fixtures/invalid/path_traversal`).

---

### User Story 6 - Failures are surfaced, never silently swallowed (Priority: P3)

Several code paths currently report success (or silently fall back to defaults) when something actually failed: concurrent registry writers clobber each other with no error to either caller, a corrupted config file is indistinguishable from "never set," the pack builder discards `PackSource::resolve` and `registry.register` failures and closes the wizard reporting success anyway, multi-field config writes aren't atomic across fields, and the generate-pack gate exists only in the UI layer with no re-check in the handler.

**Why this priority**: These produce silent data loss or a misleading success state rather than an immediate crash or security compromise, so they follow the higher-severity stories, but they erode the trust a user has that "no error" means "it worked."

**Independent Test**: Force each failure path directly (concurrent `register` calls, a hand-corrupted config file, a simulated registry-write failure during pack generation, a killed process mid multi-field write, a generate call that bypasses the UI gate) and confirm each now produces a visible error/log instead of a silent success or silent fallback.

**Acceptance Scenarios**:

1. **Given** `wallpaperctl register` and the daemon's in-process registry both attempt to persist at the same time, **When** the second write lands, **Then** the writes are serialized (locked) or the loser receives an explicit conflict error, rather than one silently overwriting the other with no error to either caller (`registry.rs:183-246`).
2. **Given** a corrupted `location` or `renderer` config file on disk, **When** it is read, **Then** the failure is surfaced (logged and/or reported to the caller) rather than being silently treated as "not set," so `location get` no longer reports exit 0 for a corrupted-but-present file the same way it does for "never configured" (`location_config.rs:108-110`, `renderer_config.rs:109-111`).
3. **Given** a `PackSource::resolve` or `registry.register` failure during pack-builder generation, **When** it occurs, **Then** the wizard reports the specific failure to the user and does not close as if generation succeeded — even though the pack may already be generated and moved on disk (`pack_builder.rs:537-544`).
4. **Given** `location set` writing multiple config fields, **When** the process is killed mid-write, **Then** either all fields commit or none do — no state where coordinates update but mode remains stale, or vice versa (`commands/location.rs:110-117`).
5. **Given** `build_draft`/`generate()` being invoked through any path other than the gated UI button, **When** not every row has an assignment, **Then** the handler itself rejects the request instead of silently filtering out unassigned rows into an incomplete pack (`app.rs:356-360`, `pack_builder.rs:670,283-302`).
6. **Given** the pack builder writes `manifest.toml` into the source folder before the user has chosen Move vs. Keep, **When** the app is force-quit or crashes at that point, **Then** either the write is deferred until the choice is made, or the partial state is clearly recoverable/resumable rather than silently treated as an implicit "Keep it here" on next launch (`pack_builder.rs:452-474`).

---

### User Story 7 - Diagnostics, trust assumptions, and defensive gaps are accurate and hardened (Priority: P3)

A cluster of smaller-but-real issues: an exit code documented as "daemon unreachable" collides with clap's own usage-error exit code, location coordinates are persisted world-readable, STUN-based IP geolocation trusts an unauthenticated UDP response with no DNS pinning, portal location updates have no debounce (unlike every other config write), adapter/device GPU requests can block the event loop forever, lost/outdated surfaces are never recovered, and an unsafe FFI block has no documented safety invariant.

**Why this priority**: Each is a real gap the audit surfaced, but none has a reproduced high-impact exploit on its own — they're hardening and correctness polish that should land after the crash/traversal/resource/trust-boundary stories above.

**Independent Test**: Exercise each condition directly — trigger a clap usage error and confirm its exit code no longer matches the documented "daemon unreachable" code; inspect on-disk permissions of the location config file after a fresh write; simulate a forged STUN reply and confirm it's rejected or flagged; send rapid portal location events and confirm they're coalesced; simulate a hung adapter/device request and confirm it times out; simulate a surface loss with no compositor configure event and confirm the output recovers.

**Acceptance Scenarios**:

1. **Given** a plain CLI usage error (e.g. a typo'd subcommand argument), **When** `wallpaperctl` exits, **Then** its exit code is distinguishable from the code documented as "daemon unreachable," so a supervisor script gating on that code no longer misfires on a typo (`error.rs:40`).
2. **Given** the flag-conflict check that currently calls `process::exit(1)` directly, **When** it fires, **Then** it returns through the crate's normal error type instead of bypassing it, and is covered by a test (`main.rs:140-141`).
3. **Given** a freshly written location config file, **When** its permissions are inspected, **Then** it is not world-readable (`location_config.rs`).
4. **Given** a forged or spoofed STUN response (e.g. from an on-path attacker), **When** IP-based geolocation processes it, **Then** the daemon does not silently accept it as authoritative without at least a documented mitigation (DNS pinning, sanity-bounding the result, or similar) (`ip_geolocation.rs:32,99-127`).
5. **Given** a burst of `PortalEvent::Reading` events, **When** they arrive faster than the existing 2s coalescer used elsewhere in the daemon, **Then** they are debounced the same way instead of triggering a synchronous disk write and re-evaluation cascade per event (`cosmic-wallpaperd.rs:123-143`).
6. **Given** a GPU adapter/device request that hangs, **When** it exceeds a bounded timeout, **Then** it fails with an error instead of freezing every managed output indefinitely, matching the protection the crate's own test suite already applies to itself (`gpu.rs:41,56`, `surface.rs:254-258`).
7. **Given** a `SurfaceError::Lost`/`Outdated` event with no accompanying compositor configure event, **When** it occurs, **Then** the affected output actively re-configures instead of silently rendering nothing forever (`surface.rs:393-399`).
8. **Given** the `unsafe` block constructing raw-window-handle FFI objects, **When** it is reviewed, **Then** it carries a comment documenting why the backing Wayland objects are guaranteed to outlive every use, per the constitution's requirement that unsafe boundaries be documented (`surface.rs:254-258`).
9. **Given** a pack with many distinct images rendered over the daemon's lifetime, **When** GPU textures accumulate, **Then** an eviction policy bounds total GPU memory instead of caching every texture forever (`surface.rs:88,341-353`).
10. **Given** the `Arc<Mutex<DbusState>>` "never contended" assumption, **When** the code is reviewed or changed, **Then** the single-thread-driving invariant it depends on is documented in the types or enforced, not left as an unstated assumption (`dbus_service.rs:13-19,86-88`).
11. **Given** a location at exactly the geographic pole, **When** a schedule query runs, **Then** it resolves without the radius-doubling search degrading to its full, pathologically slow extent on every call (`location.rs:61-66`, `query.rs:68-131`).
12. **Given** `MAX_SEARCH_RADIUS_DAYS = 370`, **When** the actual worst-case search radius is documented, **Then** the constant/comment reflects the true worst case (up to 512 days under check-then-double ordering), not an understated figure (`query.rs:62,107-130`).
13. **Given** a solar pack with duplicate-instant anchors, **When** `query()` runs without the separate duplicate-instant check having been called, **Then** a zero-width transition is caught rather than silently produced — either by folding the check into `validate()`/`query()` or making the separate call impossible to forget (`pack.rs:176-210`).

---

### User Story 8 - Code-health and documentation gaps identified by the audit are cleaned up (Priority: P4)

The remaining note-level findings are correctness-adjacent maintainability issues: copy-pasted patterns, misleading names/docs, unindexed-panic risks that are currently unreached, and documentation that doesn't mention shipped features.

**Why this priority**: None of these have a demonstrated user-facing failure mode; they reduce the odds of a *future* bug or slow down a future maintainer. Appropriate to fix once the seven stories above are stable, and safe to defer independently of them.

**Independent Test**: Each item below can be verified by inspection/lint after the change (e.g., a shared helper replaces the three copy-pasted "spawn exactly once" sites; `caps.formats[0]` is guarded; the README mentions the pack builder wizard) with no behavioral test required beyond "existing tests still pass."

**Acceptance Scenarios**:

1. **Given** `generate-starter-pack`'s manifest generation, **When** reviewed, **Then** it routes through `toml::Serialize` like `manifest.rs`'s own `render()`, instead of hand-building TOML via string interpolation (`generate-starter-pack/src/main.rs:153-166`).
2. **Given** `surface.rs`'s `caps.formats[0]` access, **When** an adapter/surface reports an empty capability list, **Then** that output degrades gracefully instead of panicking (`surface.rs:662-669`).
3. **Given** output lifecycle logic spread across four trait impls in `surface.rs`, **When** a new contributor needs to find every resize/hotplug entry point, **Then** a code comment or index ties them together.
4. **Given** the D-Bus client's error mapping, **When** any `InvalidArgs` error occurs, **Then** the real message is preserved instead of being collapsed to a generic "output not found" (`dbus_client.rs:111-119` vs `dbus_service.rs:101-112`).
5. **Given** backoff constants duplicated in `portal_location.rs` and `ip_geolocation.rs`, **When** refactored, **Then** they share a single source of truth.
6. **Given** the "spawn exactly once" pattern copy-pasted three times in `cosmic-wallpaperd.rs`, **When** refactored, **Then** a single shared helper implements it once.
7. **Given** `query.rs`'s `active_before` field, **When** renamed or documented, **Then** its "outgoing image during a transition" meaning is no longer easy to misread as "currently active."
8. **Given** `ImageId` wrapping an unbounded caller-supplied string, **When** reviewed, **Then** a length bound is documented or enforced at the boundary that constructs it (`pack.rs:16-46`).
9. **Given** the `wallpaperctl` README, **When** updated, **Then** it documents the already-implemented `location ip` subcommand (`wallpaperctl/README.md:13,28`).
10. **Given** the `dbus_client.rs` well-known-bus-name trust assumption, **When** documented, **Then** it's recorded explicitly alongside the daemon-side authorization gap from User Story 4, consistent with the project's documented single-user trust boundary (`dbus_client.rs:71-75`).
11. **Given** the `wallpaper-settings` README, **When** updated, **Then** it mentions the pack builder wizard and reflects the current test count (`wallpaper-settings/README.md:35-71,93`).
12. **Given** the "Add pack folder…" button, **When** double-clicked rapidly, **Then** an in-flight guard prevents two concurrent file-chooser dialogs (`app.rs:183-198`, `pages/packs.rs:117-119`).

---

### Edge Cases

- A fix for one finding must not reopen another: e.g., rejecting absolute-path manifest entries (US5) must not weaken the existing `../` traversal and symlink-escape protections already covered by `path_safety.rs`'s tests.
- Every regression test added for a "verified — reproduced" finding must actually fail against the pre-fix code, not just pass trivially against the post-fix code (guards against a fix that doesn't address the root cause).
- Some fixes touch code paths shared across findings (e.g., `pack_builder.rs:495-517` appears in both the path-traversal fix and the TOCTOU note) — implementation order should land the stricter validation first so later fixes in the same function don't need to be redone.
- Rate-limiting or authorizing D-Bus methods (US4) must not break legitimate single-caller use (e.g. the settings GUI or `wallpaperctl` itself issuing a normal `Reevaluate` after a config change) — the fix bounds abuse, not normal operation.
- Rejecting previously-silently-accepted input (e.g. absolute paths, empty `--output`, empty rename values) is a behavior change for any existing pack or script that happened to rely on the old permissive behavior; each such change should ship with a clear error message explaining what changed and why.
- Fixing "manifest written before Move/Keep is chosen" (US6) must still leave the wizard usable if the user genuinely wants to resume a previous session's in-progress folder.

## Requirements *(mandatory)*

### Functional Requirements

**Crash-proofing (User Story 1)**

- **FR-001**: `Color::parse` MUST reject or safely handle non-ASCII-hex input without slicing on a non-char-boundary byte offset (`manifest.rs:68-84`).
- **FR-002**: Layer-shell surface reconfiguration MUST treat a zero-length axis (from opposing anchors both being set) as "choose a size," and MUST NOT pass a zero dimension into `wgpu::Surface::configure` (`surface.rs:646-691`).
- **FR-003**: Crossfade progress values MUST be clamped to a valid range before being used in `Duration::from_secs_f64` or any `Instant` arithmetic (`surface.rs:318`).
- **FR-004**: Solar-anchor offset validation MUST reject offsets that would overflow `DateTime` arithmetic at query time, rather than only validating at construction time (`solar.rs:65`).
- **FR-005**: Each of FR-001 through FR-004 MUST ship with a regression test reproducing the documented failing input.

**No arbitrary writes from the pack builder (User Story 2)**

- **FR-006**: The pack builder's collision-rename input MUST reject values containing path separators, `..` components, or an absolute-path prefix before it is joined to the destination root (`pack_builder.rs:147,495-517,586-590`).
- **FR-007**: The pack builder's collision-rename input MUST reject an empty value rather than allowing `destination_root.join("")` to collapse to the destination root itself (`pack_builder.rs:495-517`).
- **FR-008**: A rejected rename value MUST leave the source folder and destination root unmodified.
- **FR-009**: The interval between the destination-existence check and the actual move/copy write MUST be minimized or made atomic to close the TOCTOU window on the same code path (`pack_builder.rs:495-517`).

**Resource caps on untrusted pack content (User Story 3)**

- **FR-010**: Pack loading MUST check the manifest's declared image/anchor count against `MAX_ANCHORS` before performing any per-image containment check, canonicalization, or header read (`load.rs:76-95`, `pack.rs:92-93`).
- **FR-011**: Manifest files MUST be rejected once they exceed 512 KB, checked before the file is read fully into memory (`load.rs:72-73`).
- **FR-012**: Decoded pack images MUST be checked against `device.limits().max_texture_dimension_2d` and a 256 MB decoded-byte ceiling before `create_texture` is called (`texture.rs:30-45`).
- **FR-013**: An end-to-end test MUST exercise the pack-loader → renderer path to confirm the dimension/decode-bomb boundary that pack-loader documents is actually enforced downstream (`image_check.rs:14-24`).

**Local D-Bus trust boundary (User Story 4)**

- **FR-014**: The daemon's pending-request queue for `Reevaluate`/`ReevaluateAll` MUST be bounded to at most 8 pending requests, with excess requests rate-limited or rejected rather than queued without limit (`dbus_service.rs:58-62,136-150`).
- **FR-015**: Packaging MUST ship a `dbus-1` policy file constraining access to the daemon's session-bus methods (`packaging/`, currently absent).
- **FR-016**: `QueryAll`'s response MUST be scoped so that location-derived data (active images, precise solar-transition timestamps) is not handed to an arbitrary local process with no allow-list or user visibility (`dbus_service.rs`, whole-file finding).
- **FR-017**: `output_id` and other D-Bus method string arguments MUST be validated against an explicit length/format limit before use, not left bounded only by the D-Bus message-size ceiling (`dbus_service.rs:101-112`).

**Untrusted-string sinks (User Story 5)**

- **FR-018**: `wallpaperctl list`'s default (non-JSON) output MUST escape or strip tab/newline characters in a pack's `name` field so it cannot render as fabricated additional rows; `--json` output is unaffected (`commands/list.rs:43`).
- **FR-019**: `wallpaperctl assign --output <id>` MUST reject empty or otherwise invalid identifiers instead of storing an override key that can never match a real output (`main.rs:132`).
- **FR-020**: Manifest `[[images]]` entries with an absolute-path `file` value MUST be explicitly rejected at validation time, not merely caught incidentally by a later containment check (`path_safety.rs:18`).
- **FR-021**: A test fixture MUST exercise an absolute-path manifest entry, alongside the existing `../` traversal and symlink-escape fixtures (`path_safety.rs` tests / `fixtures/invalid/path_traversal`).

**Surfacing failures instead of swallowing them (User Story 6)**

- **FR-022**: Concurrent writers to the pack registry (CLI process and in-daemon registry) MUST be serialized or MUST produce an explicit conflict error to the losing writer, instead of one silently overwriting the other (`registry.rs:183-246`).
- **FR-023**: A corrupted `location` or `renderer` config file MUST be distinguishable, in logs and/or CLI output, from "never configured" rather than silently falling back to defaults (`location_config.rs:108-110`, `renderer_config.rs:109-111`).
- **FR-024**: Pack-builder generation MUST surface `PackSource::resolve` and `registry.register` failures to the user rather than discarding them and reporting success (`pack_builder.rs:537-544`).
- **FR-025**: `location set`'s multi-field config write MUST be atomic across fields — either all fields commit or none do (`commands/location.rs:110-117`).
- **FR-026**: The pack-generation handler MUST independently re-check that every image row is assigned before generating, rather than relying solely on the UI-layer gate (`app.rs:356-360`, `pack_builder.rs:670,283-302`).
- **FR-027**: The pack builder MUST NOT leave an unregistered `manifest.toml` in the source folder in a way that is silently treated as an implicit "Keep it here" if the app is interrupted before the user chooses Move vs. Keep (`pack_builder.rs:452-474`).

**Diagnostics and defensive hardening (User Story 7)**

- **FR-028**: `wallpaperctl`'s exit code for "daemon unreachable" MUST be distinguishable from clap's own usage-error exit code (`error.rs:40`).
- **FR-029**: The flag-conflict check currently calling `process::exit(1)` directly MUST instead return through the crate's standard error/result type (`main.rs:140-141`).
- **FR-030**: Location config files MUST be written with non-world-readable permissions (`location_config.rs`).
- **FR-031**: STUN-based IP geolocation MUST apply at least one mitigation against a forged/unauthenticated UDP response (e.g. DNS pinning, sanity bounds on the resolved location, or equivalent) rather than trusting the reply outright (`ip_geolocation.rs:32,99-127`).
- **FR-032**: Portal location updates (`PortalEvent::Reading`) MUST be debounced using the same coalescing window already used for other config writes (`cosmic-wallpaperd.rs:123-143`).
- **FR-033**: GPU adapter/device requests MUST be bounded by a timeout so a hung driver call cannot freeze every output indefinitely (`gpu.rs:41,56`, `surface.rs:254-258`).
- **FR-034**: A `SurfaceError::Lost`/`Outdated` event MUST trigger active re-configuration of the affected output rather than only being logged (`surface.rs:393-399`).
- **FR-035**: The `unsafe` raw-window-handle construction MUST carry a comment documenting the lifetime/safety invariant it relies on (`surface.rs:254-258`).
- **FR-036**: GPU textures MUST be evicted under a defined policy rather than cached for the daemon's entire lifetime with no bound (`surface.rs:88,341-353`).
- **FR-037**: The `Arc<Mutex<DbusState>>` single-thread-driving invariant MUST be documented at the type/call site or enforced, not left as an implicit assumption (`dbus_service.rs:13-19,86-88`).
- **FR-038**: Solar-position queries at or near the geographic pole MUST NOT degrade to the search's full worst-case extent on every call (`location.rs:61-66`, `query.rs:68-131`).
- **FR-039**: `MAX_SEARCH_RADIUS_DAYS` and its documentation MUST reflect the actual worst-case search radius under the current check-then-double implementation (`query.rs:62,107-130`).
- **FR-040**: Duplicate-instant checking for solar-anchor packs MUST be reachable from `validate()`/`query()` directly, so it cannot be silently skipped by a call site that forgets the separate check (`pack.rs:176-210`).

**Code health and documentation (User Story 8)**

- **FR-041**: `generate-starter-pack` MUST build its manifest via `toml::Serialize`, matching `manifest.rs`'s own `render()`, instead of hand-built string interpolation (`generate-starter-pack/src/main.rs:153-166`).
- **FR-042**: `surface.rs`'s `caps.formats[0]` access MUST guard against an empty capability list instead of panicking (`surface.rs:662-669`).
- **FR-043**: Output lifecycle logic in `surface.rs` MUST be indexed (via comment or refactor) so every resize/hotplug entry point is discoverable without reading the entire file (`surface.rs`, whole file).
- **FR-044**: The D-Bus client MUST preserve the real error message for `InvalidArgs` errors instead of collapsing them to a generic "output not found" (`dbus_client.rs:111-119`, `dbus_service.rs:101-112`).
- **FR-045**: Backoff constants duplicated in `portal_location.rs` and `ip_geolocation.rs` MUST share a single source of truth.
- **FR-046**: The "spawn exactly once" pattern copy-pasted three times in `cosmic-wallpaperd.rs` MUST be factored into a single shared helper (`cosmic-wallpaperd.rs:104-119,150-161,182-191`).
- **FR-047**: `query.rs`'s `active_before` field MUST be renamed or documented so its "outgoing image during transition" meaning is not misread as "currently active" (`query.rs:29-43,133-169`).
- **FR-048**: `ImageId`'s unbounded caller-supplied string MUST have a documented or enforced length bound (`pack.rs:16-46`).
- **FR-049**: The `wallpaperctl` README MUST document the already-implemented `location ip` subcommand (`wallpaperctl/README.md:13,28`).
- **FR-050**: The well-known-bus-name trust assumption in `dbus_client.rs` MUST be documented explicitly alongside the daemon-side authorization gap (`dbus_client.rs:71-75`).
- **FR-051**: The `wallpaper-settings` README MUST mention the pack builder wizard and reflect the current test count (`wallpaper-settings/README.md:35-71,93`).
- **FR-052**: The "Add pack folder…" action MUST guard against concurrent invocation so a rapid double-click cannot open two file-chooser dialogs (`app.rs:183-198`, `pages/packs.rs:117-119`).

### Key Entities

- **Finding**: One audit-reported issue, carrying a severity (critical/warning/note), originating persona (Saboteur/Security Auditor/New Hire), a source location (crate/file/line range), and — for several — a "verified: compiled & reproduced" flag distinguishing confirmed exploits from code-reading inferences. Every FR above traces back to exactly one finding by its file:line reference, preserving that traceability into implementation.
- **Trust boundary**: The three places untrusted data crosses into the system — third-party pack manifests/images on disk, the local D-Bus session bus (any co-located same-uid process), and CLI/GUI user input — each of which multiple findings independently identify as under-validated.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All four documented reproduced panics (non-ASCII hex color, zero-size surface reconfigure, out-of-range crossfade progress, unbounded solar-anchor offset) are covered by a regression test that fails pre-fix and passes post-fix, and the daemon no longer terminates when any of them occurs.
- **SC-002**: No filesystem write triggered by the pack builder's collision-rename flow ever lands outside the configured packs destination root, verified against path-separator, `..`, absolute-path, and empty-string inputs.
- **SC-003**: A manifest declaring far more than the 64-anchor limit, a manifest file over 512 KB, and an image whose decoded size exceeds 256 MB (or whose dimensions exceed the device's `max_texture_dimension_2d`) are each rejected without performing the expensive work (per-image filesystem checks, full in-memory read, or GPU upload) that currently precedes rejection.
- **SC-004**: Repeated local D-Bus calls from a single unauthorized co-located process cannot push the daemon's `Reevaluate`/`ReevaluateAll` pending queue past 8 entries, bounding worst-case memory and redraw backlog from that vector.
- **SC-005**: 100% of the 11 critical and 25 warning findings from the audit have a corresponding fix and, where the finding described a specific reproducible scenario, a regression test covering it.
- **SC-006**: Every previously-silent failure path identified by the audit (registry write conflicts, corrupted config files, pack-builder registry-registration failures) now produces a log entry or user-visible error when the failure occurs.
- **SC-007**: A follow-up adversarial review of the same six subsystems returns a verdict better than BLOCK, with zero outstanding critical findings.

## Assumptions

- The project's existing single-user desktop trust model (documented in the constitution and by the audit itself, e.g. the D-Bus client's bus-name-owner trust) is not being replaced by a hardened multi-user sandbox; fixes here are defense-in-depth against buggy or malicious *pack content* and *other local processes*, not a redesign of the trust model itself.
- All 52 findings (11 critical, 25 warning, 16 note) are in scope for this initiative, ordered by the priority levels above; User Story 8's note-level findings are lowest priority and may be deferred to a follow-up pass if time-boxed, without blocking the higher-priority stories.
- Fixes are expected to be achievable within the existing crate/dependency set (`image`, `wgpu`, `zbus`, `toml`, etc.) unless implementation discovers a specific finding requires a new dependency.
- No on-disk manifest or config schema version bump is assumed necessary; several fixes tighten validation (reject previously-accepted input) rather than change the schema shape. Any fix that does turn out to require a schema change must follow the constitution's versioned-migration requirement (Principle X).
- File:line references throughout this spec reflect `main` @ `50ca3f6`; they are expected to drift as earlier fixes land and should be re-located at implementation time rather than trusted literally once other findings in the same file have already been fixed.
- "Rate-limit or authorize" (FR-014, FR-015) is satisfied by either mechanism (or both); the spec does not mandate a specific authorization scheme (polkit, a custom token, uid-based `dbus-1` policy, etc.) — that choice is left to planning.
