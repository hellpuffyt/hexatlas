//! Applies a parsed [`TemplateFile`] to a byte slice, producing an ordered
//! list of decoded [`FieldValue`]s.

use crate::ast::{CountSpec, Endianness, FieldType, StructDef, TemplateFile};
use std::collections::HashMap;
use std::fmt;

/// Whether a decoded value is a scalar leaf or an aggregate (struct/array)
/// spanning its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Leaf,
    Group,
}

/// One decoded field, either a scalar leaf or a struct/array group.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldValue {
    /// Dotted/bracketed path, e.g. `header.items[2].r`.
    pub path: String,
    pub offset: usize,
    pub len: usize,
    pub type_name: String,
    pub kind: FieldKind,
    pub raw: Vec<u8>,
    pub rendered: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    UnexpectedEof {
        path: String,
        offset: usize,
        needed: usize,
        available: usize,
    },
    MagicMismatch {
        path: String,
        offset: usize,
        expected: Vec<u8>,
        actual: Vec<u8>,
    },
    NegativeCount {
        path: String,
        count_field: String,
        value: i128,
    },
    /// The template would produce more decoded fields than [`field_limit`]
    /// allows for this input (e.g. an array of zero-sized structs with a
    /// count read from the file, or an exponentially nested template).
    TooManyFields { path: String, limit: usize },
}

/// Upper bound on decoded fields for an input of `len` bytes. Every leaf
/// with a non-zero size consumes input, so real templates stay far below
/// this; only zero-sized elements or exponential nesting can reach it.
pub fn field_limit(len: usize) -> usize {
    len.saturating_mul(64).saturating_add(1_000_000)
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UnexpectedEof {
                path,
                offset,
                needed,
                available,
            } => write!(
                f,
                "unexpected end of file at field `{path}` (offset {offset}): needed {needed} byte(s), only {available} available"
            ),
            EvalError::MagicMismatch {
                path,
                offset,
                expected,
                actual,
            } => write!(
                f,
                "magic mismatch at field `{path}` (offset {offset}): expected [{}], found [{}]",
                hex_string(expected),
                hex_string(actual)
            ),
            EvalError::NegativeCount {
                path,
                count_field,
                value,
            } => write!(
                f,
                "array `{path}` count field `{count_field}` decoded to a negative value ({value})"
            ),
            EvalError::TooManyFields { path, limit } => write!(
                f,
                "template produces more than {limit} fields for this file (at `{path}`); \
                 zero-sized array elements or exponential nesting?"
            ),
        }
    }
}

impl std::error::Error for EvalError {}

/// Applies `template`'s root struct to `bytes`, returning every decoded
/// field in document order (each group immediately followed by its
/// children).
pub fn apply(template: &TemplateFile, bytes: &[u8]) -> Result<Vec<FieldValue>, EvalError> {
    let root_def = &template.structs[&template.root];
    let mut out = Vec::new();
    let mut cursor = 0usize;
    eval_struct(template, root_def, bytes, &mut cursor, "", &mut out)?;
    Ok(out)
}

