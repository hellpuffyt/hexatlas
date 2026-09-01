# hexatlas

Inspect and diff binary files against reusable structure templates — describe
a binary format once, in a small DSL, and reuse it across every file that
claims to be that format, with a live hex + field-annotation view.

```
hexatlas check   <template>                    # parse & validate a template
hexatlas inspect <template> <file>              # annotated hex dump + field table
hexatlas fields  <template> <file> [--format]   # decoded fields, text or JSON
hexatlas diff    <template> <file-a> <file-b>   # field-level structural diff
```

## Why this and not the 1,000 clones

Hex viewers are everywhere, and "binary diff" tools are everywhere too. What
almost none of them do is let you **describe a format once and reuse that
description** — most hex editors annotate a file interactively, by hand,
every single time you open a new file of the same format, and most binary
diff tools (`cmp`, `vbindiff`, `radiff2`) compare two files as undifferentiated
byte streams with no notion of "this range is a `u32` called `width`."

hexatlas separates the two concerns that everyone else bundles together:

- **A structure template is a first-class, reusable artifact.** Write
  `png.hext` once — describing the PNG signature and IHDR chunk as named,
  typed fields — and every command (`inspect`, `fields`, `diff`) applies it
  to as many files as you want. Nobody has to re-annotate the same 33 bytes
  by hand for the tenth PNG they open this week.
- **Diffing is field-aware, not byte-aware.** `hexatlas diff` doesn't tell
  you "byte 16 changed from `0x02` to `0x03`" — it tells you
  `ihdr.width  2 -> 3`, because it decoded both files through the same
  template first. A `cmp -l` or `vbindiff` session on a format with any
  variable-length or reordered fields turns into noise the moment one field
  shifts everything after it; hexatlas compares by *field path*, so an
  inserted array element shows up as one `+` line, not a wall of "everything
  after byte N differs."
- **The template DSL has real semantics, not just byte offsets.** Magic-byte
  validation (`magic "PNG"`), endianness (per-field or template-wide,
  `le`/`be`), nested structs, and arrays sized by an earlier field
  (`items: u16[count]`) are first-class grammar, checked at parse time — a
  count field that doesn't exist, or a struct that references itself, is
  rejected before you ever point it at a file. That is a small, real type
  system for binary layouts, not a list of `(offset, length, label)` tuples.
- **A template only needs to describe as much of the file as you care
  about.** The bundled example decodes exactly the fixed 33-byte PNG header
  and leaves the rest of the file — compressed image data, `IEND` — as
  plain, unmapped bytes in the hex dump. You are not forced to model an
  entire format before you can use any of it.

None of these pieces is exotic on its own. The combination — a small,
validated DSL for binary layout, applied identically by three different
consumers (hex view, field table, diff) — is not what you get from a hex
editor, a `struct`-annotation macro for one language's own types, or a
generic byte-level diff tool.

## The template DSL

