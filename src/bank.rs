//! Offline lookup of Czech bank codes.
//!
//! The module resolves the four-digit bank code from a Czech IBAN against a
//! static table of established Czech bank codes. The lookup is offline, so it
//! performs no network access. A code outside the table resolves to no name;
//! unknown codes do not fail validation.

use serde::Serialize;

/// A Czech bank resolved from an IBAN.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CzechBank {
    /// Four-digit bank code from positions 5 to 8 of the IBAN.
    pub code: String,
    /// Bank name, or `None` when the code is not in the offline table.
    pub name: Option<&'static str>,
}

/// Resolves a four-digit Czech bank code to a bank name.
///
/// The offline table covers ten established Czech bank codes: `0100`
/// Komerční banka, `0300` ČSOB, `0600` MONETA Money Bank, `0710` Česká
/// národní banka, `0800` Česká spořitelna, `2010` Fio banka, `2250` Banka
/// CREDITAS, `3030` Air Bank, `5500` Raiffeisenbank and `6210` mBank. Any
/// other code returns `None` and does not fail validation.
pub fn bank_name(code: &str) -> Option<&'static str> {
    match code {
        "0100" => Some("Komerční banka"),
        "0300" => Some("ČSOB"),
        "0600" => Some("MONETA Money Bank"),
        "0710" => Some("Česká národní banka"),
        "0800" => Some("Česká spořitelna"),
        "2010" => Some("Fio banka"),
        "2250" => Some("Banka CREDITAS"),
        "3030" => Some("Air Bank"),
        "5500" => Some("Raiffeisenbank"),
        "6210" => Some("mBank"),
        _ => None,
    }
}

/// Resolves the Czech bank of a normalized IBAN.
///
/// The IBAN must be normalized and valid; use [`crate::normalize_iban`] first.
/// The bank code occupies positions 5 to 8, right after the `CZ` prefix and
/// the two check digits. The function returns `None` for an IBAN from another
/// country and for code positions that are not four digits. An unknown code
/// resolves to a bank without a name.
///
/// # Example
///
/// ```
/// use qrplatba::bank::czech_bank;
///
/// let bank = czech_bank("CZ6508000000192000145399").unwrap();
/// assert_eq!(bank.code, "0800");
/// assert_eq!(bank.name, Some("Česká spořitelna"));
/// ```
pub fn czech_bank(iban: &str) -> Option<CzechBank> {
    let bban = iban.strip_prefix("CZ")?;
    let code = bban.get(2..6)?;
    if !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(CzechBank {
        code: code.to_string(),
        name: bank_name(code),
    })
}
