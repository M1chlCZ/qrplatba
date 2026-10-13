//! Czech QR payment (SPAYD/SPD) string and QR image generator.
//!
//! The crate builds deterministic SPAYD ("Short Payment Descriptor") strings for
//! Czech QR payments and renders them as PNG or SVG QR codes. It also builds
//! EPC069-12 (SEPA) payloads, validates complete SPD strings, resolves Czech
//! bank codes, and generates one QR image per CSV row in batch mode. All
//! processing happens offline; the crate performs no network access.
//!
//! # Example
//!
//! ```
//! use qrplatba::Payment;
//!
//! let payment = Payment::new("CZ6508000000192000145399", 125_000)
//!     .with_variable_symbol("1234567890")
//!     .with_message("Faktura 2026-001");
//! let spd = payment.to_spd().unwrap();
//! assert_eq!(
//!     spd,
//!     "SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK*MSG:Faktura 2026-001*X-VS:1234567890"
//! );
//! ```

pub mod bank;
pub mod batch;

use std::fmt;
use std::fmt::Write as _;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, Luma};
use qrcode::bits::Bits;
use qrcode::{Color as ModuleColor, EcLevel, QrCode, Version};
use serde::{Deserialize, Serialize};

/// Default number of pixels per QR module used by the command line interface.
pub const DEFAULT_SCALE: u32 = 8;

/// Width of the quiet zone around the QR matrix, in modules.
pub const QUIET_ZONE: u32 = 4;

/// Maximum message length in characters after ASCII folding.
pub const MAX_MESSAGE_CHARS: usize = 60;

/// Maximum beneficiary name length in an EPC069-12 payload, in characters.
pub const MAX_EPC_BENEFICIARY_CHARS: usize = 70;

/// Maximum remittance length in an EPC069-12 payload, in characters.
pub const MAX_EPC_REMITTANCE_CHARS: usize = 140;

/// Maximum information length in an EPC069-12 payload, in characters.
pub const MAX_EPC_INFORMATION_CHARS: usize = 70;

/// Maximum EPC amount in minor units, `999999999.99` EUR.
pub const MAX_EPC_AMOUNT_MINOR: i64 = 99_999_999_999;

/// A single Czech QR payment.
///
/// Amounts are stored in minor units. For example, `125_000` means 1250.00 CZK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    /// IBAN of the receiving account. Spaces and letter case are ignored.
    pub iban: String,
    /// Amount in minor units, for example haléře. Must be greater than zero.
    pub amount_minor: i64,
    /// ISO 4217 currency code. Defaults to `CZK`.
    #[serde(default = "default_currency")]
    pub currency: String,
    /// Variable symbol, 1 to 10 digits.
    pub variable_symbol: Option<String>,
    /// Constant symbol, 1 to 4 digits.
    pub constant_symbol: Option<String>,
    /// Specific symbol, 1 to 4 digits.
    pub specific_symbol: Option<String>,
    /// Remittance message, at most 60 characters after ASCII folding.
    pub message: Option<String>,
    /// Due date in `YYYY-MM-DD` form.
    pub due_date: Option<String>,
}

fn default_currency() -> String {
    "CZK".to_string()
}

impl Payment {
    /// Creates a payment for `iban` with `amount_minor` minor units and currency `CZK`.
    pub fn new(iban: impl Into<String>, amount_minor: i64) -> Self {
        Self {
            iban: iban.into(),
            amount_minor,
            currency: default_currency(),
            variable_symbol: None,
            constant_symbol: None,
            specific_symbol: None,
            message: None,
            due_date: None,
        }
    }

    /// Sets the ISO 4217 currency code.
    pub fn with_currency(mut self, currency: impl Into<String>) -> Self {
        self.currency = currency.into();
        self
    }

