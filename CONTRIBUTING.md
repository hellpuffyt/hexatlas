# Contributing to hexatlas

## Setup

```
git clone https://github.com/hellpuffyt/hexatlas
cd hexatlas
cargo build
```

Only the stable Rust toolchain is required. No network access, database, or
external service is needed to build, test, or run this project.

## Running the checks locally

```
cargo test --all-targets   # unit tests + the tests/cli.rs integration suite
cargo test --doc           # the doctest in src/lib.rs
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

CI (`.github/workflows/ci.yml`) runs all of the above on Linux, macOS, and
Windows, plus a `dogfood` job that builds the release binary and diffs its
output against `examples/formats/png.hext` and `examples/fixtures/*.png`
byte-for-byte against `tests/fixtures/*`. If you change anything that
affects rendered output (the hex dump layout, the field table columns, the
diff format, the JSON shape, or an error message), regenerate those fixtures
and update the matching quoted output in `README.md` — CI will catch a
mismatch, but it's much faster to catch it yourself first:

```
cargo build --release
./target/release/hexatlas check   examples/formats/png.hext                                   > tests/fixtures/check-output.txt
./target/release/hexatlas inspect examples/formats/png.hext examples/fixtures/a.png            > tests/fixtures/inspect-a-output.txt
./target/release/hexatlas diff    examples/formats/png.hext examples/fixtures/a.png examples/fixtures/b.png > tests/fixtures/diff-a-b-output.txt
./target/release/hexatlas fields  examples/formats/png.hext examples/fixtures/a.png --format json > tests/fixtures/fields-a-output.json
```

## Ground rules

- **No new runtime dependencies without a real reason.** The only one today
  is `clap`, for argument parsing. The DSL lexer, parser, evaluator, hex
  renderer, diff engine, and JSON encoder are hand-written on purpose — this
  project exists partly to demonstrate that a small binary-format DSL
  doesn't need a parser-combinator crate or a serialization framework. If
  you genuinely need a dependency, explain in the PR why hand-rolling it
  isn't reasonable.
- **Every new template DSL feature needs: a lexer/parser test, a validation
  test if it can be misused (undefined reference, bad count field, etc.), an
  evaluator test that decodes real bytes, and — if it changes anything a
  command prints — an update to the bundled `examples/formats/png.hext`
  demonstration or a new example plus the matching fixture/README update
  described above.** A feature that only has a "does it parse" test but was
  never actually applied to bytes is not tested.
- **Errors carry a field path and a byte offset.** `EvalError` variants
  exist so a user can find the exact field and byte that failed, not just
  "decoding failed." A new error path that just says "invalid input" will be
  asked to include a path and offset like the existing ones.
- **Determinism matters more than cleverness.** Field iteration order,
  rendered numeric formatting, and JSON key order are all fixed and tested
  because the CI dogfood job diffs real output byte-for-byte. Don't
  introduce `HashMap` iteration (or anything else non-deterministic) into
  any code path that produces user-visible output.
- **`clippy::all -D warnings` and `rustfmt` are non-negotiable** — CI fails
  the build otherwise, matching the rest of this project's style: run both
  before opening a PR.
