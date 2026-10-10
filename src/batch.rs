//! Batch generation of payment QR codes from CSV data.
//!
//! The module reads a CSV table, generates one QR image per row, and writes an
//! `index.json` report into the output directory. A bad row is recorded in the
//! report and does not stop the remaining rows. [`zip_batch`] packages the
//! generated files and the report into a ZIP archive.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{EpcPayment, Error, Payment, parse_amount};

/// Payment scheme used for batch generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentScheme {
    /// Czech SPAYD (SPD) payment string.
    Spd,
    /// EPC069-12 (SEPA) credit transfer payload.
    Epc,
}

/// Output image format for batch generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrFormat {
    /// Portable Network Graphics image.
    Png,
    /// Scalable Vector Graphics image.
    Svg,
}

impl QrFormat {
    fn extension(self) -> &'static str {
        match self {
            QrFormat::Png => "png",
            QrFormat::Svg => "svg",
        }
    }
}

impl PaymentScheme {
    fn default_currency(self) -> &'static str {
        match self {
            PaymentScheme::Spd => "CZK",
            PaymentScheme::Epc => "EUR",
        }
    }
}

/// One generated or failed batch row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchEntry {
    /// 1-based data row number.
    pub row: usize,
    /// File name relative to the output directory, when the row succeeded.
    pub file: Option<String>,
    /// Error message, when the row failed.
    pub error: Option<String>,
}

struct Columns {
    iban: usize,
    amount: usize,
    currency: Option<usize>,
    vs: Option<usize>,
    ks: Option<usize>,
    ss: Option<usize>,
    message: Option<usize>,
    due: Option<usize>,
    name: Option<usize>,
}

/// Processes CSV data and writes one QR image per row into `output_dir`.
///
/// The CSV requires a header with the `iban` and `amount` columns. The columns
/// `currency`, `vs`, `ks`, `ss`, `message`, `due` and `name` are optional; the
/// `name` column is required for [`PaymentScheme::Epc`]. A missing currency
/// defaults to `CZK` for SPD and `EUR` for EPC.
///
/// The function creates the output directory, writes `index.json`, and returns
/// one [`BatchEntry`] per data row. Failed rows carry an error and no file.
pub fn process_batch(
    input: impl Read,
    output_dir: &Path,
    scheme: PaymentScheme,
    format: QrFormat,
    scale: u32,
) -> Result<Vec<BatchEntry>, Error> {
    fs::create_dir_all(output_dir).map_err(io_error)?;
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(input);
    let headers = reader.headers().map_err(csv_error)?.clone();
    let find = |name: &str| headers.iter().position(|header| header == name);
    let columns = Columns {
        iban: find("iban").ok_or(Error::MissingCsvColumn { column: "iban" })?,
        amount: find("amount").ok_or(Error::MissingCsvColumn { column: "amount" })?,
        currency: find("currency"),
        vs: find("vs"),
        ks: find("ks"),
        ss: find("ss"),
        message: find("message"),
        due: find("due"),
        name: find("name"),
    };
    let mut entries = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let row = index + 1;
        match record {
            Ok(record) => {
                match generate_row(&record, &columns, row, output_dir, scheme, format, scale) {
                    Ok(file) => entries.push(BatchEntry {
                        row,
                        file: Some(file),
                        error: None,
                    }),
                    Err(error) => entries.push(BatchEntry {
                        row,
                        file: None,
                        error: Some(error.to_string()),
                    }),
                }
            }
            Err(error) => entries.push(BatchEntry {
                row,
                file: None,
                error: Some(csv_error(error).to_string()),
            }),
        }
    }
    let json = serde_json::to_string_pretty(&entries).map_err(|error| Error::JsonEncoding {
        message: error.to_string(),
    })?;
    fs::write(output_dir.join("index.json"), format!("{json}\n")).map_err(io_error)?;
    Ok(entries)
}