    /// Sets the variable symbol.
    pub fn with_variable_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.variable_symbol = Some(symbol.into());
        self
    }

    /// Sets the constant symbol.
    pub fn with_constant_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.constant_symbol = Some(symbol.into());
        self
    }

    /// Sets the specific symbol.
    pub fn with_specific_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.specific_symbol = Some(symbol.into());
        self
    }

    /// Sets the remittance message.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Sets the due date in `YYYY-MM-DD` form.
    pub fn with_due_date(mut self, due_date: impl Into<String>) -> Self {
        self.due_date = Some(due_date.into());
        self
    }

    /// Builds the SPD string in the fixed field order ACC, AM, CC, DT, MSG, X-VS, X-KS, X-SS.
    pub fn to_spd(&self) -> Result<String, Error> {
        let iban = normalize_iban(&self.iban)?;
        let amount = format_amount(self.amount_minor)?;
        let currency = normalize_currency(&self.currency)?;
        let mut fields: Vec<String> = Vec::with_capacity(8);
        fields.push("SPD*1.0".to_string());
        fields.push(format!("ACC:{iban}"));
        fields.push(format!("AM:{amount}"));
        fields.push(format!("CC:{currency}"));
        if let Some(due_date) = self
            .due_date
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            fields.push(format!("DT:{}", format_due_date(due_date)?));
        }
        if let Some(message) = self.message.as_deref() {
            let folded = fold_ascii(message);
            if !folded.is_empty() {
                let length = folded.chars().count();
                if length > MAX_MESSAGE_CHARS {
                    return Err(Error::MessageTooLong {
                        length,
                        max_length: MAX_MESSAGE_CHARS,
                    });
                }
                fields.push(format!("MSG:{}", escape_ascii(&folded)));
            }
        }
        if let Some(symbol) = &self.variable_symbol {
            fields.push(format!(
                "X-VS:{}",
                normalize_symbol("variable symbol", symbol, 10)?
            ));
        }
        if let Some(symbol) = &self.constant_symbol {
            fields.push(format!(
                "X-KS:{}",
                normalize_symbol("constant symbol", symbol, 4)?
            ));
        }
        if let Some(symbol) = &self.specific_symbol {
            fields.push(format!(
                "X-SS:{}",
                normalize_symbol("specific symbol", symbol, 4)?
            ));
        }
        Ok(fields.join("*"))
    }

    /// Renders the payment as a PNG QR code with `scale` pixels per module.
    pub fn to_qr_png(&self, scale: u32) -> Result<Vec<u8>, Error> {
        let spd = self.to_spd()?;
        encode_qr_png(spd.as_bytes(), scale)
    }

    /// Renders the payment as an SVG QR code with `scale` pixels per module.
    pub fn to_qr_svg(&self, scale: u32) -> Result<String, Error> {
        let spd = self.to_spd()?;
        encode_qr_svg(spd.as_bytes(), scale)
    }

    /// Returns a normalized human readable summary of the payment fields.
    ///
    /// Each field appears on its own line. The summary omits empty optional
    /// fields and prints the amount in major units with two decimal places.
    /// For a Czech IBAN it adds the bank code from positions 5 to 8 and the
    /// bank name when the offline table knows the code; unknown codes print
    /// `unknown`.
    pub fn summary(&self) -> Result<String, Error> {
        let iban = normalize_iban(&self.iban)?;
        let amount = format_amount(self.amount_minor)?;
        let currency = normalize_currency(&self.currency)?;
        let mut lines = vec![format!("IBAN: {iban}")];
        if let Some(bank) = bank::czech_bank(&iban) {
            lines.push(format!("Bank code: {}", bank.code));
            lines.push(format!("Bank: {}", bank.name.unwrap_or("unknown")));
        }
        lines.push(format!("Amount: {amount} {currency}"));
        if let Some(due_date) = non_empty(self.due_date.as_deref()) {
            format_due_date(due_date)?;
            lines.push(format!("Due date: {due_date}"));
        }
        if let Some(message) = non_empty(self.message.as_deref()) {
            lines.push(format!("Message: {message}"));
        }
        let symbols = [
            ("Variable symbol", self.variable_symbol.as_deref()),
            ("Constant symbol", self.constant_symbol.as_deref()),
            ("Specific symbol", self.specific_symbol.as_deref()),
        ];
        for (label, symbol) in symbols {
            if let Some(symbol) = non_empty(symbol) {
                lines.push(format!("{label}: {symbol}"));
            }
        }
        Ok(lines.join("\n"))
    }
}

/// Builder for an EPC069-12 (SEPA) credit transfer payload.
///
/// The payload is Euro only and uses the UTF-8 character set. The builder
/// requires the beneficiary name. The BIC and the unstructured information
/// line are optional. The builder writes the payment message on the
/// unstructured remittance line.
///
/// # Example
///
/// ```
/// use qrplatba::{EpcPayment, Payment};
///
/// let payment = Payment::new("CZ6508000000192000145399", 125_000)
///     .with_currency("EUR")
///     .with_message("Faktura 2026-001");
/// let payload = EpcPayment::new(&payment, "Firma s.r.o.")
///     .with_information("Objednavka 42")
///     .to_payload()
///     .unwrap();
/// assert!(payload.starts_with("BCD\n002\n1\nSCT\n"));
/// ```
#[derive(Debug, Clone)]
pub struct EpcPayment<'a> {
    payment: &'a Payment,
    beneficiary: String,
    bic: Option<String>,
    information: Option<String>,
}

