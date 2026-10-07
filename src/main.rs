use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use qrplatba::{DEFAULT_SCALE, Payment, parse_amount};

/// Output format for the generated QR code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    /// Portable Network Graphics image.
    Png,
    /// Scalable Vector Graphics image.
    Svg,
}

/// Generate Czech QR payment (SPAYD/SPD) strings and QR images.
#[derive(Debug, Parser)]
#[command(name = "qrplatba", version, about, arg_required_else_help = true)]
struct Cli {
    /// IBAN of the receiving account.
    #[arg(long)]
    iban: String,

    /// Amount in major units, for example 1250.00 or 1250,00.
    #[arg(long)]
    amount: String,

    /// ISO 4217 currency code.
    #[arg(long, default_value = "CZK")]
    currency: String,

    /// Variable symbol, 1 to 10 digits.
    #[arg(long)]
    vs: Option<String>,

    /// Constant symbol, 1 to 4 digits.
    #[arg(long)]
    ks: Option<String>,

    /// Specific symbol, 1 to 4 digits.
    #[arg(long)]
    ss: Option<String>,

    /// Remittance message, at most 60 characters.
    #[arg(long)]
    message: Option<String>,

    /// Due date in YYYY-MM-DD form.
    #[arg(long)]
    due: Option<String>,

    /// QR image format.
    #[arg(long, value_enum, default_value = "png")]
    format: OutputFormat,

    /// Write output to this path instead of standard output.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Pixels per QR module.
    #[arg(long, default_value_t = DEFAULT_SCALE)]
    scale: u32,

    /// Print the SPD string instead of a QR image.
    #[arg(long)]
    spd_only: bool,
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("error: {error:#}");
        std::process::exit(2);
    }
}

fn run(cli: Cli) -> Result<()> {
    let amount_minor = parse_amount(&cli.amount)?;
    let mut payment = Payment::new(cli.iban, amount_minor).with_currency(cli.currency);
    if let Some(vs) = cli.vs {
        payment = payment.with_variable_symbol(vs);
    }
    if let Some(ks) = cli.ks {
        payment = payment.with_constant_symbol(ks);
    }
    if let Some(ss) = cli.ss {
        payment = payment.with_specific_symbol(ss);
    }
    if let Some(message) = cli.message {
        payment = payment.with_message(message);
    }
    if let Some(due) = cli.due {
        payment = payment.with_due_date(due);
    }

    if cli.spd_only {
        let mut spd = payment.to_spd()?;
        spd.push('\n');
        return write_output(spd.as_bytes(), cli.output.as_deref());
    }

    let bytes = match cli.format {
        OutputFormat::Png => payment.to_qr_png(cli.scale)?,
        OutputFormat::Svg => payment.to_qr_svg(cli.scale)?.into_bytes(),
    };
    write_output(&bytes, cli.output.as_deref())
}

fn write_output(bytes: &[u8], output: Option<&Path>) -> Result<()> {
    match output {
        Some(path) => {
            fs::write(path, bytes).with_context(|| format!("cannot write '{}'", path.display()))
        }
        None => {
            let mut stdout = io::stdout();
            stdout
                .write_all(bytes)
                .and_then(|()| stdout.flush())
                .context("cannot write to standard output")
        }
    }
}
