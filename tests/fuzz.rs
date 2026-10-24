//! Deterministic robustness tests: templates and files are both untrusted,
//! so no combination may panic or run away.

use hexatlas::{diff, eval, hexview, json, parser};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

const TOKENS: &[&str] = &[
    "struct",
    "root",
    "endian",
    "le",
    "be",
    "magic",
    "{",
    "}",
    ";",
    ":",
    "[",
    "]",
    "A",
    "B",
    "n",
    "x",
    "u8",
    "u16",
    "u32le",
    "u64be",
    "i32",
    "f32",
    "f64le",
    "ascii",
    "cstr",
    "bytes",
    "0",
    "1",
    "4",
    "0xFF",
    "\"PNG\"",
    "\"\"",
    "0x",
    "-1",
    "99999999999999999999",
];

fn run_all(template: &str, a: &[u8], b: &[u8]) {
    let Ok(t) = parser::parse_template(template) else {
        return;
    };
    let fa = eval::apply(&t, a);
    let fb = eval::apply(&t, b);
    if let Ok(fa) = &fa {
        let _ = hexview::render_hex_dump(a, fa, true);
        let _ = hexview::render_field_table(fa);
        let _ = json::fields_to_json(fa);
        if let Ok(fb) = &fb {
            let _ = diff::render_diff(&diff::diff_fields(fa, fb));
        }
    }
}

#[test]
fn token_soup_templates_never_panic() {
    let mut rng = Rng(0xA71A5);
    for _ in 0..20_000 {
        let n = 1 + rng.below(40);
        let src: Vec<&str> = (0..n).map(|_| rng.pick(TOKENS)).collect();
        let bytes: Vec<u8> = (0..rng.below(64)).map(|_| rng.next() as u8).collect();
        run_all(&src.join(" "), &bytes, &bytes[..bytes.len() / 2]);
    }
}

#[test]
fn random_files_through_valid_templates_never_panic() {
    let templates = [
        include_str!("../examples/formats/png.hext"),
        "struct A { n: u8; items: u16be[n]; tail: cstr[3]; f: f32; } root A;",
        "struct P { x: i8; y: i64le; } struct A { n: u16le; ps: P[n]; s: ascii[5]; } root A;",
    ];
    let mut rng = Rng(99);
    for t in templates {
        for _ in 0..2_000 {
            let a: Vec<u8> = (0..rng.below(200)).map(|_| rng.next() as u8).collect();
            let b: Vec<u8> = (0..rng.below(200)).map(|_| rng.next() as u8).collect();
            run_all(t, &a, &b);
        }
    }
}

#[test]
fn random_bytes_as_template_never_panic() {
    let mut rng = Rng(5);
    for _ in 0..5_000 {
        let raw: Vec<u8> = (0..rng.below(120)).map(|_| rng.next() as u8).collect();
        let _ = parser::parse_template(&String::from_utf8_lossy(&raw));
    }
}
