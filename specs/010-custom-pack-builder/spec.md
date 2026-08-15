# Feature Specification: Custom Pack Builder

**Feature Branch**: `010-custom-pack-builder`

**Created**: 2026-08-15

**Status**: Draft

**Input**: User description: "I want to give the user the ability to open a folder of images to create a custom pack from within the GUI and assign the images to different solar periods or specific times of the day. When the user selects a folder of images, if no manifest file exists, the user should select whether they want to configure the folder by solar periods or specific times. If solar periods are selected, thumbnails of each image should be presented and a dropdown style menu presented beside each image so that the solar period can be assigned. There should also be a box for time offset where the user can set +/- h/m from the selected solar period. If the user selects to configure based on specific times, thumbnails of each image should be presented and a time chooser should be presented beside each thumbnail so the user can set the time that they want each image to be displayed. When the user is satisfied with their selections, they will press a "Generate" button which will create a manifest.toml file in the folder and ask the user if they want to move the folder to the default location for image packs and remove it from the source location or keep it in the location it is currently in. Generating the pack will also need to allow the user to enter the author's name and should prompt the user to put in the artist's name if known or leave it as "Artist Unknown"."

## Clarifications

### Session 2026-08-15

- Q: When a generated pack is moved to the standard pack location but a pack with that folder name already exists there, what should happen? → A: Prompt the user to enter a different destination name before proceeding.
- Q: When the solar-period or specific-time configuration screen first appears, what should each image's assignment control start as? → A: Unassigned by default — the user must actively choose an assignment for every image; Generate stays disabled until every row has one.
- Q: Is there a maximum magnitude for the +/- hour/minute offset next to a solar-period assignment? → A: Yes, capped at ±12h (half a day) in either direction.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Build a pack by solar period (Priority: P1)

A user has a folder of wallpaper images with no scheduling information attached. They open the folder from within the app, choose to configure it "by solar period," see a thumbnail of every image with a solar-event selector next to it, assign each image to a solar event (optionally nudged earlier/later by an offset), and generate a working pack.

**Why this priority**: This is the primary path described by the request and mirrors how the project's own starter pack and documentation already describe packs (anchored to sunrise/sunset/etc.) — most hand-picked photo sets naturally map to "time of day" concepts like dawn/noon/dusk rather than exact clock times.

**Independent Test**: Point the wizard at a folder containing several images and no `manifest.toml`, assign each image to a distinct solar event, click Generate, and confirm a valid, loadable pack results with every image scheduled to the chosen solar event (and offset, where set).

**Acceptance Scenarios**:

1. **Given** a folder of images with no `manifest.toml`, **When** the user opens it from the pack-creation flow, **Then** the app asks whether to configure by solar period or by specific time before showing anything else.
2. **Given** the user chose "solar period," **When** the configuration screen appears, **Then** every image in the folder is shown as a thumbnail with an adjacent selector listing the recognized solar events (sunrise, sunset, solar noon, solar midnight, civil dawn, civil dusk, astronomical dawn, astronomical dusk).
3. **Given** an image's solar event is selected, **When** the user sets an hour/minute offset (positive or negative) next to it, **Then** that image's schedule reflects the event shifted by exactly that offset.
4. **Given** every image has a solar event assigned, **When** the user clicks "Generate," **Then** a `manifest.toml` is written that a pack loader can read back with each image scheduled exactly as configured.
5. **Given** two images are assigned to the same solar event with the same offset (or no offset on both), **When** the user attempts to generate, **Then** the app flags the conflict and does not generate an invalid manifest until it's resolved.
6. **Given** the configuration screen has just appeared, **When** the user looks at any row before making a choice, **Then** that row's solar-event selector shows no default selection, and "Generate" stays disabled until every row has one.

---

### User Story 2 - Build a pack by specific time of day (Priority: P2)

A user wants each image to appear at an exact clock time regardless of sunrise/sunset (e.g., no location configured, or a deliberately fixed schedule). They choose "specific times" instead, set a time for each thumbnail, and generate the pack.

**Why this priority**: Necessary alternative for users without a configured location or who want fixed schedules, but secondary to the solar-period path most users are expected to take.

**Independent Test**: Point the wizard at a folder of images, choose "specific times," assign a distinct time of day to each thumbnail, click Generate, and confirm a valid pack results with every image scheduled to its exact chosen time.

**Acceptance Scenarios**:

1. **Given** the user chose "specific times," **When** the configuration screen appears, **Then** every image is shown as a thumbnail with an adjacent time-of-day selector (no solar-event selector or offset control present).
2. **Given** a time is set for each image, **When** the user clicks "Generate," **Then** a `manifest.toml` is written that a pack loader can read back with each image scheduled to exactly that clock time.
3. **Given** two images are set to the exact same time, **When** the user attempts to generate, **Then** the app flags the conflict and does not generate an invalid manifest until it's resolved.
4. **Given** the configuration screen has just appeared, **When** the user looks at any row before making a choice, **Then** that row's time selector shows no default time, and "Generate" stays disabled until every row has one.