/// Writes `index.json` and every generated batch file into a ZIP archive.
///
/// Files are stored without compression, so the archive is deterministic.
pub fn zip_batch(zip_path: &Path, output_dir: &Path, entries: &[BatchEntry]) -> Result<(), Error> {
    let file = fs::File::create(zip_path).map_err(io_error)?;
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let json = serde_json::to_string_pretty(entries).map_err(|error| Error::JsonEncoding {
        message: error.to_string(),
    })?;
    writer
        .start_file("index.json", options)
        .map_err(zip_error)?;
    writer
        .write_all(format!("{json}\n").as_bytes())
        .map_err(io_error)?;
    for entry in entries {
        if let Some(name) = &entry.file {
            let bytes = fs::read(output_dir.join(name)).map_err(io_error)?;
            writer
                .start_file(name.clone(), options)
                .map_err(zip_error)?;
            writer.write_all(&bytes).map_err(io_error)?;
        }
    }
    writer.finish().map_err(zip_error)?;
    Ok(())
}

fn generate_row(
    record: &csv::StringRecord,
    columns: &Columns,
    row: usize,
    output_dir: &Path,
    scheme: PaymentScheme,
    format: QrFormat,
    scale: u32,
) -> Result<String, Error> {
    let amount = parse_amount(cell(record, Some(columns.amount)))?;
    let currency = match non_empty(cell(record, columns.currency)) {
        Some(currency) => currency,
        None => scheme.default_currency(),
    };
    let mut payment =
        Payment::new(cell(record, Some(columns.iban)), amount).with_currency(currency);
    if let Some(value) = non_empty(cell(record, columns.vs)) {
        payment = payment.with_variable_symbol(value);
    }
    if let Some(value) = non_empty(cell(record, columns.ks)) {
        payment = payment.with_constant_symbol(value);
    }
    if let Some(value) = non_empty(cell(record, columns.ss)) {
        payment = payment.with_specific_symbol(value);
    }
    if let Some(value) = non_empty(cell(record, columns.message)) {
        payment = payment.with_message(value);
    }
    if let Some(value) = non_empty(cell(record, columns.due)) {
        payment = payment.with_due_date(value);
    }
    let bytes = match scheme {
        PaymentScheme::Spd => render(&payment, format, scale)?,
        PaymentScheme::Epc => {
            let name = non_empty(cell(record, columns.name))
                .ok_or(Error::EpcTextEmpty {
                    name: "beneficiary name",
                })?
                .to_string();
            let epc = EpcPayment::new(&payment, name);
            render_epc(&epc, format, scale)?
        }
    };
    let file = format!(
        "{row}-{}.{}",
        filename_token(cell(record, columns.vs), row),
        format.extension()
    );
    fs::write(output_dir.join(&file), bytes).map_err(io_error)?;
    Ok(file)
}

fn render(payment: &Payment, format: QrFormat, scale: u32) -> Result<Vec<u8>, Error> {
    match format {
        QrFormat::Png => payment.to_qr_png(scale),
        QrFormat::Svg => payment.to_qr_svg(scale).map(String::into_bytes),
    }
}

fn render_epc(payment: &EpcPayment<'_>, format: QrFormat, scale: u32) -> Result<Vec<u8>, Error> {
    match format {
        QrFormat::Png => payment.to_qr_png(scale),
        QrFormat::Svg => payment.to_qr_svg(scale).map(String::into_bytes),
    }
}

fn cell(record: &csv::StringRecord, index: Option<usize>) -> &str {
    index.and_then(|index| record.get(index)).unwrap_or("")
}

fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn filename_token(value: &str, row: usize) -> String {
    let trimmed = value.trim();
    let valid = !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if valid {
        trimmed.to_string()
    } else {
        row.to_string()
    }
}

fn io_error(error: std::io::Error) -> Error {
    Error::Io {
        message: error.to_string(),
    }
}

fn csv_error(error: csv::Error) -> Error {
    Error::Csv {
        message: error.to_string(),
    }
}

fn zip_error(error: zip::result::ZipError) -> Error {
    Error::Zip {
        message: error.to_string(),
    }
}
