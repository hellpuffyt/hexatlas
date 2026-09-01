//! Renders an annotated hex + ASCII dump of a byte buffer, colored per
//! top-level template field.

use crate::eval::{FieldKind, FieldValue};

const PALETTE: &[u16] = &[32, 33, 34, 35, 36, 91, 92, 93, 94, 95, 96, 97];
const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

/// Returns the top-level name of a field path (`items[2].r` -> `items`).
fn top_level_name(path: &str) -> &str {
    let dot = path.find('.').unwrap_or(path.len());
    let bracket = path.find('[').unwrap_or(path.len());
    &path[..dot.min(bracket)]
}

/// Builds, per byte offset, the top-level field name owning that byte (if
/// any leaf field covers it).
fn build_owners(len: usize, fields: &[FieldValue]) -> Vec<Option<&str>> {
    let mut owners: Vec<Option<&str>> = vec![None; len];
    for field in fields {
        if field.kind != FieldKind::Leaf {
            continue;
        }
        let name = top_level_name(&field.path);
        let end = (field.offset + field.len).min(len);
        if field.offset < end {
            owners[field.offset..end].fill(Some(name));
        }
    }
    owners
}

fn color_for(name: &str, order: &[String]) -> u16 {
    let idx = order.iter().position(|n| n == name).unwrap_or(0);
    PALETTE[idx % PALETTE.len()]
}

fn ascii_repr(b: u8) -> char {
    if (0x20..=0x7e).contains(&b) {
        b as char
    } else {
        '.'
    }
}

/// Renders a classic 16-bytes-per-row hex + ASCII dump. When `color` is
/// true, each byte is wrapped in an ANSI color assigned to the top-level
/// field that owns it (unmapped bytes are dimmed); bytes reachable by no
/// field stay unmapped when the file is longer than what the template
/// consumed.
pub fn render_hex_dump(bytes: &[u8], fields: &[FieldValue], color: bool) -> String {
    let owners = build_owners(bytes.len(), fields);

    let mut order: Vec<String> = Vec::new();
    for o in owners.iter().flatten() {
        if !order.contains(&o.to_string()) {
            order.push(o.to_string());
        }
    }

    let mut out = String::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let end = (offset + 16).min(bytes.len());
        let row = &bytes[offset..end];

        out.push_str(&format!("{offset:08X}  "));

        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if i < row.len() {
                let b = row[i];
                let owner = owners[offset + i];
                let token = format!("{b:02X}");
                if color {
                    match owner {
                        Some(name) => {
                            let c = color_for(name, &order);
                            out.push_str(&format!("\x1b[{c}m{token}{RESET} "));
                        }
                        None => out.push_str(&format!("{DIM}{token}{RESET} ")),
                    }
                } else {
                    out.push_str(&token);
                    out.push(' ');
                }
            } else {
                out.push_str("   ");
            }
        }

        out.push_str(" |");
        for i in 0..row.len() {
            let b = row[i];
            let owner = owners[offset + i];
            let ch = ascii_repr(b);
            if color {
                match owner {
                    Some(name) => {
                        let c = color_for(name, &order);
                        out.push_str(&format!("\x1b[{c}m{ch}{RESET}"));
                    }
                    None => out.push_str(&format!("{DIM}{ch}{RESET}")),
                }
            } else {
                out.push(ch);
            }
        }
        out.push_str("|\n");

        offset += 16;
    }
    out
}

/// Renders the flat field table (offset, length, path, type, value) in
/// document order.
pub fn render_field_table(fields: &[FieldValue]) -> String {
    let mut out = String::new();
    out.push_str("OFFSET    LEN  PATH                           TYPE                 VALUE\n");
    for f in fields {
        let marker = match f.kind {
            FieldKind::Group => "+",
            FieldKind::Leaf => " ",
        };
        out.push_str(&format!(
            "{:08X}  {:>3}  {}{:<30} {:<20} {}\n",
            f.offset, f.len, marker, f.path, f.type_name, f.rendered
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::apply;
    use crate::parser::parse_template;

    #[test]
    fn top_level_name_strips_dots_and_brackets() {
        assert_eq!(top_level_name("a"), "a");
        assert_eq!(top_level_name("a.b"), "a");
        assert_eq!(top_level_name("a[0]"), "a");
        assert_eq!(top_level_name("a[0].b"), "a");
    }

    #[test]
    fn plain_dump_has_no_escape_codes() {
        let t = parse_template("struct A { a: u8; b: u8; } root A;").unwrap();
        let bytes = [1u8, 2];
        let fields = apply(&t, &bytes).unwrap();
        let dump = render_hex_dump(&bytes, &fields, false);
        assert!(!dump.contains('\x1b'));
        assert!(dump.contains("01 02"));
        assert!(dump.contains("|..|"));
    }

    #[test]
    fn color_dump_contains_escape_codes_for_mapped_bytes() {
        let t = parse_template("struct A { a: u8; } root A;").unwrap();
        let bytes = [1u8];
        let fields = apply(&t, &bytes).unwrap();
        let dump = render_hex_dump(&bytes, &fields, true);
        assert!(dump.contains('\x1b'));
    }

    #[test]
    fn dump_wraps_at_sixteen_bytes_per_row() {
        let t = parse_template("struct A { a: bytes[20]; } root A;").unwrap();
        let bytes = [0u8; 20];
        let fields = apply(&t, &bytes).unwrap();
        let dump = render_hex_dump(&bytes, &fields, false);
        assert_eq!(dump.lines().count(), 2);
        assert!(dump.starts_with("00000000"));
        assert!(dump.lines().nth(1).unwrap().starts_with("00000010"));
    }

    #[test]
    fn field_table_lists_groups_and_leaves() {
        let t = parse_template("struct A { count: u8; items: u8[count]; } root A;").unwrap();
        let bytes = [2u8, 5, 6];
        let fields = apply(&t, &bytes).unwrap();
        let table = render_field_table(&fields);
        assert!(table.contains("count"));
        assert!(table.contains("items[0]"));
        assert!(table.contains("+items"));
    }
}