impl<'a> EpcPayment<'a> {
    /// Creates an EPC payload builder for `payment` payable to `beneficiary`.
    pub fn new(payment: &'a Payment, beneficiary: impl Into<String>) -> Self {
        Self {
            payment,
            beneficiary: beneficiary.into(),
            bic: None,
            information: None,
        }
    }

    /// Sets the optional BIC of the beneficiary bank, 8 or 11 characters.
    pub fn with_bic(mut self, bic: impl Into<String>) -> Self {
        self.bic = Some(bic.into());
        self
    }

    /// Sets the optional unstructured information line.
    pub fn with_information(mut self, information: impl Into<String>) -> Self {
        self.information = Some(information.into());
        self
    }

    /// Builds the EPC069-12 payload with `\n` between the lines.
    ///
    /// The payload rejects currencies other than EUR, amounts outside the range
    /// 0.01 to 999999999.99, text over the specification limits, and control
    /// characters.
    pub fn to_payload(&self) -> Result<String, Error> {
        let currency = normalize_currency(&self.payment.currency)?;
        if currency != "EUR" {
            return Err(Error::UnsupportedEpcCurrency { input: currency });
        }
        if self.payment.amount_minor > MAX_EPC_AMOUNT_MINOR {
            return Err(Error::EpcAmountOutOfRange {
                amount_minor: self.payment.amount_minor,
            });
        }
        let iban = normalize_iban(&self.payment.iban)?;
        let amount = format_amount(self.payment.amount_minor)?;
        let beneficiary = validate_epc_text(
            "beneficiary name",
            &self.beneficiary,
            MAX_EPC_BENEFICIARY_CHARS,
        )?;
        let mut lines = vec![
            "BCD".to_string(),
            "002".to_string(),
            "1".to_string(),
            "SCT".to_string(),
        ];
        if let Some(bic) = self.bic.as_deref().filter(|value| !value.trim().is_empty()) {
            lines.push(normalize_bic(bic)?);
        }
        lines.push(beneficiary);
        lines.push(iban);
        lines.push(format!("EUR{amount}"));
        if let Some(message) = non_empty(self.payment.message.as_deref()) {
            lines.push(validate_epc_text(
                "remittance",
                message,
                MAX_EPC_REMITTANCE_CHARS,
            )?);
        }
        if let Some(information) = non_empty(self.information.as_deref()) {
            lines.push(validate_epc_text(
                "information",
                information,
                MAX_EPC_INFORMATION_CHARS,
            )?);
        }
        Ok(lines.join("\n"))
    }

    /// Renders the EPC payload as a PNG QR code with `scale` pixels per module.
    pub fn to_qr_png(&self, scale: u32) -> Result<Vec<u8>, Error> {
        let payload = self.to_payload()?;
        encode_qr_png(payload.as_bytes(), scale)
    }

    /// Renders the EPC payload as an SVG QR code with `scale` pixels per module.
    pub fn to_qr_svg(&self, scale: u32) -> Result<String, Error> {
        let payload = self.to_payload()?;
        encode_qr_svg(payload.as_bytes(), scale)
    }
}

/// Builds an EPC069-12 payload for `payment` payable to `beneficiary`.
///
/// This is a shortcut for [`EpcPayment::new`] followed by
/// [`EpcPayment::to_payload`].
pub fn epc_string(payment: &Payment, beneficiary: &str) -> Result<String, Error> {
    EpcPayment::new(payment, beneficiary).to_payload()
}

fn validate_epc_text(name: &'static str, input: &str, max_length: usize) -> Result<String, Error> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(Error::EpcTextEmpty { name });
    }
    if let Some(character) = trimmed.chars().find(|character| character.is_control()) {
        return Err(Error::ControlCharacter { character });
    }
    let length = trimmed.chars().count();
    if length > max_length {
        return Err(Error::EpcTextTooLong {
            name,
            length,
            max_length,
        });
    }
    Ok(trimmed.to_string())
}

fn normalize_bic(input: &str) -> Result<String, Error> {
    let trimmed = input.trim();
    let valid_length = trimmed.len() == 8 || trimmed.len() == 11;
    if !valid_length || !trimmed.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(Error::InvalidBic {
            input: input.to_string(),
        });
    }
    Ok(trimmed.to_ascii_uppercase())
}

