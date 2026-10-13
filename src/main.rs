use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use qrplatba::batch::{PaymentScheme, QrFormat, process_batch, zip_batch};
use qrplatba::{DEFAULT_SCALE, EpcPayment, Payment, bank, parse_amount, parse_spd};
use serde::Serialize;

/// Output format for the generated QR code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    /// Portable Network Graphics image.
    Png,
    /// Scalable Vector Graphics image.
    Svg,
}

impl From<OutputFormat> for QrFormat {
    fn from(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Png => QrFormat::Png,
            OutputFormat::Svg => QrFormat::Svg,
        }
    }
}

/// Payment scheme for the generated QR code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Scheme {
    /// Czech SPAYD (SPD) payment string.
    Spd,
    /// EPC069-12 (SEPA) credit transfer payload.
    Epc,
}

impl From<Scheme> for PaymentScheme {
    fn from(scheme: Scheme) -> Self {
        match scheme {
            Scheme::Spd => PaymentScheme::Spd,
            Scheme::Epc => PaymentScheme::Epc,
        }
    }
}

/// JSON report for a validated SPD string with the resolved Czech bank fields.
#[derive(Serialize)]
struct ValidationReport<'a> {
    #[serde(flatten)]
    payment: &'a Payment,
    /// Four-digit Czech bank code, or `null` when the IBAN is not Czech.
    bank_code: Option<&'a str>,
    /// Bank name, or `null` when the code is unknown or the IBAN is not Czech.
    bank_name: Option<&'a str>,
}

/// Generate Czech QR payment (SPAYD/SPD) strings and QR images.
#[derive(Debug, Parser)]
#[command(name = "qrplatba", version, about, arg_required_else_help = true)]
struct Cli {
    /// IBAN of the receiving account.
    #[arg(long, required_unless_present_any = ["validate", "batch"])]
    iban: Option<String>,

    /// Amount in major units, for example 1250.00 or 1250,00.
    #[arg(long, required_unless_present_any = ["validate", "batch"])]
    amount: Option<String>,

    /// ISO 4217 currency code. The default is CZK for SPD and EUR for EPC.
    #[arg(long)]
    currency: Option<String>,

    /// Variable symbol, 1 to 10 digits.
    #[arg(long)]
    vs: Option<String>,

    /// Constant symbol, 1 to 4 digits.
    #[arg(long)]
    ks: Option<String>,

    /// Specific symbol, 1 to 4 digits.
    #[arg(long)]
    ss: Option<String>,

    /// Remittance message. The SPD limit is 60 characters, the EPC limit is 140.
    #[arg(long)]
    message: Option<String>,

    /// Due date in YYYY-MM-DD form.
    #[arg(long)]
    due: Option<String>,

    /// Payment scheme for the generated output.
    #[arg(long, value_enum, default_value = "spd")]
    scheme: Scheme,

    /// Beneficiary name. Required for the EPC scheme.
    #[arg(long)]
    name: Option<String>,

    /// QR image format.
    #[arg(long, value_enum, default_value = "png")]
    format: OutputFormat,

    /// Write output to this path. In batch mode this is the output directory.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Pixels per QR module.
    #[arg(long, default_value_t = DEFAULT_SCALE)]
    scale: u32,

    /// Print the payment payload string instead of a QR image.
    #[arg(long)]
    spd_only: bool,

    /// Validate an SPD string and print a normalized summary.
    #[arg(long, value_name = "STRING")]
    validate: Option<String>,

    /// Print the validation result as JSON.
    #[arg(long, requires = "validate")]
    json: bool,

    /// Generate one QR image per row of this CSV file.
    #[arg(long, value_name = "INPUT.CSV")]
    batch: Option<PathBuf>,

    /// Also write the batch files and index.json into this ZIP archive.
    #[arg(long, value_name = "FILE.ZIP", requires = "batch")]
    zip: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("error: {error:#}");
        std::process::exit(2);
    }
}

