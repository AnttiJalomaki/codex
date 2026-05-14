#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use codex_utils_cargo_bin::find_resource;
use core_test_support::test_codex_exec::test_codex_exec;
use walkdir::WalkDir;

fn session_rollout_count(home_path: &std::path::Path) -> usize {
    let sessions_dir = home_path.join("sessions");
    if !sessions_dir.exists() {
        return 0;
    }

    WalkDir::new(sessions_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".jsonl"))
        .count()
}

#[test]
fn persists_rollout_file_by_default() -> anyhow::Result<()> {
    let test = test_codex_exec();
    let fixture = find_resource!("tests/fixtures/cli_responses_fixture.sse")?;

    test.cmd()
        .env("CODEX_RS_SSE_FIXTURE", &fixture)
        .arg("--skip-git-repo-check")
        .arg("default persistence behavior")
        .assert()
        .code(0);

    assert_eq!(session_rollout_count(test.home_path()), 1);
    Ok(())
}

#[test]
fn does_not_persist_rollout_file_in_ephemeral_mode() -> anyhow::Result<()> {
    let test = test_codex_exec();
    let fixture = find_resource!("tests/fixtures/cli_responses_fixture.sse")?;

    test.cmd()
        .env("CODEX_RS_SSE_FIXTURE", &fixture)
        .arg("--skip-git-repo-check")
        .arg("--ephemeral")
        .arg("--dangerously-bypass-approvals-and-sandbox")
        .arg("ephemeral behavior")
        .assert()
        .code(0);

    assert_eq!(session_rollout_count(test.home_path()), 0);
    assert!(!test.home_path().join("config.toml").exists());
    assert!(!test.home_path().join("installation_id").exists());
    assert!(!test.home_path().join("state_5.sqlite").exists());
    assert!(!test.home_path().join("logs_2.sqlite").exists());
    Ok(())
}
