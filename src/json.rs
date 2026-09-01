//! Minimal, dependency-free JSON encoding for field listings.

use crate::eval::{FieldKind, FieldValue};

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Renders decoded fields as a JSON array, one object per field, in
/// document order. Kept deterministic: no HashMap iteration, fixed key
/// order.
pub fn fields_to_json(fields: &[FieldValue]) -> String {
    let mut out = String::from("[\n");
    for (i, f) in fields.iter().enumerate() {
        let kind = match f.kind {
            FieldKind::Leaf => "leaf",
            FieldKind::Group => "group",
        };
        let raw_hex = crate::eval::hex_string(&f.raw);
        out.push_str(&format!(
            "  {{\"path\": \"{}\", \"offset\": {}, \"len\": {}, \"kind\": \"{}\", \"type\": \"{}\", \"value\": \"{}\", \"raw_hex\": \"{}\"}}",
            escape(&f.path),
            f.offset,
            f.len,
            kind,
            escape(&f.type_name),
            escape(&f.rendered),
            escape(&raw_hex),
        ));
        if i + 1 < fields.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push(']');
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::apply;
    use crate::parser::parse_template;

    #[test]
    fn encodes_group_field_with_empty_raw_hex() {
        let t = parse_template("struct A { count: u8; items: u8[count]; } root A;").unwrap();
        let fields = apply(&t, &[0]).unwrap();
        let json = fields_to_json(&fields);
        assert!(json.contains("\"kind\": \"group\""));
        assert!(json.contains("\"raw_hex\": \"\""));
    }

    #[test]
    fn encodes_scalar_fields() {
        let t = parse_template("struct A { a: u8; } root A;").unwrap();
        let fields = apply(&t, &[42]).unwrap();
        let json = fields_to_json(&fields);
        assert!(json.contains("\"path\": \"a\""));
        assert!(json.contains("\"value\": \"42\""));
        assert!(json.contains("\"kind\": \"leaf\""));
        assert!(json.contains("\"raw_hex\": \"2A\""));
    }

    #[test]
    fn escapes_special_characters_in_strings() {
        let t = parse_template("struct A { a: ascii[4]; } root A;").unwrap();
        let fields = apply(&t, b"a\"b\0").unwrap();
        let json = fields_to_json(&fields);
        assert!(json.contains("a\\\"b"));
    }

    #[test]
    fn is_valid_json_bracket_structure() {
        let t = parse_template("struct A { a: u8; b: u8; } root A;").unwrap();
        let fields = apply(&t, &[1, 2]).unwrap();
        let json = fields_to_json(&fields);
        assert!(json.starts_with("[\n"));
        assert!(json.trim_end().ends_with(']'));
        assert!(json.contains("  {\"path\""));
    }
}