fn encode_qr_png(data: &[u8], scale: u32) -> Result<Vec<u8>, Error> {
    let code = encode_byte_mode(data)?;
    let modules = code.width() as u32;
    let side = pixel_side(modules, scale)?;
    let mut image = image::GrayImage::from_pixel(side, side, Luma([255u8]));
    for (index, color) in code.to_colors().iter().enumerate() {
        if *color == ModuleColor::Dark {
            let index = index as u32;
            let x = (QUIET_ZONE + index % modules) * scale;
            let y = (QUIET_ZONE + index / modules) * scale;
            for pixel_y in y..y + scale {
                for pixel_x in x..x + scale {
                    image.put_pixel(pixel_x, pixel_y, Luma([0u8]));
                }
            }
        }
    }
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(image.as_raw(), side, side, ExtendedColorType::L8)
        .map_err(|error| Error::PngEncoding {
            message: error.to_string(),
        })?;
    Ok(bytes)
}

fn encode_qr_svg(data: &[u8], scale: u32) -> Result<String, Error> {
    let code = encode_byte_mode(data)?;
    let modules = code.width() as u32;
    let side = pixel_side(modules, scale)?;
    let total = modules + 2 * QUIET_ZONE;
    let mut path = String::new();
    for (index, color) in code.to_colors().iter().enumerate() {
        if *color == ModuleColor::Dark {
            let index = index as u32;
            let x = QUIET_ZONE + index % modules;
            let y = QUIET_ZONE + index / modules;
            let _ = write!(path, "M{x} {y}h1v1h-1z");
        }
    }
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{side}\" height=\"{side}\" viewBox=\"0 0 {total} {total}\" shape-rendering=\"crispEdges\"><rect width=\"100%\" height=\"100%\" fill=\"#FFFFFF\"/><path d=\"{path}\" fill=\"#000000\"/></svg>"
    ))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn encode_byte_mode(data: &[u8]) -> Result<QrCode, Error> {
    for version in 1..=40u8 {
        let mut bits = Bits::new(Version::Normal(i16::from(version)));
        if bits.push_byte_data(data).is_err() {
            continue;
        }
        if bits.push_terminator(EcLevel::M).is_err() {
            continue;
        }
        return QrCode::with_bits(bits, EcLevel::M).map_err(|error| Error::QrEncoding {
            message: error.to_string(),
        });
    }
    Err(Error::QrEncoding {
        message: "data does not fit into a version 40 QR code with error correction level M"
            .to_string(),
    })
}

fn pixel_side(modules: u32, scale: u32) -> Result<u32, Error> {
    if scale == 0 {
        return Err(Error::InvalidScale { scale });
    }
    (modules + 2 * QUIET_ZONE)
        .checked_mul(scale)
        .ok_or(Error::InvalidScale { scale })
}

fn normalize_symbol(name: &'static str, input: &str, max_length: usize) -> Result<String, Error> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || trimmed.len() > max_length
        || !trimmed.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(Error::InvalidSymbol {
            name,
            input: input.to_string(),
            max_length,
        });
    }
    Ok(trimmed.to_string())
}

fn format_due_date(input: &str) -> Result<String, Error> {
    let trimmed = input.trim();
    let invalid = || Error::InvalidDueDate {
        input: input.to_string(),
    };
    let bytes = trimmed.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(invalid());
    }
    let parts = [&bytes[0..4], &bytes[5..7], &bytes[8..10]];
    if !parts.iter().all(|part| part.iter().all(u8::is_ascii_digit)) {
        return Err(invalid());
    }
    let year: u32 = trimmed[0..4].parse().map_err(|_| invalid())?;
    let month: u32 = trimmed[5..7].parse().map_err(|_| invalid())?;
    let day: u32 = trimmed[8..10].parse().map_err(|_| invalid())?;
    if year == 0 || !(1..=12).contains(&month) {
        return Err(invalid());
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    };
    if day == 0 || day > max_day {
        return Err(invalid());
    }
    Ok(format!("{year:04}{month:02}{day:02}"))
}