fn run(cli: Cli) -> Result<()> {
    if let Some(input) = cli.validate.as_deref() {
        return validate(input, cli.json);
    }
    if let Some(input) = cli.batch.as_deref() {
        return run_batch(&cli, input);
    }
    generate(&cli)
}

fn validate(input: &str, json: bool) -> Result<()> {
    let payment = parse_spd(input)?;
    let bank = bank::czech_bank(&payment.iban);
    if json {
        let report = ValidationReport {
            payment: &payment,
            bank_code: bank.as_ref().map(|bank| bank.code.as_str()),
            bank_name: bank.as_ref().and_then(|bank| bank.name),
        };
        let text = serde_json::to_string_pretty(&report)
            .context("cannot serialize the validation result")?;
        println!("{text}");
    } else {
        println!("{}", payment.summary()?);
    }
    Ok(())
}

fn build_payment(cli: &Cli) -> Result<Payment> {
    let iban = cli.iban.as_deref().context("--iban is required")?;
    let amount = parse_amount(cli.amount.as_deref().context("--amount is required")?)?;
    let currency = cli.currency.clone().unwrap_or_else(|| match cli.scheme {
        Scheme::Spd => "CZK".to_string(),
        Scheme::Epc => "EUR".to_string(),
    });
    let mut payment = Payment::new(iban, amount).with_currency(currency);
    if let Some(vs) = &cli.vs {
        payment = payment.with_variable_symbol(vs.clone());
    }
    if let Some(ks) = &cli.ks {
        payment = payment.with_constant_symbol(ks.clone());
    }
    if let Some(ss) = &cli.ss {
        payment = payment.with_specific_symbol(ss.clone());
    }
    if let Some(message) = &cli.message {
        payment = payment.with_message(message.clone());
    }
    if let Some(due) = &cli.due {
        payment = payment.with_due_date(due.clone());
    }
    Ok(payment)
}

fn generate(cli: &Cli) -> Result<()> {
    let payment = build_payment(cli)?;
    let bytes = match cli.scheme {
        Scheme::Spd => {
            if cli.spd_only {
                return write_text(&payment.to_spd()?, cli.output.as_deref());
            }
            match cli.format {
                OutputFormat::Png => payment.to_qr_png(cli.scale)?,
                OutputFormat::Svg => payment.to_qr_svg(cli.scale)?.into_bytes(),
            }
        }
        Scheme::Epc => {
            let name = cli
                .name
                .as_deref()
                .context("--name is required for --scheme epc")?;
            let epc = EpcPayment::new(&payment, name);
            if cli.spd_only {
                return write_text(&epc.to_payload()?, cli.output.as_deref());
            }
            match cli.format {
                OutputFormat::Png => epc.to_qr_png(cli.scale)?,
                OutputFormat::Svg => epc.to_qr_svg(cli.scale)?.into_bytes(),
            }
        }
    };
    write_output(&bytes, cli.output.as_deref())
}

fn run_batch(cli: &Cli, input: &Path) -> Result<()> {
    let output_dir = cli
        .output
        .as_deref()
        .context("--batch requires --output <DIR>")?;
    let file = File::open(input).with_context(|| format!("cannot open '{}'", input.display()))?;
    let entries = process_batch(
        file,
        output_dir,
        cli.scheme.into(),
        cli.format.into(),
        cli.scale,
    )?;
    if let Some(zip_path) = cli.zip.as_deref() {
        zip_batch(zip_path, output_dir, &entries)?;
        println!("batch archive written to '{}'", zip_path.display());
    }
    let generated = entries.iter().filter(|entry| entry.file.is_some()).count();
    let failed = entries.len() - generated;
    for entry in &entries {
        if let Some(error) = &entry.error {
            eprintln!("row {}: {error}", entry.row);
        }
    }
    println!(
        "batch: {generated} generated, {failed} failed, index written to '{}'",
        output_dir.join("index.json").display()
    );
    Ok(())
}

fn write_text(text: &str, output: Option<&Path>) -> Result<()> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(b'\n');
    write_output(&bytes, output)
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