A `.hext` template is a sequence of `struct` definitions plus one `root`
declaration naming the struct applied at offset 0. See
[`examples/formats/png.hext`](examples/formats/png.hext) for a complete,
real example (PNG's fixed-shape signature + IHDR chunk):

```hext
endian be;

struct IHDR {
    length: u32;
    chunk_type: magic "IHDR";
    width: u32;
    height: u32;
    bit_depth: u8;
    color_type: u8;
    compression_method: u8;
    filter_method: u8;
    interlace_method: u8;
    crc: bytes[4];
}

struct PngHeader {
    signature: magic 0x89504E47;
    line_ending_check: magic 0x0D0A1A0A;
    ihdr: IHDR;
}

root PngHeader;
```

Field types:

| Syntax                       | Meaning                                                              |
|-------------------------------|-----------------------------------------------------------------------|
| `u8 u16 u32 u64`               | unsigned integers                                                     |
| `i8 i16 i32 i64`               | signed integers (two's complement)                                    |
| `f32 f64`                      | IEEE-754 floats                                                       |
| `u16le` / `u16be` (any width)  | per-field endianness override                                         |
| `magic "text"` / `magic 0xHEX` | a literal byte sequence that must match exactly, or decoding fails    |
| `ascii[N]`                     | fixed-width text, trailing NUL padding trimmed                        |
| `cstr[N]`                      | fixed-size buffer holding a NUL-terminated string                     |
| `bytes[N]`                     | an opaque N-byte blob, rendered as hex                                |
| `SomeStruct`                   | a nested struct, by name                                              |
| `Type[N]`                      | a fixed-length array                                                  |
| `Type[some_earlier_field]`     | an array whose length is read from an earlier sibling integer field   |

`endian le;` / `endian be;` at the top of the file sets the template-wide
default (little-endian if omitted); any scalar field can override it.

## Usage — real, observed output

The commands below were run against
[`examples/formats/png.hext`](examples/formats/png.hext) and two tiny, real
PNGs generated for this repo
([`examples/fixtures/a.png`](examples/fixtures/a.png) is 2x2,
[`b.png`](examples/fixtures/b.png) is 3x2). Every command's output shown
here is checked byte-for-byte in CI against
[`tests/fixtures/`](tests/fixtures) — see the `dogfood` job in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

### `hexatlas check`

```
$ hexatlas check examples/formats/png.hext
template OK: examples/formats/png.hext
default endian: be
root struct: PngHeader
structs:
  IHDR (10 field(s))
    length: u32
    chunk_type: magic[4]
    width: u32
    height: u32
    bit_depth: u8
    color_type: u8
    compression_method: u8
    filter_method: u8
    interlace_method: u8
    crc: bytes[4]
  PngHeader (3 field(s))
    signature: magic[4]
    line_ending_check: magic[4]
    ihdr: IHDR
```

### `hexatlas inspect` (annotated hex dump + field table)

```
$ hexatlas inspect examples/formats/png.hext examples/fixtures/a.png
00000000  89 50 4E 47 0D 0A 1A 0A  00 00 00 0D 49 48 44 52  |.PNG........IHDR|
00000010  00 00 00 02 00 00 00 02  08 02 00 00 00 FD D4 9A  |................|
00000020  73 00 00 00 10 49 44 41  54 78 DA 63 38 A1 A1 01  |s....IDATx.c8...|
00000030  44 0C 10 0A 00 21 2E 04  61 F6 E1 C2 4D 00 00 00  |D....!..a...M...|
00000040  00 49 45 4E 44 AE 42 60  82                       |.IEND.B`.|

OFFSET    LEN  PATH                           TYPE                 VALUE
00000000    4   signature                      magic[4]             89 50 4E 47
00000004    4   line_ending_check              magic[4]             0D 0A 1A 0A
00000008   25  +ihdr                           IHDR                 { 10 field(s) }
00000008    4   ihdr.length                    u32                  13
0000000C    4   ihdr.chunk_type                magic[4]             49 48 44 52
00000010    4   ihdr.width                     u32                  2
00000014    4   ihdr.height                    u32                  2
00000018    1   ihdr.bit_depth                 u8                   8
00000019    1   ihdr.color_type                u8                   2
0000001A    1   ihdr.compression_method        u8                   0
0000001B    1   ihdr.filter_method             u8                   0
0000001C    1   ihdr.interlace_method          u8                   0
0000001D    4   ihdr.crc                       bytes[4]             FD D4 9A 73
```

Notice the header (offsets `0x00`-`0x20`) is fully annotated, while the
`IDAT`/`IEND` chunks past it are just plain hex — the template only claims to
describe what it describes. Pass `--color` in a real terminal to see each
top-level field highlighted in its own ANSI color across the hex and ASCII
columns; `--no-hex` skips the dump and prints only the field table.

### `hexatlas diff` (field-aware, not byte-aware)

```
$ hexatlas diff examples/formats/png.hext examples/fixtures/a.png examples/fixtures/b.png
~ ihdr.width                   u32        2 -> 3
~ ihdr.crc                     bytes[4]   FD D4 9A 73 -> 12 16 F1 4D

2 changed, 0 added, 0 removed
```

Only `width` and `crc` (which covers `width` in its checksum) changed
between the 2x2 and 3x2 fixtures — everything else in the header, including
every other IHDR byte, is identical, and the diff says so directly instead of
making you eyeball two hex dumps for a one-byte change. Diffing files whose
templates produce differently-sized arrays reports `+`/`-` per array element
by path (e.g. `items[2]`), not a single "sizes differ" bail-out.

### `hexatlas fields --format json`

```
$ hexatlas fields examples/formats/png.hext examples/fixtures/a.png --format json
[
  {"path": "signature", "offset": 0, "len": 4, "kind": "leaf", "type": "magic[4]", "value": "89 50 4E 47", "raw_hex": "89 50 4E 47"},
  {"path": "line_ending_check", "offset": 4, "len": 4, "kind": "leaf", "type": "magic[4]", "value": "0D 0A 1A 0A", "raw_hex": "0D 0A 1A 0A"},
  {"path": "ihdr", "offset": 8, "len": 25, "kind": "group", "type": "IHDR", "value": "{ 10 field(s) }", "raw_hex": ""},
  {"path": "ihdr.length", "offset": 8, "len": 4, "kind": "leaf", "type": "u32", "value": "13", "raw_hex": "00 00 00 0D"},
  {"path": "ihdr.chunk_type", "offset": 12, "len": 4, "kind": "leaf", "type": "magic[4]", "value": "49 48 44 52", "raw_hex": "49 48 44 52"},
  {"path": "ihdr.width", "offset": 16, "len": 4, "kind": "leaf", "type": "u32", "value": "2", "raw_hex": "00 00 00 02"},
  ...
]
```

`--format text` (the default) prints the same field table `inspect` prints
below its hex dump.

### A template applied to the wrong file fails loudly

```
$ hexatlas inspect examples/formats/png.hext Cargo.toml
error: Cargo.toml (examples/formats/png.hext): magic mismatch at field `signature` (offset 0): expected [89 50 4E 47], found [5B 70 61 63]
```

Exit code 1. Unrecognized-format detection is not an afterthought — every
`magic` field is checked before any later field is even read, so you get a
precise failure at the first byte that disagrees with the template, not a
garbage decode of the rest of the file.

## Installation

```
git clone https://github.com/hellpuffyt/hexatlas
cd hexatlas
cargo install --path .
```

Requires only Rust's standard toolchain — the sole runtime dependency is
[`clap`](https://crates.io/crates/clap) for argument parsing; the DSL lexer,
parser, evaluator, hex renderer, diff engine, and JSON encoder are all
hand-written with no other crates.

## Development

```
cargo test --all-targets   # 54 unit tests + 8 CLI integration tests
cargo test --doc           # the doctest in src/lib.rs
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for more.

## License

MIT — see [LICENSE](LICENSE).