/// Errors returned while validating payment data or generating output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The IBAN is empty or contains only whitespace.
    EmptyIban,
    /// The normalized IBAN length is outside the 15 to 34 range.
    IbanLength {
        /// Normalized length in characters.
        length: usize,
    },
    /// The IBAN contains a character that is not ASCII alphanumeric.
    IbanCharacter {
        /// The offending character.
        character: char,
    },
    /// The IBAN checksum fails the MOD-97 test.
    IbanChecksum,
    /// The amount string is not a decimal number with at most two fraction digits.
    InvalidAmountFormat {
        /// The rejected input.
        input: String,
    },
    /// The amount is zero or negative.
    AmountNotPositive,
    /// The amount does not fit into `i64` minor units.
    AmountOverflow {
        /// The rejected input.
        input: String,
    },
    /// The currency is not a three-letter code.
    InvalidCurrency {
        /// The rejected input.
        input: String,
    },
    /// A payment symbol is empty, too long, or contains a non-digit character.
    InvalidSymbol {
        /// Human readable symbol name.
        name: &'static str,
        /// The rejected input.
        input: String,
        /// Maximum accepted number of digits.
        max_length: usize,
    },
    /// The message exceeds [`MAX_MESSAGE_CHARS`] characters after ASCII folding.
    MessageTooLong {
        /// Length in characters after folding.
        length: usize,
        /// Maximum accepted length.
        max_length: usize,
    },
    /// The due date is not a real date in `YYYY-MM-DD` form.
    InvalidDueDate {
        /// The rejected input.
        input: String,
    },
    /// The scale is zero or overflows the image size.
    InvalidScale {
        /// The rejected scale.
        scale: u32,
    },
    /// The SPD string cannot be encoded as a QR code.
    QrEncoding {
        /// Encoder message.
        message: String,
    },
    /// The PNG image cannot be encoded.
    PngEncoding {
        /// Encoder message.
        message: String,
    },
    /// The EPC payload currency is not EUR.
    UnsupportedEpcCurrency {
        /// The rejected currency.
        input: String,
    },
    /// A required EPC text field is empty.
    EpcTextEmpty {
        /// Human readable field name.
        name: &'static str,
    },
    /// An EPC text field exceeds its specification limit.
    EpcTextTooLong {
        /// Human readable field name.
        name: &'static str,
        /// Length in characters.
        length: usize,
        /// Maximum accepted length.
        max_length: usize,
    },
    /// The EPC amount is outside the range 0.01 to 999999999.99 EUR.
    EpcAmountOutOfRange {
        /// The rejected amount in minor units.
        amount_minor: i64,
    },
    /// The BIC is not 8 or 11 alphanumeric characters.
    InvalidBic {
        /// The rejected input.
        input: String,
    },
    /// The input contains a control character.
    ControlCharacter {
        /// The offending character.
        character: char,
    },
    /// The string does not start with the supported `SPD*1.0` header.
    InvalidSpdHeader {
        /// The rejected header.
        input: String,
    },
    /// An SPD field does not have the `NAME:VALUE` form.
    MalformedSpdField {
        /// The rejected field.
        field: String,
    },
    /// The SPD string contains an unsupported field.
    UnknownSpdField {
        /// The field name.
        field: String,
    },
    /// The SPD string contains the same field more than once.
    DuplicateSpdField {
        /// The field name.
        field: &'static str,
    },
    /// The SPD string lacks a required field.
    MissingSpdField {
        /// The field name.
        field: &'static str,
    },
    /// A percent escape is incomplete, not hexadecimal, or not valid UTF-8.
    InvalidPercentEscape {
        /// The rejected field value.
        input: String,
    },
    /// A file cannot be read or written.
    Io {
        /// Operating system message.
        message: String,
    },
    /// CSV data cannot be read.
    Csv {
        /// CSV parser message.
        message: String,
    },
    /// The CSV header lacks a required column.
    MissingCsvColumn {
        /// The column name.
        column: &'static str,
    },
    /// A value cannot be serialized to JSON.
    JsonEncoding {
        /// Encoder message.
        message: String,
    },
    /// A ZIP archive cannot be written.
    Zip {
        /// ZIP encoder message.
        message: String,
    },
}

/// Normalizes and validates an IBAN.
///
/// Removes ASCII whitespace, uppercases the input, checks the length range 15
/// to 34, the character set, and the MOD-97 checksum. Returns the normalized
/// IBAN.
pub fn normalize_iban(input: &str) -> Result<String, Error> {
    let normalized: String = input
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    if normalized.is_empty() {
        return Err(Error::EmptyIban);
    }
    if let Some(character) = normalized
        .chars()
        .find(|character| !character.is_ascii_alphanumeric())
    {
        return Err(Error::IbanCharacter { character });
    }
    let length = normalized.len();
    if !(15..=34).contains(&length) {
        return Err(Error::IbanLength { length });
    }
    let rearranged = format!("{}{}", &normalized[4..], &normalized[..4]);
    let mut remainder = 0u32;
    for character in rearranged.chars() {
        let value = if character.is_ascii_digit() {
            u32::from(character as u8 - b'0')
        } else {
            u32::from(character as u8 - b'A' + 10)
        };
        let base = if value >= 10 { 100 } else { 10 };
        remainder = (remainder * base + value) % 97;
    }
    if remainder != 1 {
        return Err(Error::IbanChecksum);
    }
    Ok(normalized)
}

