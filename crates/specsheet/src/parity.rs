//! Visual parity checks against the real game (`docs/PARITY.md`, "Visual
//! checks"): `specsheet --parity [cases.json] [case…]` renders each case of
//! the manifest through the dev script mode, then compares the screenshot
//! with the user's reference capture of the real game. References and
//! outputs stay local in `research/parity/` (gitignored: rendered game
//! content).

use std::path::{Path, PathBuf};
use std::process::Command;

use image::{GrayImage, Rgb, RgbImage};
use serde::Deserialize;

const DEFAULT_CASES: &str = "parity/cases.json";
const REFERENCE_DIR: &str = "research/parity";
const OUTPUT_DIR: &str = "research/parity/out";
/// SSIM window side in pixels.
const WINDOW: u32 = 8;

#[derive(Debug, Deserialize)]
struct Manifest {
    /// Window size the cases and references use.
    resolution: [u32; 2],
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    truck: String,
    #[serde(default)]
    drive: Option<String>,
    /// Extra dev-script lines run after the truck has loaded.
    #[serde(default)]
    steps: Vec<String>,
    /// Pixel rectangles `[x0, y0, x1, y1]` left out of the comparison.
    #[serde(default)]
    mask: Vec<[u32; 4]>,
    /// Highest mean absolute error (0..1) that passes.
    threshold: f32,
}

/// How close a render is to the reference, over unmasked pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    /// Mean absolute luma difference, 0 (equal) to 1.
    pub mae: f32,
    /// Mean structural similarity over 8 × 8 windows, 1 = equal.
    pub ssim: f32,
}

fn masked(mask: &[[u32; 4]], x: u32, y: u32) -> bool {
    mask.iter()
        .any(|[x0, y0, x1, y1]| x >= *x0 && x < *x1 && y >= *y0 && y < *y1)
}

/// Compares two equally sized greyscale images outside `mask`.
pub fn compare(
    rendered: &GrayImage,
    reference: &GrayImage,
    mask: &[[u32; 4]],
) -> Result<Score, String> {
    if rendered.dimensions() != reference.dimensions() {
        return Err(format!(
            "size {:?} differs from the reference {:?}",
            rendered.dimensions(),
            reference.dimensions()
        ));
    }
    let (w, h) = rendered.dimensions();
    let value = |image: &GrayImage, x, y| f32::from(image.get_pixel(x, y).0[0]) / 255.0;
    let mut error = 0.0f64;
    let mut count = 0u64;
    for y in 0..h {
        for x in 0..w {
            if !masked(mask, x, y) {
                error += f64::from((value(rendered, x, y) - value(reference, x, y)).abs());
                count += 1;
            }
        }
    }
    if count == 0 {
        return Err("the mask covers the whole image".to_owned());
    }
    // SSIM per window (luma range 1), skipping windows that touch the mask.
    let (c1, c2) = (0.01f32.powi(2), 0.03f32.powi(2));
    let mut ssim = 0.0f64;
    let mut windows = 0u64;
    for wy in (0..h.saturating_sub(WINDOW - 1)).step_by(WINDOW as usize) {
        for wx in (0..w.saturating_sub(WINDOW - 1)).step_by(WINDOW as usize) {
            let pixels: Vec<(u32, u32)> = (wy..wy + WINDOW)
                .flat_map(|y| (wx..wx + WINDOW).map(move |x| (x, y)))
                .collect();
            if pixels.iter().any(|&(x, y)| masked(mask, x, y)) {
                continue;
            }
            let n = pixels.len() as f32;
            let a: Vec<f32> = pixels.iter().map(|&(x, y)| value(rendered, x, y)).collect();
            let b: Vec<f32> = pixels
                .iter()
                .map(|&(x, y)| value(reference, x, y))
                .collect();
            let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
            let va = a.iter().map(|v| (v - ma).powi(2)).sum::<f32>() / n;
            let vb = b.iter().map(|v| (v - mb).powi(2)).sum::<f32>() / n;
            let cov = a
                .iter()
                .zip(&b)
                .map(|(x, y)| (x - ma) * (y - mb))
                .sum::<f32>()
                / n;
            let s = ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            ssim += f64::from(s);
            windows += 1;
        }
    }
    Ok(Score {
        mae: (error / count as f64) as f32,
        ssim: if windows == 0 {
            1.0
        } else {
            (ssim / windows as f64) as f32
        },
    })
}

/// Absolute difference as a red heatmap; masked pixels are dark blue.
pub fn heatmap(rendered: &GrayImage, reference: &GrayImage, mask: &[[u32; 4]]) -> RgbImage {
    let (w, h) = rendered.dimensions();
    RgbImage::from_fn(w, h, |x, y| {
        if masked(mask, x, y) || x >= reference.width() || y >= reference.height() {
            return Rgb([0, 0, 40]);
        }
        let a = i16::from(rendered.get_pixel(x, y).0[0]);
        let b = i16::from(reference.get_pixel(x, y).0[0]);
        let d = (a - b).unsigned_abs().min(255) as u8;
        Rgb([d, d / 4, d / 4])
    })
}