---

### User Story 3 - Name the pack's author and choose where it lives (Priority: P3)

Having finished assigning images, the user supplies an author/artist name (or leaves it unset), generates the pack, and decides whether the finished pack should move into the app's standard pack storage (removed from its original folder location) or stay exactly where it is. Either way, the new pack shows up as available immediately.

**Why this priority**: Completes the flow end-to-end but is meaningful only after a mode and per-image assignments already exist (US1 or US2), so it depends on — and is tested on top of — one of those.

**Independent Test**: With a fully-assigned draft pack (from either US1 or US2) ready to generate, verify the author-name prompt behaves correctly with both a supplied name and a blank one, and that both the "move" and "keep in place" choices leave the pack in a working, registered state afterward.

**Acceptance Scenarios**:

1. **Given** the user is about to generate, **When** the author-name field is presented, **Then** it is pre-filled or clearly labeled with "Artist Unknown" as what will be used if left blank.
2. **Given** the user leaves the author field blank, **When** the pack is generated, **Then** the manifest's author is recorded as "Artist Unknown."
3. **Given** the user enters a name, **When** the pack is generated, **Then** the manifest's author is recorded as exactly that name.
4. **Given** generation just succeeded, **When** the app follows up, **Then** the user is asked to either move the pack folder into the app's standard pack location (no longer present at the original path) or leave it where it is.
5. **Given** either choice in Scenario 4, **When** the prompt is resolved, **Then** the generated pack appears in the user's list of available packs without any further manual step.
6. **Given** the user chose to move the folder, **When** a pack folder with the same name already exists at the standard pack location, **Then** the user is prompted to enter a different destination name before the move proceeds, rather than anything being silently overwritten or auto-renamed.

---

### Edge Cases