/// Normalizes and validates an ISO 4217 currency code.
///
/// Returns the code in uppercase; the input must contain exactly three ASCII
/// letters after trimming.
pub fn normalize_currency(input: &str) -> Result<String, Error> {
    let trimmed = input.trim();
    if trimmed.len() != 3
        || !trimmed
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err(Error::InvalidCurrency {
            input: input.to_string(),
        });
    }
    Ok(trimmed.to_ascii_uppercase())
}

/// Parses and validates a complete SPD string, returning the payment it describes.
///
/// The input must start with `SPD*1.0` and contain the `ACC`, `AM` and `CC`
/// fields. The optional fields are `DT`, `MSG`, `X-VS`, `X-KS` and `X-SS`.
/// Field order is free. The parser rejects malformed, duplicate and unknown
/// fields. It decodes and validates percent escapes and rejects control
/// characters.
pub fn parse_spd(input: &str) -> Result<Payment, Error> {
    let trimmed = input.trim();
    if let Some(character) = trimmed.chars().find(|character| character.is_control()) {
        return Err(Error::ControlCharacter { character });
    }
    let mut parts = trimmed.split('*');
    let header = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if header != "SPD" || version != "1.0" {
        return Err(Error::InvalidSpdHeader {
            input: trimmed.to_string(),
        });
    }
    let mut acc: Option<String> = None;
    let mut am: Option<String> = None;
    let mut cc: Option<String> = None;
    let mut dt: Option<String> = None;
    let mut msg: Option<String> = None;
    let mut vs: Option<String> = None;
    let mut ks: Option<String> = None;
    let mut ss: Option<String> = None;
    for part in parts {
        let (field, value) = part
            .split_once(':')
            .ok_or_else(|| Error::MalformedSpdField {
                field: part.to_string(),
            })?;
        let decoded = unescape_spd(value)?;
        match field {
            "ACC" => set_spd_field(&mut acc, "ACC", decoded)?,
            "AM" => set_spd_field(&mut am, "AM", decoded)?,
            "CC" => set_spd_field(&mut cc, "CC", decoded)?,
            "DT" => set_spd_field(&mut dt, "DT", decoded)?,
            "MSG" => set_spd_field(&mut msg, "MSG", decoded)?,
            "X-VS" => set_spd_field(&mut vs, "X-VS", decoded)?,
            "X-KS" => set_spd_field(&mut ks, "X-KS", decoded)?,
            "X-SS" => set_spd_field(&mut ss, "X-SS", decoded)?,
            other => {
                return Err(Error::UnknownSpdField {
                    field: other.to_string(),
                });
            }
        }
    }
    let iban = normalize_iban(&take_spd_field(acc, "ACC")?)?;
    let amount_minor = parse_amount(&take_spd_field(am, "AM")?)?;
    let currency = normalize_currency(&take_spd_field(cc, "CC")?)?;
    let due_date = match dt {
        Some(value) => Some(parse_spd_date(&value)?),
        None => None,
    };
    let message = msg.filter(|value| !value.is_empty());
    if let Some(message) = &message {
        let length = message.chars().count();
        if length > MAX_MESSAGE_CHARS {
            return Err(Error::MessageTooLong {
                length,
                max_length: MAX_MESSAGE_CHARS,
            });
        }
    }
    let variable_symbol = match vs {
        Some(value) => Some(normalize_symbol("variable symbol", &value, 10)?),
        None => None,
    };
    let constant_symbol = match ks {
        Some(value) => Some(normalize_symbol("constant symbol", &value, 4)?),
        None => None,
    };
    let specific_symbol = match ss {
        Some(value) => Some(normalize_symbol("specific symbol", &value, 4)?),
        None => None,
    };
    Ok(Payment {
        iban,
        amount_minor,
        currency,
        variable_symbol,
        constant_symbol,
        specific_symbol,
        message,
        due_date,
    })
}

fn set_spd_field(
    slot: &mut Option<String>,
    name: &'static str,
    value: String,
) -> Result<(), Error> {
    if slot.replace(value).is_some() {
        return Err(Error::DuplicateSpdField { field: name });
    }
    Ok(())
}

