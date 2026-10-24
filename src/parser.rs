//! Recursive-descent parser turning a token stream into a [`TemplateFile`].

use crate::ast::*;
use crate::lexer::{lex, LexError, SpannedToken, Token};
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, col {}: {}", self.line, self.col, self.message)
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            message: e.message,
            line: e.line,
            col: e.col,
        }
    }
}

struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &SpannedToken {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> SpannedToken {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        let t = self.peek();
        Err(ParseError {
            message: message.into(),
            line: t.line,
            col: t.col,
        })
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek().token.clone() {
            Token::Ident(s) => {
                self.bump();
                Ok(s)
            }
            other => self.err(format!("expected identifier, found {other}")),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParseError> {
        if self.peek().token == expected {
            self.bump();
            Ok(())
        } else {
            let found = self.peek().token.clone();
            self.err(format!("expected {expected}, found {found}"))
        }
    }

    fn parse_template(&mut self) -> Result<TemplateFile, ParseError> {
        let mut default_endian = Endianness::Little;
        let mut structs: HashMap<String, StructDef> = HashMap::new();
        let mut struct_order: Vec<String> = Vec::new();
        let mut root: Option<String> = None;

        loop {
            match self.peek().token.clone() {
                Token::Eof => break,
                Token::Ident(kw) if kw == "endian" => {
                    self.bump();
                    let dir = self.expect_ident()?;
                    default_endian = match dir.as_str() {
                        "le" => Endianness::Little,
                        "be" => Endianness::Big,
                        other => {
                            return self.err(format!(
                                "expected `le` or `be` after `endian`, found `{other}`"
                            ))
                        }
                    };
                    self.expect(Token::Semicolon)?;
                }
                Token::Ident(kw) if kw == "struct" => {
                    self.bump();
                    let def = self.parse_struct()?;
                    if structs.contains_key(&def.name) {
                        return self.err(format!("struct `{}` defined more than once", def.name));
                    }
                    struct_order.push(def.name.clone());
                    structs.insert(def.name.clone(), def);
                }
                Token::Ident(kw) if kw == "root" => {
                    self.bump();
                    let name = self.expect_ident()?;
                    self.expect(Token::Semicolon)?;
                    if root.is_some() {
                        return self.err("`root` declared more than once");
                    }
                    root = Some(name);
                }
                other => return self.err(format!("unexpected {other} at top level")),
            }
        }

        let root = root.ok_or_else(|| ParseError {
            message: "template has no `root` declaration".to_string(),
            line: 0,
            col: 0,
        })?;

        if !structs.contains_key(&root) {
            return Err(ParseError {
                message: format!("root struct `{root}` is not defined"),
                line: 0,
                col: 0,
            });
        }

        Ok(TemplateFile {
            default_endian,
            structs,
            struct_order,
            root,
        })
    }

    fn parse_struct(&mut self) -> Result<StructDef, ParseError> {
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;
        let mut fields = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        while self.peek().token != Token::RBrace {
            let field = self.parse_field()?;
            if !seen.insert(field.name.clone()) {
                return self.err(format!(
                    "duplicate field `{}` in struct `{}`",
                    field.name, name
                ));
            }
            fields.push(field);
        }
        self.expect(Token::RBrace)?;
        Ok(StructDef { name, fields })
    }

    fn parse_field(&mut self) -> Result<FieldDef, ParseError> {
        let name = self.expect_ident()?;
        self.expect(Token::Colon)?;
        let ty = self.parse_type()?;
        self.expect(Token::Semicolon)?;
        Ok(FieldDef { name, ty })
    }

    fn parse_bracket_count(&mut self) -> Result<usize, ParseError> {
        self.expect(Token::LBracket)?;
        let n = match self.peek().token.clone() {
            Token::Number(n) => {
                self.bump();
                n as usize
            }
            other => return self.err(format!("expected a size in brackets, found {other}")),
        };
        self.expect(Token::RBracket)?;
        Ok(n)
    }

    fn parse_type(&mut self) -> Result<FieldType, ParseError> {
        let name = self.expect_ident()?;
        match name.as_str() {
            "ascii" => {
                let n = self.parse_bracket_count()?;
                Ok(FieldType::Ascii(n))
            }
            "cstr" => {
                let n = self.parse_bracket_count()?;
                Ok(FieldType::CStr(n))
            }
            "bytes" => {
                let n = self.parse_bracket_count()?;
                Ok(FieldType::Bytes(n))
            }
            "magic" => match self.peek().token.clone() {
                Token::Str(s) => {
                    self.bump();
                    let bytes = s.clone().into_bytes();
                    Ok(FieldType::Magic(MagicLiteral {
                        bytes,
                        source: format!("\"{s}\""),
                    }))
                }
                Token::Number(n) => {
                    self.bump();
                    // Render as the minimal big-endian byte sequence covering `n`.
                    let mut bytes = n.to_be_bytes().to_vec();
                    while bytes.len() > 1 && bytes[0] == 0 {
                        bytes.remove(0);
                    }
                    Ok(FieldType::Magic(MagicLiteral {
                        bytes,
                        source: format!("0x{n:X}"),
                    }))
                }
                other => self.err(format!(
                    "expected a string or hex literal after `magic`, found {other}"
                )),
            },
            other_name => {
                let base = if let Some(ty) = parse_numeric_type(other_name) {
                    ty
                } else {
                    FieldType::StructRef(other_name.to_string())
                };
                if self.peek().token == Token::LBracket {
                    self.bump();
                    let count = match self.peek().token.clone() {
                        Token::Number(n) => {
                            self.bump();
                            CountSpec::Fixed(n as usize)
                        }
                        Token::Ident(field_name) => {
                            self.bump();
                            CountSpec::FieldRef(field_name)
                        }
                        other => {
                            return self
                                .err(format!("expected a size or field name, found {other}"))
                        }
                    };
                    self.expect(Token::RBracket)?;
                    Ok(FieldType::Array {
                        elem: Box::new(base),
                        count,
                    })
                } else {
                    Ok(base)
                }
            }
        }
    }
}

/// Parses `u8`/`i16be`/`f64le`-style identifiers into a bare (endian possibly
/// unset) numeric [`FieldType`]. Returns `None` if `name` is not a numeric
/// type identifier, in which case the caller treats it as a struct reference.
fn parse_numeric_type(name: &str) -> Option<FieldType> {
    if let Some(rest) = name.strip_prefix('f') {
        let (digits, endian) = split_endian_suffix(rest);
        let bits: u8 = digits.parse().ok()?;
        if bits == 32 || bits == 64 {
            return Some(FieldType::Float { bits, endian });
        }
        return None;
    }
    let (signed, rest) = if let Some(r) = name.strip_prefix('u') {
        (false, r)
    } else {
        let r = name.strip_prefix('i')?;
        (true, r)
    };
    let (digits, endian) = split_endian_suffix(rest);
    let bits: u8 = digits.parse().ok()?;
    if matches!(bits, 8 | 16 | 32 | 64) {
        Some(FieldType::Int {
            bits,
            signed,
            endian,
        })
    } else {
        None
    }
}

fn split_endian_suffix(s: &str) -> (&str, Option<Endianness>) {
    if let Some(r) = s.strip_suffix("le") {
        (r, Some(Endianness::Little))
    } else if let Some(r) = s.strip_suffix("be") {
        (r, Some(Endianness::Big))
    } else {
        (s, None)
    }
}

/// Parses and validates a template source string end to end.
pub fn parse_template(source: &str) -> Result<TemplateFile, ParseError> {
    let tokens = lex(source)?;
    let mut parser = Parser { tokens, pos: 0 };
    let template = parser.parse_template()?;
    validate(&template)?;
    Ok(template)
}

fn validate(template: &TemplateFile) -> Result<(), ParseError> {
    for name in &template.struct_order {
        let def = &template.structs[name];
        let mut int_fields_so_far: HashSet<String> = HashSet::new();
        for field in &def.fields {
            validate_type(template, &def.name, &field.ty, &int_fields_so_far)?;
            if is_integer(&field.ty) {
                int_fields_so_far.insert(field.name.clone());
            }
        }
    }
    check_no_cycles(template)?;
    Ok(())
}

fn is_integer(ty: &FieldType) -> bool {
    matches!(ty, FieldType::Int { .. })
}

fn validate_type(
    template: &TemplateFile,
    struct_name: &str,
    ty: &FieldType,
    int_fields_so_far: &HashSet<String>,
) -> Result<(), ParseError> {
    match ty {
        FieldType::StructRef(name) => {
            if !template.structs.contains_key(name) {
                return Err(ParseError {
                    message: format!("struct `{struct_name}` references undefined struct `{name}`"),
                    line: 0,
                    col: 0,
                });
            }
            Ok(())
        }
        FieldType::Array { elem, count } => {
            if let CountSpec::FieldRef(name) = count {
                if !int_fields_so_far.contains(name) {
                    return Err(ParseError {
                        message: format!(
                            "struct `{struct_name}` array count `{name}` must refer to an earlier integer field in the same struct"
                        ),
                        line: 0,
                        col: 0,
                    });
                }
            }
            validate_type(template, struct_name, elem, int_fields_so_far)
        }
        _ => Ok(()),
    }
}

/// Deepest allowed chain of nested struct references. Evaluation recurses
/// once per level, so this also bounds the evaluator's stack use.
pub const MAX_STRUCT_DEPTH: usize = 256;

/// Rejects cyclic struct references and nesting deeper than
/// [`MAX_STRUCT_DEPTH`]. Each struct's nesting height is computed once
/// (memoised), so a template whose structs share sub-structs many times over
/// ("diamonds") is checked in linear time instead of exponential time, and
/// the recursion here is itself bounded by `MAX_STRUCT_DEPTH`.
fn check_no_cycles(template: &TemplateFile) -> Result<(), ParseError> {
    fn too_deep(name: &str) -> ParseError {
        ParseError {
            message: format!("struct nesting deeper than {MAX_STRUCT_DEPTH} levels (at `{name}`)"),
            line: 0,
            col: 0,
        }
    }
    /// Returns the nesting height of `name` (a struct without struct
    /// fields has height 1).
    fn visit<'t>(
        template: &'t TemplateFile,
        name: &'t str,
        stack: &mut Vec<&'t str>,
        heights: &mut HashMap<&'t str, usize>,
    ) -> Result<usize, ParseError> {
        if let Some(&h) = heights.get(name) {
            return Ok(h);
        }
        if stack.contains(&name) {
            stack.push(name);
            return Err(ParseError {
                message: format!("cyclic struct reference: {}", stack.join(" -> ")),
                line: 0,
                col: 0,
            });
        }
        if stack.len() >= MAX_STRUCT_DEPTH {
            return Err(too_deep(name));
        }
        stack.push(name);
        let def = &template.structs[name];
        let mut height = 1;
        for field in &def.fields {
            let mut t = &field.ty;
            while let FieldType::Array { elem, .. } = t {
                t = elem;
            }
            if let FieldType::StructRef(inner) = t {
                height = height.max(1 + visit(template, inner, stack, heights)?);
            }
        }
        stack.pop();
        if height > MAX_STRUCT_DEPTH {
            return Err(too_deep(name));
        }
        heights.insert(name, height);
        Ok(height)
    }

    let mut heights = HashMap::new();
    for name in &template.struct_order {
        let mut stack = Vec::new();
        visit(template, name, &mut stack, &mut heights)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_template() {
        let src = "struct A { x: u8; } root A;";
        let t = parse_template(src).unwrap();
        assert_eq!(t.root, "A");
        assert_eq!(t.structs["A"].fields.len(), 1);
    }

    #[test]
    fn parses_numeric_types_with_endian_suffix() {
        let src = "struct A { a: u16le; b: u32be; c: i8; d: f64be; } root A;";
        let t = parse_template(src).unwrap();
        let fields = &t.structs["A"].fields;
        assert_eq!(
            fields[0].ty,
            FieldType::Int {
                bits: 16,
                signed: false,
                endian: Some(Endianness::Little)
            }
        );
        assert_eq!(
            fields[1].ty,
            FieldType::Int {
                bits: 32,
                signed: false,
                endian: Some(Endianness::Big)
            }
        );
        assert_eq!(
            fields[2].ty,
            FieldType::Int {
                bits: 8,
                signed: true,
                endian: None
            }
        );
        assert_eq!(
            fields[3].ty,
            FieldType::Float {
                bits: 64,
                endian: Some(Endianness::Big)
            }
        );
    }

    #[test]
    fn parses_magic_string_and_hex() {
        let src = r#"struct A { m: magic "PNG"; h: magic 0x89504E47; } root A;"#;
        let t = parse_template(src).unwrap();
        let fields = &t.structs["A"].fields;
        assert_eq!(
            fields[0].ty,
            FieldType::Magic(MagicLiteral {
                bytes: b"PNG".to_vec(),
                source: "\"PNG\"".into()
            })
        );
        assert_eq!(
            fields[1].ty,
            FieldType::Magic(MagicLiteral {
                bytes: vec![0x89, 0x50, 0x4E, 0x47],
                source: "0x89504E47".into()
            })
        );
    }

    #[test]
    fn parses_array_with_field_ref_count() {
        let src = "struct A { count: u16; items: u32[count]; } root A;";
        let t = parse_template(src).unwrap();
        let items = &t.structs["A"].fields[1];
        match &items.ty {
            FieldType::Array { elem, count } => {
                assert_eq!(
                    **elem,
                    FieldType::Int {
                        bits: 32,
                        signed: false,
                        endian: None
                    }
                );
                assert_eq!(*count, CountSpec::FieldRef("count".into()));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_struct_and_array_of_struct() {
        let src = "struct Rgba { r: u8; g: u8; b: u8; a: u8; } struct A { one: Rgba; many: Rgba[3]; } root A;";
        let t = parse_template(src).unwrap();
        assert_eq!(
            t.structs["A"].fields[0].ty,
            FieldType::StructRef("Rgba".into())
        );
    }

    #[test]
    fn rejects_undefined_struct_reference() {
        let src = "struct A { x: Missing; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("undefined struct"));
    }

    #[test]
    fn rejects_undefined_root() {
        let src = "struct A { x: u8; } root B;";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("not defined"));
    }

    #[test]
    fn rejects_missing_root() {
        let src = "struct A { x: u8; }";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("no `root`"));
    }

    #[test]
    fn rejects_duplicate_struct() {
        let src = "struct A { x: u8; } struct A { y: u8; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("defined more than once"));
    }

    #[test]
    fn rejects_duplicate_field() {
        let src = "struct A { x: u8; x: u8; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("duplicate field"));
    }

    #[test]
    fn rejects_array_count_ref_to_non_integer() {
        let src = "struct A { name: ascii[4]; items: u8[name]; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err
            .message
            .contains("must refer to an earlier integer field"));
    }

    #[test]
    fn rejects_array_count_ref_to_later_field() {
        let src = "struct A { items: u8[count]; count: u16; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err
            .message
            .contains("must refer to an earlier integer field"));
    }

    #[test]
    fn rejects_self_referential_struct() {
        let src = "struct A { next: A; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("cyclic struct reference"));
    }

    /// Regression: the cycle check re-walked shared sub-structs without
    /// memoisation, so a 40-struct "diamond" template took exponential time
    /// (`hexatlas check` never finished).
    #[test]
    fn diamond_templates_validate_in_linear_time() {
        let mut src = String::from("struct S0 { x: u8; }\n");
        for i in 1..60 {
            src += &format!("struct S{i} {{ a: S{p}; b: S{p}; }}\n", p = i - 1);
        }
        src += "root S59;";
        let started = std::time::Instant::now();
        parse_template(&src).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    /// Regression: a long chain of nested structs was accepted and then
    /// overflowed the stack during evaluation; nesting is now bounded.
    #[test]
    fn struct_nesting_deeper_than_the_limit_is_rejected() {
        for reverse in [false, true] {
            let mut defs: Vec<String> = vec!["struct T0 { x: u8; }".into()];
            for i in 1..=MAX_STRUCT_DEPTH {
                defs.push(format!("struct T{i} {{ a: T{p}; }}", p = i - 1));
            }
            if reverse {
                defs.reverse();
            }
            let src = format!("{}\nroot T{MAX_STRUCT_DEPTH};", defs.join("\n"));
            let err = parse_template(&src).unwrap_err();
            assert!(err.message.contains("deeper than"), "{}", err.message);
        }
    }

    #[test]
    fn rejects_mutually_recursive_structs() {
        let src = "struct A { b: B; } struct B { a: A; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("cyclic struct reference"));
    }

    #[test]
    fn endian_directive_sets_default() {
        let src = "endian be; struct A { x: u16; } root A;";
        let t = parse_template(src).unwrap();
        assert_eq!(t.default_endian, Endianness::Big);
    }

    #[test]
    fn rejects_bad_endian_direction() {
        let src = "endian middle; struct A { x: u8; } root A;";
        let err = parse_template(src).unwrap_err();
        assert!(err.message.contains("expected `le` or `be`"));
    }

    #[test]
    fn skips_comments_between_declarations() {
        let src = "# header\nstruct A { x: u8; } # trailing\nroot A;";
        let t = parse_template(src).unwrap();
        assert_eq!(t.root, "A");
    }
}