- Folder selected already contains a `manifest.toml`: the solar-period/specific-time wizard does not apply; the app treats it as an existing pack instead of prompting for configuration mode.
- Folder contains no usable image files: generation is blocked with a clear explanation rather than producing an empty or broken manifest.
- Folder contains more images than a single pack may hold: the app tells the user the limit was exceeded rather than silently truncating or failing generation without explanation.
- Folder contains non-image files alongside the images: they are ignored — not shown as thumbnails, not blocking, not included in the manifest.
- An image file is present but can't actually be read/decoded: it's flagged to the user and excluded rather than silently causing generation to fail partway through.
- User switches from "solar period" to "specific time" (or back) after making some assignments: assignments made under the abandoned mode are discarded, since one pack cannot mix solar and clock scheduling.
- User cancels the wizard at any point before clicking "Generate": no `manifest.toml` is written and the source folder is left completely unmodified.
- Writing `manifest.toml` fails (e.g., the folder isn't writable): the user sees a specific error and their in-progress assignments are preserved so they can retry or pick a different folder, rather than losing their work.
- User chooses to move the folder to the standard pack location, but the move fails (e.g., destination full or inaccessible): the original folder and its newly-written manifest remain intact and usable rather than being left in a half-moved, unusable state.
- A folder with the same name already exists at the standard pack location: the user is prompted for a different destination name rather than the move overwriting or silently auto-renaming anything.
- The author name entered contains unusual characters (quotes, non-Latin scripts, emoji): it's stored and displayed correctly rather than corrupting the manifest file.
- The user enters (or drags) a solar-period offset beyond ±12h: the control rejects or clamps the value rather than accepting an out-of-range shift.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST let the user pick a folder from the local filesystem as the source for a new custom pack.
- **FR-002**: The system MUST detect whether the selected folder already contains a `manifest.toml` and, if so, MUST NOT run the configuration wizard — the folder is treated as an already-configured pack instead.
- **FR-003**: When no manifest is present, the system MUST scan the folder for supported image files and present a thumbnail preview of each one found.
- **FR-004**: Before any image is assigned, the system MUST let the user choose exactly one configuration mode for the whole folder: "solar period" or "specific time" — the two are mutually exclusive within a single pack.
- **FR-005**: In solar-period mode, the system MUST present, next to every image's thumbnail, a selector listing the recognized solar events: sunrise, sunset, solar noon, solar midnight, civil dawn, civil dusk, astronomical dawn, and astronomical dusk.
- **FR-006**: In solar-period mode, the system MUST let the user independently set a signed hour/minute offset (e.g., "-30m," "+1h15m") from the chosen solar event for each image, constrained to a maximum magnitude of 12 hours in either direction.
- **FR-007**: In specific-time mode, the system MUST present, next to every image's thumbnail, a time-of-day selection control the user can set independently per image.
- **FR-008**: The system MUST detect when two images in the same draft would resolve to the exact same scheduling instant and MUST prevent generating a manifest while that conflict is unresolved.
- **FR-009**: The system MUST show every image's assignment control unassigned (no default guess) when the configuration screen first appears, and MUST require every scanned image to have an explicit scheduling assignment (solar event, or time) before the "Generate" action becomes available.
- **FR-010**: The system MUST let the user enter an author/artist name as part of generating the pack, clearly indicating that leaving it blank results in "Artist Unknown."
- **FR-011**: When the user activates "Generate," the system MUST write a `manifest.toml` into the source folder reflecting the chosen mode, every image's assignment, and the author name (or "Artist Unknown" if left blank).
- **FR-012**: The generated manifest MUST be immediately valid and loadable — every value written MUST conform to the pack manifest format's requirements (structural validity, no duplicate/conflicting instants, no invalid field values).
- **FR-013**: After a successful generation, the system MUST ask the user whether to move the pack folder to the application's standard pack storage location (removing it from its original location) or leave it in place.
- **FR-014**: If the user chooses to move the folder, the system MUST relocate the entire folder — every file in it, not only the images referenced by the manifest — to the standard location, and it MUST no longer be present at the original path.
- **FR-014a**: If a folder with the same name already exists at the standard pack storage location, the system MUST prompt the user to enter a different destination name before completing the move, rather than overwriting or silently auto-renaming the destination.
- **FR-015**: If the user chooses to keep the folder in place, the system MUST leave it completely untouched at its original location aside from the new `manifest.toml`.
- **FR-016**: Regardless of which location choice is made, the newly generated pack MUST become available in the user's list of packs without requiring any separate manual registration step.
- **FR-017**: If writing the manifest or moving the folder fails, the system MUST show a specific, actionable error and MUST leave the folder (and any manifest already written) in a consistent, non-corrupted state.
- **FR-018**: The system MUST reject folders with zero usable images and folders whose image count exceeds the pack format's maximum, explaining why generation can't proceed.
- **FR-019**: The system MUST let the user cancel the wizard at any point before "Generate" without writing a manifest or otherwise modifying the source folder.
- **FR-020**: The system MUST populate every other field the manifest format requires (display name, default scaling, fallback color) with sensible defaults so the generated manifest is complete and valid without the user having to specify them.

### Key Entities

- **Custom Pack Draft**: The in-progress, not-yet-saved state of the wizard — the source folder, the chosen configuration mode, every image's current assignment, and the author name entered so far.
- **Image Thumbnail Row**: One scanned image's preview plus its current scheduling assignment and the control used to set it (solar-event selector and offset, or time selector), depending on the active mode.
- **Solar Period Assignment**: A named solar event plus an optional signed hour/minute offset, applied to exactly one image.
- **Time Assignment**: An absolute time of day, applied to exactly one image.
- **Generated Manifest**: The `manifest.toml` produced by "Generate" — pack metadata (display name, author, default scaling, fallback color) plus the full list of image-to-schedule assignments.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user with a folder of 8 unlabeled images and no prior manifest can produce a fully working custom pack in under 5 minutes without consulting outside documentation.
- **SC-002**: 100% of manifests produced by the wizard load successfully as valid packs, with no manual editing ever required afterward.
- **SC-003**: 100% of attempts to generate a pack with a scheduling conflict (duplicate instant) or missing assignment are caught before any manifest is written — no invalid manifest is ever produced.
- **SC-004**: When the author field is left blank, 100% of generated packs display "Artist Unknown" as the author.
- **SC-005**: After generation, the new pack appears in the user's available-packs list within a few seconds, with no separate manual "add pack" step needed, regardless of whether the user chose to move it or keep it in place.
- **SC-006**: A cancelled wizard session leaves the source folder byte-for-byte unchanged — verified by the absence of any new or modified files in it.

## Assumptions

- "Supported image files" means whatever image formats the rest of the application already accepts for pack images (per the existing pack manifest format) — this feature does not expand or restrict that set.
- The pack's display name (the manifest's required `name` field) defaults to the source folder's name and is not separately edited as part of this wizard; renaming a generated pack afterward is out of scope for this feature.
- Default scaling and fallback color — both required manifest fields — are filled with the application's existing standard defaults (matching what a freshly hand-authored pack would reasonably use) rather than being exposed as choices in this wizard.
- A folder that already contains a `manifest.toml` is out of scope for this wizard entirely; opening such a folder is handled by the existing "add an already-configured pack" experience, not this feature.
- "The application's standard pack storage location" refers to a single, consistent, application-managed location already understood by the rest of the app for holding user packs; this feature does not introduce a new concept of where packs live, only the choice of whether a given pack ends up there.
- The maximum number of images a single pack may contain, and the rule that one pack cannot mix solar-event and clock-time scheduling, both follow the existing pack format's established limits rather than introducing new ones.
