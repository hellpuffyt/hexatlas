use clap::{Parser, Subcommand, ValueEnum};
use hexatlas::diff::{diff_fields, render_diff};
use hexatlas::eval::apply;
use hexatlas::hexview::{render_field_table, render_hex_dump};
use hexatlas::json::fields_to_json;
use hexatlas::parser::parse_template;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "hexatlas",
    version,
    about = "Inspect and diff binary files against reusable structure templates."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a template and report the structs and fields it defines.
    Check {
        /// Path to a `.hext` template file.
        template: PathBuf,
    },
    /// Apply a template to a file and print an annotated hex dump plus a field table.
    Inspect {
        template: PathBuf,
        file: PathBuf,
        /// Colorize the hex dump by owning field.
        #[arg(long)]
        color: bool,
        /// Skip the hex dump, print only the field table.
        #[arg(long)]
        no_hex: bool,
    },
    /// Apply a template to a file and print the decoded fields.
    Fields {
        template: PathBuf,
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Apply a template to two files and print the field-level differences.
    Diff {
        template: PathBuf,
        file_a: PathBuf,
        file_b: PathBuf,
    },
}

fn read_template(path: &PathBuf) -> Result<hexatlas::ast::TemplateFile, String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("failed to read template `{}`: {e}", path.display()))?;
    parse_template(&src).map_err(|e| format!("template error in `{}`: {e}", path.display()))
}

fn read_file(path: &PathBuf) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("failed to read file `{}`: {e}", path.display()))
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { template } => {
            let t = read_template(&template)?;
            println!("template OK: {}", template.display());
            println!("default endian: {}", t.default_endian);
            println!("root struct: {}", t.root);
            println!("structs:");
            for name in &t.struct_order {
                let def = &t.structs[name];
                println!("  {} ({} field(s))", def.name, def.fields.len());
                for field in &def.fields {
                    println!("    {}: {}", field.name, field.ty.type_name());
                }
            }
            Ok(())
        }
        Command::Inspect {
            template,
            file,
            color,
            no_hex,
        } => {
            let t = read_template(&template)?;
            let bytes = read_file(&file)?;
            let fields = apply(&t, &bytes)
                .map_err(|e| format!("{} ({}): {e}", file.display(), template.display()))?;
            if !no_hex {
                print!("{}", render_hex_dump(&bytes, &fields, color));
                println!();
            }
            print!("{}", render_field_table(&fields));
            Ok(())
        }
        Command::Fields {
            template,
            file,
            format,
        } => {
            let t = read_template(&template)?;
            let bytes = read_file(&file)?;
            let fields = apply(&t, &bytes)
                .map_err(|e| format!("{} ({}): {e}", file.display(), template.display()))?;
            match format {
                OutputFormat::Text => print!("{}", render_field_table(&fields)),
                OutputFormat::Json => print!("{}", fields_to_json(&fields)),
            }
            Ok(())
        }
        Command::Diff {
            template,
            file_a,
            file_b,
        } => {
            let t = read_template(&template)?;
            let bytes_a = read_file(&file_a)?;
            let bytes_b = read_file(&file_b)?;
            let fa = apply(&t, &bytes_a)
                .map_err(|e| format!("{} ({}): {e}", file_a.display(), template.display()))?;
            let fb = apply(&t, &bytes_b)
                .map_err(|e| format!("{} ({}): {e}", file_b.display(), template.display()))?;
            let entries = diff_fields(&fa, &fb);
            print!("{}", render_diff(&entries));
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
