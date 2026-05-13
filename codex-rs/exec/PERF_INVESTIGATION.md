# Codex Exec Cold-Start Performance Investigation

This branch is an investigation for OpenCodeCommit-style use of Codex CLI:
short prompt in, final text out, fresh `codex exec` process, usually with an
isolated `CODEX_HOME`, no apps, no plugins, no MCP servers, and `--ephemeral`.

Branch:

```text
investigate/exec-ephemeral-state-skip
```

## Current Patch

`codex exec --ephemeral` no longer initializes the SQLite state runtime in
`exec::run_main`.

Before this patch, `codex exec --ephemeral` did not persist rollout JSONL files,
but still initialized `state_5.sqlite` and `logs_2.sqlite` before starting the
in-process app-server. Session initialization later treated ephemeral sessions
as stateless, so this early DB initialization was mostly wasted for one-shot
exec usage.

Changed files:

```text
codex-rs/exec/src/lib.rs
codex-rs/exec/tests/suite/ephemeral.rs
```

The integration test now asserts that ephemeral exec creates neither rollout
files nor SQLite state/log DB files.

## Verification Already Run

```sh
cd /home/antti/code/public/codex/codex-rs
cargo build -p codex-cli
cargo test -p codex-exec
cargo test -p codex-exec --test all suite::ephemeral::does_not_persist_rollout_file_in_ephemeral_mode
```

## Laptop Measurements

These were run against a local fake Responses API server, so model latency was
not part of the measurement. Treat numbers as directional; the laptop was not a
stable benchmarking host.

Installed release CLI:

```text
native_minimal       n=8 avg_elapsed=0.890s min=0.560s max=1.390s avg_rss=74024KB
wrapper_minimal      n=8 avg_elapsed=0.961s min=0.650s max=1.450s avg_rss=74054KB
native_real_home     n=8 avg_elapsed=2.749s min=2.220s max=3.220s avg_rss=211410KB
native_ignore_config n=8 avg_elapsed=0.899s min=0.340s max=2.580s avg_rss=73659KB
schema               no meaningful overhead in this run
```

Debug source binary, skills disabled to isolate state DB side effects:

```text
baseline n=6 avg_elapsed=0.152s min=0.130s max=0.230s avg_rss=170831KB
patched  n=6 avg_elapsed=0.123s min=0.110s max=0.150s avg_rss=168223KB
```

Filesystem side effects from the debug run:

```text
baseline CODEX_HOME: 1.2M, created state_5.sqlite and logs_2.sqlite
patched  CODEX_HOME: 8.0K, no SQLite state/log files
```

Bundled skills observation:

```text
--disable plugins --disable apps does not disable bundled system skills.
Adding:
  -c 'skills.bundled.enabled=false'
  -c 'skills.include_instructions=false'
prevented writing skills/.system in a fresh CODEX_HOME.
```

## Desktop Test Plan

Install profiling/benchmarking tools:

```sh
hyperfine --version
strace -V
perf --version
heaptrack --version
```

If missing, install:

```sh
sudo dnf install hyperfine strace perf heaptrack
```

Build release binaries:

```sh
cd /home/antti/code/public/codex/codex-rs
cargo build --release -p codex-cli
cargo test -p codex-exec
```

Compare:

```text
official npm native binary
official npm JS wrapper
local release main
local release investigate/exec-ephemeral-state-skip
```

## Local Mock Backend

Use a local fake Responses API server so network/model latency does not dominate
startup measurements. It should respond to `/v1/responses` with a valid SSE
stream containing one assistant message, for example `ok`.

Benchmark `CODEX_HOME/config.toml`:

```toml
model_provider = "bench"

[model_providers.bench]
name = "bench"
base_url = "http://127.0.0.1:PORT/v1"
env_key = "CODEX_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false
```

Base command:

```sh
printf 'hello\n' | env CODEX_HOME="$HOME_DIR" CODEX_API_KEY=dummy \
  /usr/bin/time -f '%e %M' "$CODEX_BIN" exec \
  --ephemeral \
  --skip-git-repo-check \
  --disable plugins \
  --disable apps \
  --json \
  -s read-only \
  --dangerously-bypass-approvals-and-sandbox \
  -c 'mcp_servers={}' \
  -c 'model_reasoning_effort="none"' \
  -c 'web_search="disabled"' \
  -m gpt-5.4-mini -
```

OpenCodeCommit fast-path variant:

```sh
-c 'skills.bundled.enabled=false'
-c 'skills.include_instructions=false'
```

## Benchmark Matrix

Run each with:

```sh
hyperfine --warmup 5 --runs 50 '<command>'
```

Cases:

```text
1. native binary + minimal isolated CODEX_HOME
2. npm wrapper + minimal isolated CODEX_HOME
3. local release main + minimal isolated CODEX_HOME
4. local release patched branch + minimal isolated CODEX_HOME
5. local release patched branch + OpenCodeCommit-style tiny CODEX_HOME
6. same cases with real ~/.codex
7. same cases from real git repo cwd
8. same cases from empty temp cwd
9. same cases with --output-schema
10. same cases with bundled skills disabled
```

Important separate cases:

```text
first run in a fresh CODEX_HOME
second run in the same CODEX_HOME
large historical ~/.codex with existing sessions/state DB
```

## Profiling Pass

For the slowest cases:

```sh
strace -f -c -o strace.txt <command>
perf stat -r 20 <command>
heaptrack <command>
```

Record filesystem side effects:

```sh
find "$CODEX_HOME" -maxdepth 4 -type f -printf '%P %s\n' | sort
du -sh "$CODEX_HOME"
```

Record DB files specifically:

```sh
find "$CODEX_HOME" -maxdepth 1 \
  \( -name 'state_*.sqlite*' -o -name 'logs_*.sqlite*' \) \
  -printf '%P %s\n' | sort
```

## Follow-Up Experiments

1. Keep this branch's ephemeral state/log DB skip if desktop numbers confirm it
   removes real-home variance or persistent writes.

2. Measure:

   ```sh
   -c 'skills.bundled.enabled=false'
   -c 'skills.include_instructions=false'
   ```

   This may be an OpenCodeCommit-only improvement even if it is not an upstream
   default.

3. Temporarily skip `sess.schedule_startup_prewarm(...)` for `SessionSource::Exec`
   and measure. Exec immediately starts the first turn, so websocket prewarm may
   be redundant or harmful for cold starts.

4. Make in-process exec pass `PluginStartupTasks::Skip` when plugins/apps are
   disabled. Measure with and without plugin directories present in `CODEX_HOME`.

5. Add an empty-MCP fast path when apps are disabled and `mcp_servers={}`. This
   would avoid building auth status and MCP startup machinery for no configured
   servers.

6. Test websocket fallback. A custom provider with `supports_websockets=false`
   avoided a multi-second fallback delay against a mock HTTP-only endpoint.

7. Longer-term: investigate a true prompt-only exec path that bypasses the full
   in-process app-server/session stack for the common non-interactive case.

## PR Decision Criteria

Good upstream PR candidate if it does one or more of:

```text
saves at least 50ms median cold start
saves at least 10MB peak RSS
removes persistent writes in --ephemeral mode
removes large variance from real ~/.codex
has narrow behavior-preserving tests
```

Current likely PR ranking:

```text
1. Ephemeral state/log DB skip.
2. OpenCodeCommit-side bundled skills disable flags.
3. Skip websocket prewarm for one-shot exec when it is not useful.
4. Skip plugin startup tasks when plugins/apps are disabled.
5. True headless prompt-only exec path.
```
