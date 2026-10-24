# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- A one-line template with an array of zero-sized structs whose count is
  read from the file (e.g. `struct E { } struct A { n: u32le; z: E[n]; }`)
  made `hexatlas` loop and allocate once per count, hanging on a 4-byte
  file; evaluation is now bounded (`EvalError::TooManyFields`).
- Template validation re-walked shared sub-structs without memoisation, so
  a 40-struct "diamond" template never finished `check`; it is now linear.
- Arbitrarily deep struct chains were accepted and then overflowed the
  stack during evaluation; nesting is limited to 256 levels.

- `ascii`/`cstr` text decoded from a file was printed verbatim, so a crafted
  file could inject terminal escape sequences; control characters are now
  rendered as `\u{..}` escapes.

### Added

- Deterministic fuzz tests (`tests/fuzz.rs`), `SECURITY.md`, `.editorconfig`.

## [0.1.0] - 2026-09-01

### Added

- The `.hext` template DSL: `struct` definitions, a `root` declaration,
  template-wide and per-field `le`/`be` endianness, unsigned/signed integers
  (`u8`..`u64`, `i8`..`i64`), `f32`/`f64` floats, `magic` literals (string or
  hex), fixed-width `ascii[N]`, NUL-terminated `cstr[N]`, opaque `bytes[N]`,
  nested struct references, fixed-length arrays, and arrays sized by an
  earlier sibling integer field (`Type[count_field]`).
- A hand-written lexer and recursive-descent parser with source line/column
  error reporting, plus a validation pass that rejects undefined struct
  references, out-of-scope or non-integer array count fields, and cyclic
  (including mutually recursive) struct definitions before any file is ever
  read.
- The evaluator (`hexatlas::eval::apply`), which walks a template against a
  byte buffer and produces an ordered list of decoded fields (leaves and
  struct/array groups), each with its dotted/bracketed path, offset,
  length, raw bytes, and rendered value. Magic mismatches and truncated
  files are reported as precise, field-scoped errors.
- `hexatlas check <template>` — parse and validate a template, printing its
  struct/field summary.
- `hexatlas inspect <template> <file>` — an annotated 16-bytes-per-row hex +
  ASCII dump (optionally `--color`, per top-level field) followed by a flat
  field table; `--no-hex` prints just the table.
- `hexatlas fields <template> <file> [--format text|json]` — the decoded
  field list on its own, as the field table or as dependency-free JSON.
- `hexatlas diff <template> <file-a> <file-b>` — a field-path-aware diff
  between two files decoded with the same template, reporting changed,
  added, and removed fields (including individual array elements) instead
  of raw byte ranges.
- A bundled real-world example: `examples/formats/png.hext` (PNG's
  fixed-shape signature + IHDR chunk) plus two tiny real PNG fixtures
  (`examples/fixtures/{a,b}.png`) generated for this repo, exercised by both
  the integration test suite and the CI dogfood job.
- 54 unit tests across the lexer, parser/validator, evaluator, hex-dump
  renderer, diff engine, and JSON encoder, plus 8 CLI integration tests
  that run the compiled binary end to end against the bundled example.

[0.1.0]: https://github.com/hellpuffyt/hexatlas/releases/tag/v0.1.0
