# Wallpaper Pack Manifest Schema

This document describes the `manifest.toml` format for a Cosmic Dynamic Wallpaper pack. It's
written for pack authors — you don't need to read this project's source code or know
Rust to hand-author a valid pack.

## Directory layout

A pack is a directory containing a `manifest.toml` file plus the image files it
references:

```
my-pack/
├── manifest.toml
├── dawn.jpg
├── noon.jpg
└── dusk.jpg
```

Image files in the directory that aren't mentioned in `manifest.toml` are ignored — you
can keep source files, alternates, or drafts alongside the pack without them being
treated as an error.

If you just want a single, unchanging wallpaper with no manifest at all, point the
daemon directly at one image file instead of a directory — see "Zero-config static
wallpapers" below.

## `manifest.toml` reference

```toml
schema_version = 1
name = "Example Pack"
author = "Jane Author <jane@example.com> — CC-BY-4.0"
default_scaling = "Fill"
fallback_color = "#000000"

[[images]]
file = "dawn.jpg"
anchor = "sunrise"

[[images]]
file = "noon.jpg"
anchor = "solar_noon"
scaling = "Fit"

[[images]]
file = "dusk.jpg"
anchor = "sunset-30m"
```

### Top-level fields

| Field | Type | Required | Notes |
|---|---|---|---|
| `schema_version` | integer | yes | Currently must be `1`. A newer loader may support future versions with a documented migration; an older loader rejects a `schema_version` it doesn't understand yet, with a clear error — it will never silently guess. |
| `name` | string | yes | Display name for the pack. Two packs may share a `name` without conflict — a pack's real identity is its directory location, not this field. |
| `author` | string | no | Free-form author/license note (e.g. `"Jane Author — CC-BY-4.0"`). |
| `default_scaling` | string | yes | One of `Fill`, `Fit`, `Stretch`, `Center` (case-insensitive). Applies to every image that doesn't declare its own `scaling`. |
| `fallback_color` | string | yes | `#RRGGBB` or `#RRGGBBAA` hex color, used to fill any letterboxed edges left by `Fit`/`Center` scaling. |
| `images` | array of tables | yes | One `[[images]]` entry per image, in any order — see below. |

### `[[images]]` entries

| Field | Type | Required | Notes |
|---|---|---|---|
| `file` | string | yes | Path to the image, relative to the manifest's own directory. Must stay inside that directory — `..`, an absolute path, or a symlink pointing outside the pack is rejected. |
| `anchor` | string | yes | When this image becomes active — see "Anchor grammar" below. |
| `scaling` | string | no | Overrides `default_scaling` for this one image. |

**All images in one pack must use the same *kind* of anchor** — either every image is
anchored to a solar event, or every image is anchored to a clock time. Mixing the two
kinds in one manifest is rejected.

### Anchor grammar

An anchor string is one of:

**A solar event name**, optionally offset by a signed duration:

```
sunrise
sunset
solar_noon
solar_midnight
civil_dawn
civil_dusk
astronomical_dawn
astronomical_dusk
```

Append `+<duration>` or `-<duration>` to offset it — e.g. `civil_dawn-30m` (thirty
minutes before civil dawn) or `sunset+1h` (one hour after sunset). The duration accepts
any combination of hours/minutes/seconds understood by
[`humantime`](https://docs.rs/humantime) — `30m`, `1h`, `1h30m`, `45s`.

Solar anchors require the daemon to have a location configured (manually-entered
latitude/longitude); no anchor line in the manifest itself sets location.

**An absolute clock time**, `HH:MM` or `HH:MM:SS` (24-hour, local time):

```
06:00
18:30
23:15:00
```

Clock anchors require no location at all.

### Duplicate anchors

Two images resolving to the exact same instant (e.g. two images both anchored to plain
`sunrise` with no offset) is rejected — pick a different event, add an offset, or merge
the images.

## Zero-config static wallpapers

If you just want one unchanging image, skip the manifest and directory structure
entirely — point the daemon straight at the image file. It's treated as a single,
always-active image, matching a traditional desktop wallpaper picker exactly.

## Errors you might see

Every rejection names the specific file, field, or value at fault rather than a generic
failure:

- A malformed `manifest.toml` (bad TOML syntax) — fails to parse, names the parse error.
- A `file` entry naming an image that isn't present in the directory.
- A `file` entry that resolves outside the pack directory.
- An image file that exists but isn't a readable/decodable image.
- An `anchor` string that doesn't match the grammar above.
- A `scaling` or `default_scaling` value that isn't `Fill`/`Fit`/`Stretch`/`Center`.
- A `fallback_color` that isn't a valid `#RRGGBB`/`#RRGGBBAA` hex string.
- A `schema_version` newer than the installed daemon understands.
- Mixed anchor kinds (some images solar-anchored, others clock-anchored) in one pack.
- Two images resolving to the exact same instant.

None of these crash the daemon or produce a partially-loaded pack — a rejected pack is
simply not loaded, with the rest of your other known packs unaffected.
