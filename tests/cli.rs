//! End-to-end tests that exercise the compiled `hexatlas` binary the same
//! way a user would, against the bundled PNG example.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hexatlas"))
}

#[test]
fn check_reports_template_structure() {
    let out = bin()
        .args(["check", "examples/formats/png.hext"])
        .output()
        .expect("run hexatlas check");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("root struct: PngHeader"));
    assert!(stdout.contains("width: u32"));
}

#[test]
fn inspect_decodes_real_png_header() {
    let out = bin()
        .args([
            "inspect",
            "examples/formats/png.hext",
            "examples/fixtures/a.png",
            "--no-hex",
        ])
        .output()
        .expect("run hexatlas inspect");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ihdr.width"));
    assert!(stdout.contains("2\n") || stdout.contains("2\r\n"));
    assert!(stdout.contains("ihdr.chunk_type"));
    assert!(stdout.contains("49 48 44 52"));
}

#[test]
fn inspect_hex_dump_shows_offsets_and_ascii_column() {
    let out = bin()
        .args([
            "inspect",
            "examples/formats/png.hext",
            "examples/fixtures/a.png",
        ])
        .output()
        .expect("run hexatlas inspect");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("00000000  89 50 4E 47"));
    assert!(stdout.contains("|.PNG"));
}

#[test]
fn fields_json_is_well_formed_and_field_complete() {
    let out = bin()
        .args([
            "fields",
            "examples/formats/png.hext",
            "examples/fixtures/a.png",
            "--format",
            "json",
        ])
        .output()
        .expect("run hexatlas fields");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.trim_start().starts_with('['));
    assert!(stdout.trim_end().ends_with(']'));
    for expected in [
        "\"path\": \"signature\"",
        "\"path\": \"ihdr.width\"",
        "\"value\": \"2\"",
    ] {
        assert!(stdout.contains(expected), "missing {expected} in {stdout}");
    }
}

#[test]
fn diff_reports_changed_width_and_crc_between_fixtures() {
    let out = bin()
        .args([
            "diff",
            "examples/formats/png.hext",
            "examples/fixtures/a.png",
            "examples/fixtures/b.png",
        ])
        .output()
        .expect("run hexatlas diff");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("~ ihdr.width"));
    assert!(stdout.contains("2 -> 3"));
    assert!(stdout.contains("2 changed, 0 added, 0 removed"));
}

#[test]
fn diff_of_identical_files_reports_no_differences() {
    let out = bin()
        .args([
            "diff",
            "examples/formats/png.hext",
            "examples/fixtures/a.png",
            "examples/fixtures/a.png",
        ])
        .output()
        .expect("run hexatlas diff");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "no differences\n");
}

#[test]
fn inspect_reports_magic_mismatch_and_fails() {
    let out = bin()
        .args(["inspect", "examples/formats/png.hext", "Cargo.toml"])
        .output()
        .expect("run hexatlas inspect");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("magic mismatch"));
}

#[test]
fn check_reports_missing_template_file() {
    let out = bin()
        .args(["check", "examples/formats/does-not-exist.hext"])
        .output()
        .expect("run hexatlas check");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("failed to read template"));
}
