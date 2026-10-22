# Production readiness — hexatlas

Audit date: 2026-10-22. Tested commit: `2fceae3` on branch
`audit/production-readiness` (this file was added in the next commit; no
code changed after the tested commit).

## Scope

hexatlas applies reusable `.hext` binary-structure templates (small DSL:
structs, ints/floats with endianness, magic, ascii/cstr/bytes, nested
structs, fixed and field-sized arrays) to files: `check`, `inspect`
(annotated hex + field table), `fields` (text/JSON), `diff` (field-aware).
Single runtime dependency: `clap`. Maturity: 0.1.0.

## Platforms and prerequisites

Rust stable; pure Rust, no OS-specific code. Verified on macOS 27 (Apple
silicon, rustc 1.98.0); default-branch CI covers ubuntu, windows, macos.

## Clean-clone verification (this audit)

```
git clone -b audit/production-readiness <repo> cc && cd cc
cargo install --path . --root <scratch>        # exit 0 (README: plain `cargo install --path .`)
hexatlas check   examples/formats/png.hext                          # == tests/fixtures/check-output.txt
hexatlas inspect examples/formats/png.hext examples/fixtures/a.png  # == inspect-a-output.txt
hexatlas diff    examples/formats/png.hext examples/fixtures/a.png examples/fixtures/b.png  # == diff-a-b-output.txt
hexatlas fields  examples/formats/png.hext examples/fixtures/a.png --format json            # == fields-a-output.json
hexatlas inspect examples/formats/png.hext Cargo.toml               # exit 1, magic mismatch
cargo test --all-targets                       # exit 0: 71 passed, 0 failed, 0 ignored
cargo test --doc                               # exit 0: 1 passed
cargo fmt --all -- --check; cargo clippy --all-targets -- -D warnings   # 0 / 0
```

## Commands executed (exit codes)

| Command | Before (main `d40b8c7`) | After (`2fceae3`) |
|---|---|---|
| `cargo test --all-targets` | 0: 62 passed (54 unit + 8 CLI) | 0: 71 passed (60 unit + 8 CLI + 3 fuzz), 0 ignored |
| `cargo test --doc` | 0: 1 passed | 0: 1 passed |
| `cargo fmt --all -- --check` | 0 | 0 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 0 |
| `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --locked` | — | 0 |
| CI dogfood diffs (4 outputs + mismatch exit 1) | pass | pass |
| `cargo audit --db <advisory-db 2026-10-22> --no-fetch` | 0 (22 crates) | 0 (22 crates) |
| `gitleaks git` / `gitleaks dir` | — | no leaks found |

## CI status

Default branch (`main`, `d40b8c7`): **success** (run 33525122103).
Audit branch: pending until pushed.

## Bugs found and fixed

| # | Severity | Defect (reproduced on the baseline release binary) | Fix | Regression test |
|---|---|---|---|---|
| 1 | Medium (DoS) | `struct E { } struct A { n: u32le; z: E[n]; } root A;` + a 4-byte file: `fields` looped/allocated once per count — killed after 15 s | evaluation capped at `64 × len + 1,000,000` fields (`EvalError::TooManyFields`) | `eval::tests::zero_sized_array_elements_cannot_run_away`, `exponential_zero_sized_nesting_is_bounded` |
| 2 | Medium (DoS) | 40-struct "diamond" template: `check` never finished (exponential cycle check) — killed after 10 s | memoised height computation, linear time | `parser::tests::diamond_templates_validate_in_linear_time` |
| 3 | Medium (DoS/crash) | 100,000-struct chain: `check` ran > 20 s (quadratic) and evaluation would recurse 100k deep | nesting limit 256 levels | `parser::tests::struct_nesting_deeper_than_the_limit_is_rejected`, `eval::tests::nesting_at_the_depth_limit_still_evaluates` |
| 4 | Low (terminal injection) | `ascii`/`cstr` text from files printed raw (escape sequences, newlines) | control chars rendered as `\u{..}` | `eval::tests::decoded_text_cannot_carry_control_characters` |

## Security review

Untrusted inputs: files and templates. Bounds-checked reads, validated
templates, and now bounded evaluation; deterministic fuzz suite (20,000
token-soup templates, 8,000 random files through valid templates, 5,000
random-byte templates) — no panics. No `unsafe`. Workflow already
`permissions: contents: read`. History: 2 pre-existing commits, no secret
hits. No rotation needed. Note: commit author and `Cargo.toml` `authors`
contain a personal name ("Prabesh Sharma") — pre-existing, left unchanged.

## Known limitations

- Whole file read into memory; `inspect` dumps the entire file.
- Validation errors report `line 0, col 0` (pre-existing; no position info).
- License MIT (kept).

## Acceptance checklist

| Item | Expected | Command / test | Result | Actual | Reference |
|---|---|---|---|---|---|
| Template check output | matches README | dogfood diff | pass | identical | `src/main.rs` |
| Inspect PNG header | matches README | dogfood diff | pass | identical | `src/hexview.rs` |
| Field-aware diff | width/crc only | dogfood diff | pass | identical | `src/diff.rs` |
| JSON fields | matches README | dogfood diff | pass | identical | `src/json.rs` |
| Wrong file fails | exit 1, magic mismatch | CLI test | pass | ok | `src/eval.rs` |
| Bad templates rejected | positioned errors, cycles | parser tests | pass | ok | `src/parser.rs` |
| Hostile templates/files bounded | error, no hang/panic | new unit + fuzz tests | pass | ok | `src/eval.rs`, `src/parser.rs` |
| Dependency/secret audit | clean | cargo audit, gitleaks | pass | clean | — |

## Classification proposal

**Partially Verified** — all documented features pass from a clean clone on
macOS with regression tests for the DoS fixes; Linux/Windows rely on CI
(green on `main`, pending for this branch).