fn take_spd_field(slot: Option<String>, name: &'static str) -> Result<String, Error> {
    slot.ok_or(Error::MissingSpdField { field: name })
}

fn parse_spd_date(input: &str) -> Result<String, Error> {
    let bytes = input.as_bytes();
    if bytes.len() != 8 || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::InvalidDueDate {
            input: input.to_string(),
        });
    }
    let normalized = format!("{}-{}-{}", &input[0..4], &input[4..6], &input[6..8]);
    format_due_date(&normalized)?;
    Ok(normalized)
}

fn unescape_spd(input: &str) -> Result<String, Error> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(Error::InvalidPercentEscape {
                    input: input.to_string(),
                });
            }
            let high = hex_value(bytes[index + 1]).ok_or_else(|| Error::InvalidPercentEscape {
                input: input.to_string(),
            })?;
            let low = hex_value(bytes[index + 2]).ok_or_else(|| Error::InvalidPercentEscape {
                input: input.to_string(),
            })?;
            output.push(high * 16 + low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(output).map_err(|_| Error::InvalidPercentEscape {
        input: input.to_string(),
    })?;
    if let Some(character) = decoded.chars().find(|character| character.is_control()) {
        return Err(Error::ControlCharacter { character });
    }
    Ok(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Formats minor units as an SPD amount with a dot and exactly two fraction digits.
pub fn format_amount(amount_minor: i64) -> Result<String, Error> {
    if amount_minor <= 0 {
        return Err(Error::AmountNotPositive);
    }
    let major = amount_minor / 100;
    let minor = amount_minor % 100;
    Ok(format!("{major}.{minor:02}"))
}

/// Parses an amount string in major units into minor units.
///
/// Accepts exactly one optional separator, either `.` or `,`, and at most two
/// fraction digits. Rejects signs, zero, and values that do not fit into `i64`
/// minor units.
pub fn parse_amount(input: &str) -> Result<i64, Error> {
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.split(['.', ',']).collect();
    if trimmed.is_empty() || !(1..=2).contains(&parts.len()) {
        return Err(Error::InvalidAmountFormat {
            input: input.to_string(),
        });
    }
    let major = parts[0];
    if major.is_empty() || !major.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::InvalidAmountFormat {
            input: input.to_string(),
        });
    }
    let fraction = if parts.len() == 2 { parts[1] } else { "" };
    if parts.len() == 2
        && (fraction.is_empty()
            || fraction.len() > 2
            || !fraction.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(Error::InvalidAmountFormat {
            input: input.to_string(),
        });
    }
    let major_value: i128 = major.parse().map_err(|_| Error::AmountOverflow {
        input: input.to_string(),
    })?;
    let mut fraction_value = 0i128;
    for byte in fraction.bytes() {
        fraction_value = fraction_value * 10 + i128::from(byte - b'0');
    }
    if fraction.len() == 1 {
        fraction_value *= 10;
    }
    let total = major_value
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction_value))
        .ok_or_else(|| Error::AmountOverflow {
            input: input.to_string(),
        })?;
    if total <= 0 {
        return Err(Error::AmountNotPositive);
    }
    i64::try_from(total).map_err(|_| Error::AmountOverflow {
        input: input.to_string(),
    })
}

/// Folds Czech diacritics to plain ASCII characters.
pub fn fold_ascii(input: &str) -> String {
    input.chars().map(fold_char).collect()
}

fn fold_char(character: char) -> char {
    match character {
        'á' => 'a',
        'č' => 'c',
        'ď' => 'd',
        'é' => 'e',
        'ě' => 'e',
        'í' => 'i',
        'ň' => 'n',
        'ó' => 'o',
        'ř' => 'r',
        'š' => 's',
        'ť' => 't',
        'ú' => 'u',
        'ů' => 'u',
        'ý' => 'y',
        'ž' => 'z',
        'Á' => 'A',
        'Č' => 'C',
        'Ď' => 'D',
        'É' => 'E',
        'Ě' => 'E',
        'Í' => 'I',
        'Ň' => 'N',
        'Ó' => 'O',
        'Ř' => 'R',
        'Š' => 'S',
        'Ť' => 'T',
        'Ú' => 'U',
        'Ů' => 'U',
        'Ý' => 'Y',
        'Ž' => 'Z',
        other => other,
    }
}

