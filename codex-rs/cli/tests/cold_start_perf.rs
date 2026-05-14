#![cfg(not(target_os = "windows"))]

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use std::time::Instant;

use codex_utils_cargo_bin::find_resource;

const BASELINE_ENV: &str = "CODEX_CLI_PERF_BASELINE";
const CANDIDATE_ENV: &str = "CODEX_CLI_PERF_CANDIDATE";
const RUNS_ENV: &str = "CODEX_CLI_PERF_RUNS";
const MIN_SPEEDUP_ENV: &str = "CODEX_CLI_PERF_MIN_SPEEDUP";
const DEFAULT_RUNS: usize = 9;
const DEFAULT_MIN_SPEEDUP: f64 = 1.05;

#[test]
fn codex_exec_cold_start_candidate_beats_baseline() -> anyhow::Result<()> {
    let Some(baseline) = env_path(BASELINE_ENV) else {
        eprintln!("skipping cold-start perf e2e: set {BASELINE_ENV} to a baseline codex binary");
        return Ok(());
    };
    let candidate = env_path(CANDIDATE_ENV).unwrap_or(codex_utils_cargo_bin::cargo_bin("codex")?);
    let fixture = find_resource!("../exec/tests/fixtures/cli_responses_fixture.sse")?;
    let runs = env_usize(RUNS_ENV, DEFAULT_RUNS).max(3);
    let min_speedup = env_f64(MIN_SPEEDUP_ENV, DEFAULT_MIN_SPEEDUP);

    let baseline_samples = measure_codex(&baseline, &fixture, runs)?;
    let candidate_samples = measure_codex(&candidate, &fixture, runs)?;
    let baseline_median = median(baseline_samples);
    let candidate_median = median(candidate_samples);
    let speedup = baseline_median.as_secs_f64() / candidate_median.as_secs_f64();

    eprintln!(
        "codex exec cold-start perf: baseline={} candidate={} baseline_median={}ms candidate_median={}ms speedup={speedup:.2}x",
        baseline.display(),
        candidate.display(),
        millis(baseline_median),
        millis(candidate_median),
    );

    assert!(
        speedup >= min_speedup,
        "expected candidate to be at least {min_speedup:.2}x faster than baseline; \
         baseline median={}ms candidate median={}ms speedup={speedup:.2}x",
        millis(baseline_median),
        millis(candidate_median),
    );

    Ok(())
}

fn measure_codex(bin: &Path, fixture: &Path, runs: usize) -> anyhow::Result<Vec<Duration>> {
    let mut samples = Vec::with_capacity(runs);
    for idx in 0..runs {
        samples.push(run_once(bin, fixture, idx)?);
    }
    Ok(samples)
}

fn run_once(bin: &Path, fixture: &Path, idx: usize) -> anyhow::Result<Duration> {
    let home = tempfile::Builder::new()
        .prefix("codex-cli-perf-home")
        .tempdir()?;
    let cwd = tempfile::Builder::new()
        .prefix("codex-cli-perf-cwd")
        .tempdir()?;
    let started = Instant::now();
    let output = Command::new(bin)
        .current_dir(cwd.path())
        .env("CODEX_HOME", home.path())
        .env("CODEX_SQLITE_HOME", home.path())
        .env("CODEX_API_KEY", "dummy")
        .env("CODEX_RS_SSE_FIXTURE", fixture)
        .env("NO_COLOR", "1")
        .args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "-s",
            "read-only",
            "--dangerously-bypass-approvals-and-sandbox",
            "--disable",
            "plugins",
            "--disable",
            "apps",
            "--disable",
            "shell_tool",
            "-c",
            "mcp_servers={}",
            "-c",
            "model_reasoning_effort=\"none\"",
            "-c",
            "web_search=\"disabled\"",
            "-m",
            "gpt-5.4-mini",
            "cold start perf fixture",
        ])
        .output()?;
    let elapsed = started.elapsed();
    if !output.status.success() {
        anyhow::bail!(
            "codex perf run {idx} failed for {} with status {:?}\nstdout:\n{}\nstderr:\n{}",
            bin.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    Ok(elapsed)
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn env_f64(name: &str, fallback: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn millis(duration: Duration) -> u128 {
    duration.as_millis()
}