fn script(case: &Case, screenshot: &Path) -> String {
    let mut lines = vec![
        "wait".to_owned(),
        format!("truck {}", case.truck),
        "wait".to_owned(),
    ];
    if let Some(drive) = &case.drive {
        lines.push(format!("drive {drive}"));
        lines.push("wait".to_owned());
    }
    for step in &case.steps {
        lines.push(step.clone());
        lines.push("wait".to_owned());
    }
    lines.push(format!("screenshot {}", screenshot.display()));
    lines.push("exit".to_owned());
    lines.join("\n") + "\n"
}

/// Runs the parity check; `args` are the command-line arguments after
/// `--parity`. Returns whether every compared case passed.
pub fn run(args: &[String]) -> Result<bool, String> {
    let (manifest_path, names): (PathBuf, Vec<&String>) = match args.first() {
        Some(first) if first.ends_with(".json") => {
            (PathBuf::from(first), args[1..].iter().collect())
        }
        _ => (PathBuf::from(DEFAULT_CASES), args.iter().collect()),
    };
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let out_dir = PathBuf::from(OUTPUT_DIR);
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut all_passed = true;
    // Release builds have no console; the table also goes to a file.
    let mut report = vec![format!("{:<28} {:>8} {:>8}  result", "case", "MAE", "SSIM")];
    for case in manifest
        .cases
        .iter()
        .filter(|c| names.is_empty() || names.iter().any(|n| **n == c.name))
    {
        let shot = out_dir.join(format!("{}.png", case.name));
        let script_path = out_dir.join(format!("{}.script", case.name));
        std::fs::write(&script_path, script(case, &shot)).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&shot);
        let status = Command::new(&exe)
            .env(crate::script::ENV_VAR, &script_path)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() || !shot.exists() {
            report.push(format!(
                "{:<28} {:>8} {:>8}  render failed ({status})",
                case.name, "-", "-"
            ));
            all_passed = false;
            continue;
        }
        let reference_path = Path::new(REFERENCE_DIR).join(format!("{}.png", case.name));
        if !reference_path.exists() {
            report.push(format!(
                "{:<28} {:>8} {:>8}  no reference ({})",
                case.name,
                "-",
                "-",
                reference_path.display()
            ));
            continue;
        }
        let load = |p: &Path| {
            image::open(p)
                .map(|i| i.to_luma8())
                .map_err(|e| format!("{}: {e}", p.display()))
        };
        let (rendered, reference) = (load(&shot)?, load(&reference_path)?);
        if reference.dimensions() != (manifest.resolution[0], manifest.resolution[1]) {
            report.push(format!(
                "{:<28} {:>8} {:>8}  reference is {:?}, cases expect {:?}",
                case.name,
                "-",
                "-",
                reference.dimensions(),
                manifest.resolution
            ));
            all_passed = false;
            continue;
        }
        match compare(&rendered, &reference, &case.mask) {
            Ok(score) => {
                let pass = score.mae <= case.threshold;
                all_passed &= pass;
                heatmap(&rendered, &reference, &case.mask)
                    .save(out_dir.join(format!("{}-diff.png", case.name)))
                    .map_err(|e| e.to_string())?;
                report.push(format!(
                    "{:<28} {:>8.4} {:>8.4}  {}",
                    case.name,
                    score.mae,
                    score.ssim,
                    if pass { "pass" } else { "FAIL" }
                ));
            }
            Err(error) => {
                report.push(format!("{:<28} {:>8} {:>8}  {error}", case.name, "-", "-"));
                all_passed = false;
            }
        }
    }
    let text = report.join(
        "
",
    ) + "
";
    print!("{text}");
    std::fs::write(out_dir.join("report.txt"), text).map_err(|e| e.to_string())?;
    Ok(all_passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    fn gray(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| Luma([f(x, y)]))
    }

    #[test]
    fn identical_images_score_perfectly() {
        let a = gray(16, 16, |x, y| ((x * 13 + y * 7) % 256) as u8);
        let score = compare(&a, &a, &[]).expect("score");
        assert_eq!(score.mae, 0.0);
        assert!((score.ssim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn masked_differences_are_ignored() {
        let a = gray(16, 16, |_, _| 100);
        let b = gray(16, 16, |x, _| if x < 8 { 255 } else { 100 });
        assert!(compare(&a, &b, &[]).expect("score").mae > 0.2);
        let masked = compare(&a, &b, &[[0, 0, 8, 16]]).expect("score");
        assert_eq!(masked.mae, 0.0);
        assert!(compare(&a, &b, &[[0, 0, 16, 16]]).is_err(), "all masked");
    }

    #[test]
    fn different_sizes_are_reported_not_compared() {
        let a = gray(16, 16, |_, _| 0);
        let b = gray(8, 16, |_, _| 0);
        assert!(compare(&a, &b, &[]).is_err());
    }

    #[test]
    fn cases_become_dev_scripts() {
        let case = Case {
            name: "lightbar".to_owned(),
            truck: "scania.s_2016".to_owned(),
            drive: Some("right".to_owned()),
            steps: vec!["slot r_grill".to_owned(), "pick 0".to_owned()],
            mask: Vec::new(),
            threshold: 0.1,
        };
        let text = script(&case, Path::new("out/lightbar.png"));
        let steps = crate::script::parse(&text).expect("valid script");
        assert_eq!(steps.len(), 11);
        assert!(text.ends_with("screenshot out/lightbar.png\nexit\n"));
    }
}
