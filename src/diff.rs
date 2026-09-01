//! Structural, field-aware diff between two byte buffers decoded with the
//! same template.

use crate::eval::FieldValue;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffEntry {
    pub path: String,
    pub kind: DiffKind,
    pub type_name: String,
    pub offset_a: Option<usize>,
    pub offset_b: Option<usize>,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// Compares two decoded field lists (produced by applying the same template
/// to two files) and returns every path whose presence or rendered value
/// differs, in first-seen order (A's order, then any B-only paths).
pub fn diff_fields(a: &[FieldValue], b: &[FieldValue]) -> Vec<DiffEntry> {
    let map_a: HashMap<&str, &FieldValue> = a.iter().map(|f| (f.path.as_str(), f)).collect();
    let map_b: HashMap<&str, &FieldValue> = b.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut order: Vec<&str> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for f in a {
        if seen.insert(f.path.as_str()) {
            order.push(f.path.as_str());
        }
    }
    for f in b {
        if seen.insert(f.path.as_str()) {
            order.push(f.path.as_str());
        }
    }

    let mut out = Vec::new();
    for path in order {
        match (map_a.get(path), map_b.get(path)) {
            (Some(fa), Some(fb)) => {
                if fa.rendered != fb.rendered || fa.raw != fb.raw {
                    out.push(DiffEntry {
                        path: path.to_string(),
                        kind: DiffKind::Changed,
                        type_name: fa.type_name.clone(),
                        offset_a: Some(fa.offset),
                        offset_b: Some(fb.offset),
                        old: Some(fa.rendered.clone()),
                        new: Some(fb.rendered.clone()),
                    });
                }
            }
            (Some(fa), None) => out.push(DiffEntry {
                path: path.to_string(),
                kind: DiffKind::Removed,
                type_name: fa.type_name.clone(),
                offset_a: Some(fa.offset),
                offset_b: None,
                old: Some(fa.rendered.clone()),
                new: None,
            }),
            (None, Some(fb)) => out.push(DiffEntry {
                path: path.to_string(),
                kind: DiffKind::Added,
                type_name: fb.type_name.clone(),
                offset_a: None,
                offset_b: Some(fb.offset),
                old: None,
                new: Some(fb.rendered.clone()),
            }),
            (None, None) => unreachable!("path came from a or b"),
        }
    }
    out
}

/// Renders a diff entry list as a unified-diff-flavored text report.
pub fn render_diff(entries: &[DiffEntry]) -> String {
    if entries.is_empty() {
        return "no differences\n".to_string();
    }
    let mut out = String::new();
    for e in entries {
        match e.kind {
            DiffKind::Changed => out.push_str(&format!(
                "~ {:<28} {:<10} {} -> {}\n",
                e.path,
                e.type_name,
                e.old.as_deref().unwrap_or(""),
                e.new.as_deref().unwrap_or("")
            )),
            DiffKind::Removed => out.push_str(&format!(
                "- {:<28} {:<10} {}\n",
                e.path,
                e.type_name,
                e.old.as_deref().unwrap_or("")
            )),
            DiffKind::Added => out.push_str(&format!(
                "+ {:<28} {:<10} {}\n",
                e.path,
                e.type_name,
                e.new.as_deref().unwrap_or("")
            )),
        }
    }
    let changed = entries
        .iter()
        .filter(|e| e.kind == DiffKind::Changed)
        .count();
    let added = entries.iter().filter(|e| e.kind == DiffKind::Added).count();
    let removed = entries
        .iter()
        .filter(|e| e.kind == DiffKind::Removed)
        .count();
    out.push_str(&format!(
        "\n{changed} changed, {added} added, {removed} removed\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::apply;
    use crate::parser::parse_template;

    #[test]
    fn identical_files_produce_no_diff() {
        let t = parse_template("struct A { a: u8; b: u16le; } root A;").unwrap();
        let bytes = [1u8, 2, 3];
        let fa = apply(&t, &bytes).unwrap();
        let fb = apply(&t, &bytes).unwrap();
        let entries = diff_fields(&fa, &fb);
        assert!(entries.is_empty());
        assert_eq!(render_diff(&entries), "no differences\n");
    }

    #[test]
    fn detects_changed_scalar_field() {
        let t = parse_template("struct A { a: u8; } root A;").unwrap();
        let fa = apply(&t, &[1]).unwrap();
        let fb = apply(&t, &[2]).unwrap();
        let entries = diff_fields(&fa, &fb);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, DiffKind::Changed);
        assert_eq!(entries[0].old.as_deref(), Some("1"));
        assert_eq!(entries[0].new.as_deref(), Some("2"));
    }

    #[test]
    fn detects_added_and_removed_array_elements() {
        let t = parse_template("struct A { count: u8; items: u8[count]; } root A;").unwrap();
        let fa = apply(&t, &[1, 10]).unwrap();
        let fb = apply(&t, &[2, 10, 20]).unwrap();
        let entries = diff_fields(&fa, &fb);
        let added: Vec<&DiffEntry> = entries
            .iter()
            .filter(|e| e.kind == DiffKind::Added)
            .collect();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].path, "items[1]");
        let changed_count = entries
            .iter()
            .find(|e| e.path == "count")
            .expect("count field diff present");
        assert_eq!(changed_count.kind, DiffKind::Changed);
        let changed_group = entries
            .iter()
            .find(|e| e.path == "items")
            .expect("items group diff present");
        assert_eq!(changed_group.old.as_deref(), Some("[1 element(s)]"));
        assert_eq!(changed_group.new.as_deref(), Some("[2 element(s)]"));
    }

    #[test]
    fn render_diff_reports_summary_counts() {
        let t = parse_template("struct A { a: u8; b: u8; } root A;").unwrap();
        let fa = apply(&t, &[1, 5]).unwrap();
        let fb = apply(&t, &[2, 5]).unwrap();
        let entries = diff_fields(&fa, &fb);
        let text = render_diff(&entries);
        assert!(text.contains("1 changed, 0 added, 0 removed"));
        assert!(text.contains("~ a"));
    }
}