fn eval_struct(
    template: &TemplateFile,
    def: &StructDef,
    bytes: &[u8],
    cursor: &mut usize,
    prefix: &str,
    out: &mut Vec<FieldValue>,
) -> Result<(), EvalError> {
    let mut ints: HashMap<String, i128> = HashMap::new();
    for field in &def.fields {
        let path = if prefix.is_empty() {
            field.name.clone()
        } else {
            format!("{prefix}.{}", field.name)
        };
        let value = eval_field(
            template,
            &field.ty,
            bytes,
            cursor,
            &path,
            out,
            &ints,
            template.default_endian,
        )?;
        if let Some(v) = value {
            ints.insert(field.name.clone(), v);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn eval_field(
    template: &TemplateFile,
    ty: &FieldType,
    bytes: &[u8],
    cursor: &mut usize,
    path: &str,
    out: &mut Vec<FieldValue>,
    ints: &HashMap<String, i128>,
    default_endian: Endianness,
) -> Result<Option<i128>, EvalError> {
    // Every branch below pushes at least one field, so this bounds the work
    // and memory of any template, including loops over zero-sized elements.
    let limit = field_limit(bytes.len());
    if out.len() >= limit {
        return Err(EvalError::TooManyFields {
            path: path.to_string(),
            limit,
        });
    }
    match ty {
        FieldType::Int {
            bits,
            signed,
            endian,
        } => {
            let n = (*bits / 8) as usize;
            let raw = take_bytes(bytes, *cursor, n, path)?;
            let endian = endian.unwrap_or(default_endian);
            let value = decode_int(raw, *bits, *signed, endian);
            out.push(FieldValue {
                path: path.to_string(),
                offset: *cursor,
                len: n,
                type_name: ty.type_name(),
                kind: FieldKind::Leaf,
                raw: raw.to_vec(),
                rendered: value.to_string(),
            });
            *cursor += n;
            Ok(Some(value))
        }
        FieldType::Float { bits, endian } => {
            let n = (*bits / 8) as usize;
            let raw = take_bytes(bytes, *cursor, n, path)?;
            let endian = endian.unwrap_or(default_endian);
            let rendered = render_float(raw, *bits, endian);
            out.push(FieldValue {
                path: path.to_string(),
                offset: *cursor,
                len: n,
                type_name: ty.type_name(),
                kind: FieldKind::Leaf,
                raw: raw.to_vec(),
                rendered,
            });
            *cursor += n;
            Ok(None)
        }
        FieldType::Magic(lit) => {
            let n = lit.bytes.len();
            let raw = take_bytes(bytes, *cursor, n, path)?;
            if raw != lit.bytes.as_slice() {
                return Err(EvalError::MagicMismatch {
                    path: path.to_string(),
                    offset: *cursor,
                    expected: lit.bytes.clone(),
                    actual: raw.to_vec(),
                });
            }
            out.push(FieldValue {
                path: path.to_string(),
                offset: *cursor,
                len: n,
                type_name: ty.type_name(),
                kind: FieldKind::Leaf,
                raw: raw.to_vec(),
                rendered: hex_string(raw),
            });
            *cursor += n;
            Ok(None)
        }
        FieldType::Ascii(n) => {
            let raw = take_bytes(bytes, *cursor, *n, path)?;
            let mut end = raw.len();
            while end > 0 && raw[end - 1] == 0 {
                end -= 1;
            }
            let rendered = printable(&String::from_utf8_lossy(&raw[..end]));
            out.push(FieldValue {
                path: path.to_string(),
                offset: *cursor,
                len: *n,
                type_name: ty.type_name(),
                kind: FieldKind::Leaf,
                raw: raw.to_vec(),
                rendered,
            });
            *cursor += n;
            Ok(None)
        }
        FieldType::CStr(n) => {
            let raw = take_bytes(bytes, *cursor, *n, path)?;
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            let rendered = printable(&String::from_utf8_lossy(&raw[..end]));
            out.push(FieldValue {
                path: path.to_string(),
                offset: *cursor,
                len: *n,
                type_name: ty.type_name(),
                kind: FieldKind::Leaf,
                raw: raw.to_vec(),
                rendered,
            });
            *cursor += n;
            Ok(None)
        }
        FieldType::Bytes(n) => {
            let raw = take_bytes(bytes, *cursor, *n, path)?;
            let rendered = hex_string(raw);
            out.push(FieldValue {
                path: path.to_string(),
                offset: *cursor,
                len: *n,
                type_name: ty.type_name(),
                kind: FieldKind::Leaf,
                raw: raw.to_vec(),
                rendered,
            });
            *cursor += n;
            Ok(None)
        }
        FieldType::StructRef(name) => {
            let start_idx = out.len();
            let start_cursor = *cursor;
            let def = &template.structs[name];
            eval_struct(template, def, bytes, cursor, path, out)?;
            let len = *cursor - start_cursor;
            let group = FieldValue {
                path: path.to_string(),
                offset: start_cursor,
                len,
                type_name: name.clone(),
                kind: FieldKind::Group,
                raw: Vec::new(),
                rendered: format!("{{ {} field(s) }}", def.fields.len()),
            };
            out.insert(start_idx, group);
            Ok(None)
        }
        FieldType::Array { elem, count } => {
            let n = match count {
                CountSpec::Fixed(n) => *n,
                CountSpec::FieldRef(name) => {
                    let value = ints[name];
                    if value < 0 {
                        return Err(EvalError::NegativeCount {
                            path: path.to_string(),
                            count_field: name.clone(),
                            value,
                        });
                    }
                    value as usize
                }
            };
            let start_idx = out.len();
            let start_cursor = *cursor;
            for idx in 0..n {
                let elem_path = format!("{path}[{idx}]");
                eval_field(
                    template,
                    elem,
                    bytes,
                    cursor,
                    &elem_path,
                    out,
                    ints,
                    default_endian,
                )?;
            }
            let len = *cursor - start_cursor;
            let group = FieldValue {
                path: path.to_string(),
                offset: start_cursor,
                len,
                type_name: ty.type_name(),
                kind: FieldKind::Group,
                raw: Vec::new(),
                rendered: format!("[{n} element(s)]"),
            };
            out.insert(start_idx, group);
            Ok(None)
        }
    }
}

/// Text decoded from a file is untrusted: control characters (terminal
/// escape sequences, newlines that would break the field table) are shown
/// as `\u{..}` escapes instead of being written to the terminal raw.
fn printable(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

fn take_bytes<'a>(
    bytes: &'a [u8],
    offset: usize,
    n: usize,
    path: &str,
) -> Result<&'a [u8], EvalError> {
    if offset > bytes.len() || bytes.len() - offset < n {
        let available = bytes.len().saturating_sub(offset);
        return Err(EvalError::UnexpectedEof {
            path: path.to_string(),
            offset,
            needed: n,
            available,
        });
    }
    Ok(&bytes[offset..offset + n])
}

fn decode_int(raw: &[u8], bits: u8, signed: bool, endian: Endianness) -> i128 {
    let n = (bits / 8) as usize;
    let mut buf = [0u8; 8];
    match endian {
        Endianness::Little => buf[..n].copy_from_slice(raw),
        Endianness::Big => {
            for (i, b) in raw.iter().enumerate() {
                buf[n - 1 - i] = *b;
            }
        }
    }
    let u = u64::from_le_bytes(buf);
    if signed {
        let shift = 64 - bits as u32;
        (((u << shift) as i64) >> shift) as i128
    } else {
        u as i128
    }
}

fn render_float(raw: &[u8], bits: u8, endian: Endianness) -> String {
    match bits {
        32 => {
            let mut b = [0u8; 4];
            b.copy_from_slice(raw);
            let v = match endian {
                Endianness::Little => f32::from_le_bytes(b),
                Endianness::Big => f32::from_be_bytes(b),
            };
            format!("{v}")
        }
        64 => {
            let mut b = [0u8; 8];
            b.copy_from_slice(raw);
            let v = match endian {
                Endianness::Little => f64::from_le_bytes(b),
                Endianness::Big => f64::from_be_bytes(b),
            };
            format!("{v}")
        }
        _ => unreachable!("float bit width validated at parse time"),
    }
}

pub fn hex_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_template;

    /// Regression: `ascii`/`cstr` text from the file was rendered verbatim,
    /// so a crafted file could inject terminal escape sequences into
    /// `inspect`/`fields` output.
    #[test]
    fn decoded_text_cannot_carry_control_characters() {
        let t = parse_template("struct A { a: ascii[6]; c: cstr[4]; } root A;").unwrap();
        let fields = apply(&t, b"\x1b[2Jhi\nx\x07\0").unwrap();
        assert_eq!(fields[0].rendered, "\\u{1b}[2Jhi");
        assert_eq!(fields[1].rendered, "\\u{a}x\\u{7}");
        let plain = apply(&t, b"hello\0abc\0").unwrap();
        assert_eq!(plain[0].rendered, "hello");
        assert_eq!(plain[1].rendered, "abc");
    }

    /// Regression: an array of zero-sized structs with a count read from
    /// the file looped (and allocated) once per count — a 4-byte file and a
    /// one-line template hung the tool. Evaluation is now bounded.
    #[test]
    fn zero_sized_array_elements_cannot_run_away() {
        let t = parse_template("struct E { } struct A { n: u32le; z: E[n]; } root A;").unwrap();
        match apply(&t, &[0xFF, 0xFF, 0xFF, 0x7F]) {
            Err(EvalError::TooManyFields { limit, .. }) => assert_eq!(limit, field_limit(4)),
            other => panic!("expected TooManyFields, got {other:?}"),
        }
    }

    /// Regression: zero-sized fields shared through a binary "diamond" of
    /// structs produced 2^n fields from a 1-byte file.
    #[test]
    fn exponential_zero_sized_nesting_is_bounded() {
        let mut src = String::from("struct S0 { x: u8[0]; }\n");
        for i in 1..30 {
            src += &format!("struct S{i} {{ a: S{p}; b: S{p}; }}\n", p = i - 1);
        }
        src += "root S29;";
        let t = parse_template(&src).unwrap();
        assert!(matches!(
            apply(&t, &[1]),
            Err(EvalError::TooManyFields { .. })
        ));
    }

    #[test]
    fn nesting_at_the_depth_limit_still_evaluates() {
        let depth = crate::parser::MAX_STRUCT_DEPTH;
        let mut src = String::from("struct T1 { x: u8; }\n");
        for i in 2..=depth {
            src += &format!("struct T{i} {{ a: T{p}; }}\n", p = i - 1);
        }
        src += &format!("root T{depth};");
        let t = parse_template(&src).unwrap();
        let fields = apply(&t, &[7]).unwrap();
        assert_eq!(fields.len(), depth);
        assert_eq!(fields.last().unwrap().rendered, "7");
    }

    #[test]
    fn decodes_scalar_fields_little_endian() {
        let t = parse_template("struct A { a: u8; b: u16le; c: i32le; } root A;").unwrap();
        let bytes = [0x2Au8, 0x34, 0x12, 0xFF, 0xFF, 0xFF, 0xFF];
        let fields = apply(&t, &bytes).unwrap();
        assert_eq!(fields[0].rendered, "42");
        assert_eq!(fields[1].rendered, "4660");
        assert_eq!(fields[2].rendered, "-1");
    }

    #[test]
    fn decodes_big_endian_default() {
        let t = parse_template("endian be; struct A { a: u32; } root A;").unwrap();
        let bytes = [0x00, 0x00, 0x01, 0x00];
        let fields = apply(&t, &bytes).unwrap();
        assert_eq!(fields[0].rendered, "256");
    }

    #[test]
    fn field_endian_overrides_default() {
        let t = parse_template("endian be; struct A { a: u16le; } root A;").unwrap();
        let bytes = [0x01, 0x00];
        let fields = apply(&t, &bytes).unwrap();
        assert_eq!(fields[0].rendered, "1");
    }

    #[test]
    fn decodes_signed_bytes_correctly() {
        let t = parse_template("struct A { a: i8; } root A;").unwrap();
        let bytes = [0xFFu8];
        let fields = apply(&t, &bytes).unwrap();
        assert_eq!(fields[0].rendered, "-1");
    }

    #[test]
    fn decodes_float_values() {
        let t = parse_template("struct A { a: f32le; b: f64le; } root A;").unwrap();
        let mut bytes = 1.5f32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&2.25f64.to_le_bytes());
        let fields = apply(&t, &bytes).unwrap();
        assert_eq!(fields[0].rendered, "1.5");
        assert_eq!(fields[1].rendered, "2.25");
    }

    #[test]
    fn matches_magic_literal() {
        let t = parse_template(r#"struct A { m: magic "PNG"; } root A;"#).unwrap();
        let fields = apply(&t, b"PNGtail").unwrap();
        assert_eq!(fields[0].rendered, "50 4E 47");
    }

    #[test]
    fn rejects_magic_mismatch() {
        let t = parse_template(r#"struct A { m: magic "PNG"; } root A;"#).unwrap();
        let err = apply(&t, b"GIFtail").unwrap_err();
        match err {
            EvalError::MagicMismatch {
                expected, actual, ..
            } => {
                assert_eq!(expected, b"PNG");
                assert_eq!(actual, b"GIF");
            }
            other => panic!("expected MagicMismatch, got {other:?}"),
        }
    }

    #[test]
    fn trims_ascii_trailing_nul_padding() {
        let t = parse_template("struct A { s: ascii[8]; } root A;").unwrap();
        let bytes = b"hi\0\0\0\0\0\0";
        let fields = apply(&t, bytes).unwrap();
        assert_eq!(fields[0].rendered, "hi");
    }

    #[test]
    fn cstr_stops_at_first_nul() {
        let t = parse_template("struct A { s: cstr[8]; } root A;").unwrap();
        let bytes = b"hi\0garbage";
        let fields = apply(&t, bytes).unwrap();
        assert_eq!(fields[0].rendered, "hi");
    }

    #[test]
    fn bytes_field_renders_hex() {
        let t = parse_template("struct A { b: bytes[3]; } root A;").unwrap();
        let fields = apply(&t, &[0xDE, 0xAD, 0xBE]).unwrap();
        assert_eq!(fields[0].rendered, "DE AD BE");
    }

    #[test]
    fn nested_struct_produces_group_and_children_with_dotted_paths() {
        let t =
            parse_template("struct Rgb { r: u8; g: u8; b: u8; } struct A { pixel: Rgb; } root A;")
                .unwrap();
        let fields = apply(&t, &[10, 20, 30]).unwrap();
        let paths: Vec<&str> = fields.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["pixel", "pixel.r", "pixel.g", "pixel.b"]);
        assert_eq!(fields[0].kind, FieldKind::Group);
        assert_eq!(fields[0].len, 3);
    }

    #[test]
    fn array_with_fixed_count_produces_indexed_children() {
        let t = parse_template("struct A { items: u8[3]; } root A;").unwrap();
        let fields = apply(&t, &[1, 2, 3]).unwrap();
        let paths: Vec<&str> = fields.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["items", "items[0]", "items[1]", "items[2]"]);
    }

    #[test]
    fn array_with_field_ref_count_reads_dynamic_length() {
        let t = parse_template("struct A { count: u8; items: u16le[count]; } root A;").unwrap();
        let bytes = [2u8, 0x01, 0x00, 0x02, 0x00];
        let fields = apply(&t, &bytes).unwrap();
        let group = fields.iter().find(|f| f.path == "items").unwrap();
        assert_eq!(group.rendered, "[2 element(s)]");
        assert_eq!(
            fields
                .iter()
                .filter(|f| f.path.starts_with("items["))
                .count(),
            2
        );
    }

    #[test]
    fn zero_length_array_produces_only_group() {
        let t = parse_template("struct A { count: u8; items: u16le[count]; } root A;").unwrap();
        let bytes = [0u8];
        let fields = apply(&t, &bytes).unwrap();
        assert_eq!(fields.len(), 2); // count + items group, no elements
        assert_eq!(fields[1].rendered, "[0 element(s)]");
    }

    #[test]
    fn array_of_structs_produces_full_nesting() {
        let t = parse_template(
            "struct Rgb { r: u8; g: u8; b: u8; } struct A { pixels: Rgb[2]; } root A;",
        )
        .unwrap();
        let fields = apply(&t, &[1, 2, 3, 4, 5, 6]).unwrap();
        let paths: Vec<&str> = fields.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "pixels",
                "pixels[0]",
                "pixels[0].r",
                "pixels[0].g",
                "pixels[0].b",
                "pixels[1]",
                "pixels[1].r",
                "pixels[1].g",
                "pixels[1].b",
            ]
        );
    }

    #[test]
    fn reports_unexpected_eof() {
        let t = parse_template("struct A { a: u32; } root A;").unwrap();
        let err = apply(&t, &[1, 2]).unwrap_err();
        match err {
            EvalError::UnexpectedEof {
                needed, available, ..
            } => {
                assert_eq!(needed, 4);
                assert_eq!(available, 2);
            }
            other => panic!("expected UnexpectedEof, got {other:?}"),
        }
    }

    #[test]
    fn offsets_and_lengths_are_correct() {
        let t = parse_template("struct A { a: u8; b: u16le; c: bytes[2]; } root A;").unwrap();
        let fields = apply(&t, &[0, 0, 0, 9, 9]).unwrap();
        assert_eq!((fields[0].offset, fields[0].len), (0, 1));
        assert_eq!((fields[1].offset, fields[1].len), (1, 2));
        assert_eq!((fields[2].offset, fields[2].len), (3, 2));
    }
}
