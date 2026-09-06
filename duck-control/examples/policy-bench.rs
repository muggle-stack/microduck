//! Validate and time ONNX policies through the same loader and inference API as robotd.
//! No motor bus is opened. Input is a fixed, upright robot at its home pose; this is
//! a runtime/latency check, not a simulation or a gait-quality test.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Instant;

use duck_control::policy::{DEFAULT_STANDING_THRESHOLD, Net, Policy, PolicyPaths};
use duck_control::{ACTION_LEN, Command, DEFAULT_POSITION, ImuData, NUM_JOINTS, Observation};

fn models(path: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    if path.is_file() {
        return Ok(vec![path.to_owned()]);
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "onnx") {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no ONNX policies in {}", path.display()).into());
    }
    Ok(paths)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let Some(path) = args.next() else {
        return Err("usage: policy-bench MODEL_OR_DIRECTORY [ITERATIONS]".into());
    };
    if path == "--help" || path == "-h" {
        println!("usage: policy-bench MODEL_OR_DIRECTORY [ITERATIONS]");
        return Ok(());
    }
    let iterations = match args.next() {
        Some(raw) => raw
            .to_str()
            .ok_or("ITERATIONS must be UTF-8")?
            .parse::<usize>()?,
        None => 200,
    };
    if !(1..=1_000_000).contains(&iterations) || args.next().is_some() {
        return Err("expected 1..=1000000 iterations and no extra arguments".into());
    }
    let observation = Observation::build(
        &ImuData::default(),
        &DEFAULT_POSITION,
        &[0.0; NUM_JOINTS],
        &DEFAULT_POSITION,
        &[0.0; ACTION_LEN],
        &Command::default(),
    );

    eprintln!("CPU provider, SDK thread settings, upright home-pose input, 20 warmups");
    println!("model,iterations,mean_ms,p50_ms,p95_ms,p99_ms,max_ms");
    for path in models(Path::new(&path))? {
        let mut policy = Policy::load(
            &PolicyPaths {
                walk: path.clone(),
                ..PolicyPaths::default()
            },
            DEFAULT_STANDING_THRESHOLD,
        )?;
        let mut samples = Vec::with_capacity(iterations);
        for index in 0..(20 + iterations) {
            let start = Instant::now();
            let actions = policy.infer(&observation, Net::Walk)?;
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if !actions.iter().all(|value| value.is_finite()) {
                return Err(format!("{} returned non-finite actions", path.display()).into());
            }
            std::hint::black_box(actions);
            if index >= 20 {
                samples.push(elapsed);
            }
        }
        let mean = samples.iter().sum::<f64>() / iterations as f64;
        samples.sort_by(f64::total_cmp);
        // Nearest-rank percentiles, including when only one iteration was requested.
        let percentile = |p: usize| samples[(iterations * p).div_ceil(100) - 1];
        println!(
            "{},{iterations},{mean:.4},{:.4},{:.4},{:.4},{:.4}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            percentile(50),
            percentile(95),
            percentile(99),
            samples[iterations - 1],
        );
    }
    Ok(())
}
