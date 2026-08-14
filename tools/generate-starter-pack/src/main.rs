//! Maintainer-only starter-pack image generator (spec 7 US2, research.md R5, T042).
//!
//! Produces a fixed, deterministic set of procedurally-generated gradient/sky-art PNGs
//! (no photography, no third-party content — spec.md FR-009) spanning a full
//! solar-anchored day cycle, plus the `manifest.toml` referencing them (spec 2's
//! existing pack format, reused unchanged — no new pack-format code anywhere). Run
//! once; its own output is not a dependency of `wallpaperd`/`wallpaperctl`/
//! `wallpaper-settings` at build or runtime.
//!
//! **Note**: `assets/starter-pack/` is no longer this tool's own output — it was
//! deliberately swapped post-launch (2026-08-14) for a hand-authored illustrated
//! "Mountains" pack the project maintainer supplied directly, which better matches the
//! project's intended look than the placeholder gradients this tool produces. This
//! tool is kept as a working fallback-generator (e.g. if the bundled pack's licensing
//! or sourcing ever needs revisiting), not as the source of the currently-shipped
//! asset — don't assume re-running it reproduces `assets/starter-pack/`'s current
//! contents.
//!
//! ```sh
//! cargo run -p generate-starter-pack -- /some/output/dir
//! ```

use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

/// One frame of the day cycle: its anchor (spec 2 manifest anchor grammar), output
/// file name, top/bottom sky gradient colors, and an optional glow (position as a
/// fraction of image height, color, radius as a fraction of image width) standing in
/// for the sun/moon — deliberately simple procedural shapes, not an attempt at
/// photorealism.
struct Frame {
    anchor: &'static str,
    file: &'static str,
    top: Rgb<u8>,
    bottom: Rgb<u8>,
    glow: Option<Glow>,
}

struct Glow {
    /// Vertical position, 0.0 = top of image, 1.0 = bottom.
    y_fraction: f32,
    color: Rgb<u8>,
    radius_fraction: f32,
}

fn frames() -> Vec<Frame> {
    vec![
        Frame {
            anchor: "astronomical_dawn",
            file: "01-astronomical-dawn.png",
            top: Rgb([8, 10, 28]),
            bottom: Rgb([28, 24, 58]),
            glow: None,
        },
        Frame {
            anchor: "civil_dawn",
            file: "02-civil-dawn.png",
            top: Rgb([30, 34, 74]),
            bottom: Rgb([120, 90, 110]),
            glow: Some(Glow { y_fraction: 1.05, color: Rgb([255, 190, 140]), radius_fraction: 0.55 }),
        },
        Frame {
            anchor: "sunrise",
            file: "03-sunrise.png",
            top: Rgb([120, 150, 210]),
            bottom: Rgb([255, 170, 110]),
            glow: Some(Glow { y_fraction: 0.92, color: Rgb([255, 235, 180]), radius_fraction: 0.4 }),
        },
        Frame {
            anchor: "solar_noon",
            file: "04-solar-noon.png",
            top: Rgb([60, 140, 230]),
            bottom: Rgb([180, 220, 250]),
            glow: Some(Glow { y_fraction: 0.08, color: Rgb([255, 255, 240]), radius_fraction: 0.28 }),
        },
        Frame {
            anchor: "sunset",
            file: "05-sunset.png",
            top: Rgb([70, 70, 140]),
            bottom: Rgb([255, 120, 90]),
            glow: Some(Glow { y_fraction: 0.9, color: Rgb([255, 210, 150]), radius_fraction: 0.42 }),
        },
        Frame {
            anchor: "civil_dusk",
            file: "06-civil-dusk.png",
            top: Rgb([20, 18, 50]),
            bottom: Rgb([130, 60, 90]),
            glow: Some(Glow { y_fraction: 1.05, color: Rgb([220, 120, 120]), radius_fraction: 0.5 }),
        },
        Frame {
            anchor: "astronomical_dusk",
            file: "07-astronomical-dusk.png",
            top: Rgb([6, 8, 24]),
            bottom: Rgb([26, 22, 54]),
            glow: None,
        },
        Frame {
            anchor: "solar_midnight",
            file: "08-solar-midnight.png",
            top: Rgb([2, 3, 12]),
            bottom: Rgb([10, 10, 26]),
            glow: None,
        },
    ]
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

fn lerp_rgb(a: Rgb<u8>, b: Rgb<u8>, t: f32) -> Rgb<u8> {
    Rgb([lerp(a.0[0], b.0[0], t), lerp(a.0[1], b.0[1], t), lerp(a.0[2], b.0[2], t)])
}

fn render(frame: &Frame) -> RgbImage {
    let mut img = RgbImage::new(WIDTH, HEIGHT);

    for y in 0..HEIGHT {
        let t = y as f32 / (HEIGHT.saturating_sub(1).max(1)) as f32;
        let sky = lerp_rgb(frame.top, frame.bottom, t);
        for x in 0..WIDTH {
            img.put_pixel(x, y, sky);
        }
    }

    if let Some(glow) = &frame.glow {
        let cx = WIDTH as f32 / 2.0;
        let cy = HEIGHT as f32 * glow.y_fraction;
        let radius = WIDTH as f32 * glow.radius_fraction;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < radius {
                    // Soft falloff toward the glow's edge, additively blended over
                    // the sky gradient already drawn.
                    let strength = (1.0 - dist / radius).powf(1.8);
                    let existing = *img.get_pixel(x, y);
                    img.put_pixel(x, y, lerp_rgb(existing, glow.color, strength));
                }
            }
        }
    }

    img
}

fn manifest_toml(frames: &[Frame]) -> String {
    let mut out = String::new();
    out.push_str("schema_version = 1\n");
    out.push_str("name = \"Solar Gradient\"\n");
    out.push_str("author = \"dynamic-wallpaper project — procedurally generated, no photography, CC0\"\n");
    out.push_str("default_scaling = \"Fill\"\n");
    out.push_str("fallback_color = \"#05050f\"\n");
    for frame in frames {
        out.push_str("\n[[images]]\n");
        out.push_str(&format!("file = \"{}\"\n", frame.file));
        out.push_str(&format!("anchor = \"{}\"\n", frame.anchor));
    }
    out
}

fn main() {
    let out_dir: PathBuf = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("assets/starter-pack"));

    if let Err(e) = run(&out_dir) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    println!("wrote starter pack to {}", out_dir.display());
}

fn run(out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(out_dir)?;

    let frames = frames();
    for frame in &frames {
        let img = render(frame);
        img.save(out_dir.join(frame.file))?;
    }

    std::fs::write(out_dir.join("manifest.toml"), manifest_toml(&frames))?;
    Ok(())
}
