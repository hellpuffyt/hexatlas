# Security

hexatlas reads two kinds of untrusted input: binary files and `.hext`
templates (which may come from someone else).

| Threat | Defence | Evidence |
|---|---|---|
| Malformed file crashes the decoder | Every read is bounds-checked (`UnexpectedEof`, `MagicMismatch`, `NegativeCount` errors) | `tests/fuzz.rs`, `eval` unit tests |
| Malformed template crashes the parser | Lexer/parser return positioned errors; references, counts and cycles validated before use | `tests/fuzz.rs`, `parser` unit tests |
| Template causes a hang or memory blow-up | Struct nesting ≤ 256 (linear-time check); evaluation capped at `64 × file size + 1,000,000` fields | `eval::tests::zero_sized_array_elements_cannot_run_away`, `exponential_zero_sized_nesting_is_bounded`, `parser::tests::diamond_templates_validate_in_linear_time`, `struct_nesting_deeper_than_the_limit_is_rejected` |

Non-guarantees: the whole file is read into memory; `inspect` prints a hex
dump of the entire file; `--color` emits ANSI escapes of its own. Control
characters inside decoded `ascii`/`cstr` text are rendered as `\u{..}`
escapes, never written raw (`eval::tests::decoded_text_cannot_carry_control_characters`).

## Reporting

Report privately via
https://github.com/hellpuffyt/hexatlas/security/advisories/new with the
template and file that reproduce the issue.