/// Folds Czech diacritics and percent-encodes reserved and non-ASCII characters.
///
/// `*` becomes `%2A` and `%` becomes `%25`; other non-ASCII characters are
/// encoded byte by byte in UTF-8 with uppercase hexadecimal digits.
pub fn escape_spd(input: &str) -> String {
    escape_ascii(&fold_ascii(input))
}

fn escape_ascii(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '%' => output.push_str("%25"),
            '*' => output.push_str("%2A"),
            character if character.is_ascii_graphic() || character == ' ' => {
                output.push(character);
            }
            character => {
                let mut buffer = [0u8; 4];
                for byte in character.encode_utf8(&mut buffer).bytes() {
                    let _ = write!(output, "%{byte:02X}");
                }
            }
        }
    }
    output
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EmptyIban => write!(formatter, "IBAN is empty"),
            Error::IbanLength { length } => {
                write!(
                    formatter,
                    "IBAN must have 15 to 34 characters, got {length}"
                )
            }
            Error::IbanCharacter { character } => {
                write!(formatter, "IBAN contains invalid character '{character}'")
            }
            Error::IbanChecksum => write!(formatter, "IBAN checksum failed (MOD-97 != 1)"),
            Error::InvalidAmountFormat { input } => write!(
                formatter,
                "invalid amount '{input}': expected digits with an optional '.' or ',' and at most two decimal places"
            ),
            Error::AmountNotPositive => write!(formatter, "amount must be greater than zero"),
            Error::AmountOverflow { input } => write!(formatter, "amount '{input}' is too large"),
            Error::InvalidCurrency { input } => write!(
                formatter,
                "invalid currency '{input}': expected a three-letter ISO 4217 code"
            ),
            Error::InvalidSymbol {
                name,
                input,
                max_length,
            } => write!(
                formatter,
                "invalid {name} '{input}': expected 1 to {max_length} digits"
            ),
            Error::MessageTooLong { length, max_length } => write!(
                formatter,
                "message has {length} characters after folding, maximum is {max_length}"
            ),
            Error::InvalidDueDate { input } => write!(
                formatter,
                "invalid due date '{input}': expected YYYY-MM-DD with a real calendar date"
            ),
            Error::InvalidScale { scale } => write!(
                formatter,
                "invalid scale {scale}: expected at least 1 without exceeding the maximum image size"
            ),
            Error::QrEncoding { message } => write!(formatter, "QR encoding failed: {message}"),
            Error::PngEncoding { message } => write!(formatter, "PNG encoding failed: {message}"),
            Error::UnsupportedEpcCurrency { input } => {
                write!(formatter, "EPC supports only EUR, got '{input}'")
            }
            Error::EpcTextEmpty { name } => write!(formatter, "EPC {name} is required"),
            Error::EpcTextTooLong {
                name,
                length,
                max_length,
            } => write!(
                formatter,
                "EPC {name} has {length} characters, maximum is {max_length}"
            ),
            Error::EpcAmountOutOfRange { amount_minor } => write!(
                formatter,
                "EPC amount {amount_minor} minor units is outside 0.01 to 999999999.99 EUR"
            ),
            Error::InvalidBic { input } => write!(
                formatter,
                "invalid BIC '{input}': expected 8 or 11 alphanumeric characters"
            ),
            Error::ControlCharacter { character } => {
                write!(formatter, "control character {character:?} is not allowed")
            }
            Error::InvalidSpdHeader { input } => {
                write!(formatter, "invalid SPD header '{input}': expected SPD*1.0")
            }
            Error::MalformedSpdField { field } => write!(
                formatter,
                "malformed SPD field '{field}': expected NAME:VALUE"
            ),
            Error::UnknownSpdField { field } => write!(formatter, "unknown SPD field '{field}'"),
            Error::DuplicateSpdField { field } => {
                write!(formatter, "duplicate SPD field '{field}'")
            }
            Error::MissingSpdField { field } => {
                write!(formatter, "missing required SPD field '{field}'")
            }
            Error::InvalidPercentEscape { input } => {
                write!(formatter, "invalid percent escape in '{input}'")
            }
            Error::Io { message } => write!(formatter, "I/O error: {message}"),
            Error::Csv { message } => write!(formatter, "CSV error: {message}"),
            Error::MissingCsvColumn { column } => {
                write!(formatter, "CSV is missing the required column '{column}'")
            }
            Error::JsonEncoding { message } => {
                write!(formatter, "JSON encoding failed: {message}")
            }
            Error::Zip { message } => write!(formatter, "ZIP error: {message}"),
        }
    }
}

impl std::error::Error for Error {}
