//! Czech QR payment (SPAYD/SPD) string and QR image generator.
//!
//! The crate builds deterministic SPAYD ("Short Payment Descriptor") strings for
//! Czech QR payments and renders them as PNG or SVG QR codes. All processing
//! happens offline; the crate performs no network access.
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
        let code = self.encode_qr()?;
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

    /// Renders the payment as an SVG QR code with `scale` pixels per module.
    pub fn to_qr_svg(&self, scale: u32) -> Result<String, Error> {
        let code = self.encode_qr()?;
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

    fn encode_qr(&self) -> Result<QrCode, Error> {
        let spd = self.to_spd()?;
        encode_byte_mode(spd.as_bytes())
    }
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
        }
    }
}

impl std::error::Error for Error {}
