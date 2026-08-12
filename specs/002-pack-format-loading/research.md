# Research: Pack Format & Loading

## R1. TOML parsing

**Decision**: [`toml`](https://github.com/toml-rs/toml) (1.x, MIT/Apache-2.0) with `serde`
derive, matching the clarified manifest format (FR-002).

**Rationale**: The reference TOML implementation for Rust — ~810M downloads, actively
maintained (last publish July 2026), standard `Serialize`/`Deserialize` integration so the
manifest schema is just a `#[derive(Deserialize)]` struct with no hand-written parser.

**Alternatives considered**: `toml_edit` — preserves formatting/comments for round-trip
*writing*, which this spec doesn't need (manifests are read-only input; nothing in spec.md
requires the loader to rewrite a user's manifest).

## R2. Validating an image file is actually readable

**Decision**: [`image`](https://github.com/image-rs/image) (0.25.x, MIT/Apache-2.0), using
`ImageReader::open(path)?.with_guessed_format()?.into_dimensions()` — enough to confirm the
file is a decodable image of a real format and read its header, without decoding full pixel
data.

**Rationale**: FR-006/User Story 2 Scenario 2 require rejecting a file that "is not a
readable image" with a clear error. A full pixel decode of every image in a 64-image pack
(FR-001 cap, inherited from spec 1) on every load would be wasted work — this spec only
needs to *validate* readability, not render, so a header-only probe is sufficient and keeps
load time low (see R5). `image` is the de facto standard decode library in the Rust
ecosystem (~166M downloads, MIT/Apache-2.0, actively maintained) — the same crate the
renderer (spec 3) will use for actual pixel decode, so no second image library enters the
dependency tree later.

**Alternatives considered**: Sniffing magic bytes by hand — rejected; that's exactly the
kind of hand-rolled parsing the constitution's spirit (vetted crates over bespoke parsing,
Principle V by analogy) argues against, and `image`'s guessed-format probe already does this
correctly across formats.

## R3. Preventing a manifest from referencing a file outside the pack directory (FR-006a)

**Decision**: `std::fs::canonicalize` both the pack directory and the resolved image path,
then confirm the canonicalized image path `starts_with` the canonicalized pack directory —
no additional crate required.

**Rationale**: This is the standard, well-documented technique for exactly this problem
(canonicalize resolves `..` components *and* symlinks to their real target, so a symlink
planted inside the pack directory that points outside it is caught too, not just a literal
`../` in the manifest). `canonicalize` requires the target to exist, which is not a
limitation here — FR-006 already requires rejecting a reference to a non-existent file
before this check would even run, so existence is already guaranteed by the time containment
is checked.

**Alternatives considered**: `path_jail` (a small sandboxing crate) — found during research
but is very new/low-adoption; the standard-library technique is well-established, sufficient,
and avoids a dependency for something four lines of code handle correctly.

## R4. Pack registry persistence (FR-010, FR-011, FR-012)

**Decision**: `cosmic-config` (git dependency on `pop-os/libcosmic`, not published to
crates.io — confirmed by its own `Cargo.toml`, version 1.0.0 as of this research), using its
`CosmicConfigEntry` versioned-entry pattern for the pack registry.

**Rationale**: This is the constitution's mandated persistence layer (Principle IV) — using
it directly, rather than hand-rolling RON file I/O, gets versioning conventions and
file-watching (via the `notify` crate it already pulls in) for free, consistent with how
`cosmic-bg` and `cosmic-settings` persist their own state. Being a git dependency (not
crates.io) is a build-environment fact to plan around (network access needed to fetch it, or
a vendored/pinned checkout), not a design concern for this spec.

**Alternatives considered**: Hand-rolled RON file read/write — rejected; duplicates what
`cosmic-config` already provides and risks the "second live-reloaded config format" problem
Principle IV specifically warns against.

**Scope note**: `cosmic-config`'s versioning covers the *registry* (the list of known pack
locations, FR-010) — it does not, by itself, version the *pack manifest* TOML files
themselves, since those live outside `cosmic-config`'s store entirely (R5).

## R5. Manifest schema versioning & migration (FR-007) — distinct from R4

**Decision**: A `schema_version: u32` field at the top of every manifest, checked against
the loader's supported version(s) at parse time; a small `match` over old versions applying
a migration function to reach the current in-memory shape, kept as ordinary Rust code (no
migration-framework crate needed at this schema's size).

**Rationale**: The manifest TOML is a separate serialization boundary from `cosmic-config`'s
RON store (R4) — it's read once per pack load, not watched/live-reloaded — so it needs its
own explicit version field and migration path per constitution Principle X / NFR-5, distinct
from whatever versioning `cosmic-config` applies to the registry itself. Keeping both
migration stories explicit (rather than assuming one covers the other) is the point of
calling this out separately.

## R6. Test fixtures

**Decision**: Committed fixture directories under `crates/pack-loader/tests/fixtures/`
(valid packs, malformed manifests, path-traversal attempts, missing/corrupt images) for
read-only loading tests; [`tempfile`](https://github.com/Stebalien/tempfile) as a
dev-dependency for the registry persistence round-trip tests (FR-010–FR-012), which need a
writable, disposable directory per test run rather than a fixture checked into the repo.

**Rationale**: Fixture directories make malformed-input test cases inspectable and diffable
in review, which matters for a spec whose entire job is rejecting bad input correctly.
`tempfile` is the standard, widely-used crate for exactly the registry-persistence case,
where writing into a fixture directory would leave test runs stateful/non-repeatable.

## R7. Pack-load performance target (resolves the Outstanding item from `/speckit-clarify`)

**Decision**: Loading a manifest-based pack of the maximum size (64 images, FR-001's cap
inherited from spec 1) — parsing the manifest, checking each image's existence, containment
(R3), and format-readability (R2, header-only) — completes in well under 500ms on typical
local storage (not network/removable media).

**Rationale**: This is I/O-bound, unlike spec 1's pure in-memory computation, so it can't
reuse spec 1's sub-millisecond bound. 500ms is generous for 64 small header reads plus one
TOML parse on any storage a desktop user would realistically point the daemon at, while still
being a real, testable ceiling rather than the unquantified "single load operation" language
the spec's Success Criteria used before this bound. This does *not* include full pixel
decode of every image (R2) — that cost belongs to the renderer (spec 3), which decodes only
the images actually being displayed, not the whole pack up front.
