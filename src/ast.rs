//! Abstract syntax tree for the hexatlas template DSL.

use std::collections::HashMap;
use std::fmt;

/// Byte order used when decoding a multi-byte scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

impl fmt::Display for Endianness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Endianness::Little => write!(f, "le"),
            Endianness::Big => write!(f, "be"),
        }
    }
}

/// A literal byte sequence a `magic` field must match exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicLiteral {
    pub bytes: Vec<u8>,
    /// Source text, kept for error/render messages (e.g. `"PNG"` or `0x89504E47`).
    pub source: String,
}

/// How many times an array field repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountSpec {
    /// A literal, fixed repeat count.
    Fixed(usize),
    /// The repeat count is read from a previously decoded sibling integer field.
    FieldRef(String),
}

/// The type of a single field within a struct.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Int {
        bits: u8,
        signed: bool,
        endian: Option<Endianness>,
    },
    Float {
        bits: u8,
        endian: Option<Endianness>,
    },
    Magic(MagicLiteral),
    /// Fixed-width ASCII text, trailing NUL bytes trimmed when rendered.
    Ascii(usize),
    /// NUL-terminated string stored in a fixed-size buffer.
    CStr(usize),
    /// Opaque fixed-size byte blob.
    Bytes(usize),
    /// Reference to another struct defined in the same template.
    StructRef(String),
    /// A repeated element.
    Array {
        elem: Box<FieldType>,
        count: CountSpec,
    },
}

impl FieldType {
    /// Static byte size when known without reading any data (arrays sized by a
    /// field reference, and struct references, are resolved at eval time).
    pub fn fixed_size(&self) -> Option<usize> {
        match self {
            FieldType::Int { bits, .. } | FieldType::Float { bits, .. } => Some(*bits as usize / 8),
            FieldType::Magic(m) => Some(m.bytes.len()),
            FieldType::Ascii(n) | FieldType::CStr(n) | FieldType::Bytes(n) => Some(*n),
            FieldType::StructRef(_) => None,
            FieldType::Array { elem, count } => match count {
                CountSpec::Fixed(n) => elem.fixed_size().map(|s| s * n),
                CountSpec::FieldRef(_) => None,
            },
        }
    }

    /// Short human-readable type name used in rendered output.
    pub fn type_name(&self) -> String {
        match self {
            FieldType::Int {
                bits,
                signed,
                endian,
            } => {
                let sign = if *signed { "i" } else { "u" };
                match endian {
                    Some(e) => format!("{sign}{bits}{e}"),
                    None => format!("{sign}{bits}"),
                }
            }
            FieldType::Float { bits, endian } => match endian {
                Some(e) => format!("f{bits}{e}"),
                None => format!("f{bits}"),
            },
            FieldType::Magic(m) => format!("magic[{}]", m.bytes.len()),
            FieldType::Ascii(n) => format!("ascii[{n}]"),
            FieldType::CStr(n) => format!("cstr[{n}]"),
            FieldType::Bytes(n) => format!("bytes[{n}]"),
            FieldType::StructRef(name) => name.clone(),
            FieldType::Array { elem, count } => {
                let count_str = match count {
                    CountSpec::Fixed(n) => n.to_string(),
                    CountSpec::FieldRef(name) => name.clone(),
                };
                format!("{}[{}]", elem.type_name(), count_str)
            }
        }
    }
}

/// A single named field inside a struct definition.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub ty: FieldType,
}

/// A named, ordered collection of fields.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
}

/// A fully parsed template file: zero or more struct definitions plus the
/// name of the struct applied at offset 0 and the default endianness.
#[derive(Debug, Clone)]
pub struct TemplateFile {
    pub default_endian: Endianness,
    pub structs: HashMap<String, StructDef>,
    pub struct_order: Vec<String>,
    pub root: String,
}
