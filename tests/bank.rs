use qrplatba::bank::{bank_name, czech_bank};

const CZ_IBAN: &str = "CZ6508000000192000145399";
const CZ_UNKNOWN_BANK_IBAN: &str = "CZ7799990000192000145399";
const SK_IBAN: &str = "SK3112000000198742637541";

#[test]
fn known_codes_resolve_to_bank_names() {
    let known = [
        ("0100", "Komerční banka"),
        ("0300", "ČSOB"),
        ("0600", "MONETA Money Bank"),
        ("0710", "Česká národní banka"),
        ("0800", "Česká spořitelna"),
        ("2010", "Fio banka"),
        ("2250", "Banka CREDITAS"),
        ("3030", "Air Bank"),
        ("5500", "Raiffeisenbank"),
        ("6210", "mBank"),
    ];
    for (code, name) in known {
        assert_eq!(bank_name(code), Some(name), "bank code {code}");
    }
}

#[test]
fn unknown_codes_resolve_to_no_name() {
    assert_eq!(bank_name("9999"), None);
    assert_eq!(bank_name("2060"), None);
    assert_eq!(bank_name(""), None);
    assert_eq!(bank_name("800"), None);
}

#[test]
fn czech_bank_extracts_the_code_from_a_czech_iban() {
    let bank = czech_bank(CZ_IBAN).unwrap();
    assert_eq!(bank.code, "0800");
    assert_eq!(bank.name, Some("Česká spořitelna"));
}

#[test]
fn czech_bank_reports_an_unknown_code_without_a_name() {
    let bank = czech_bank(CZ_UNKNOWN_BANK_IBAN).unwrap();
    assert_eq!(bank.code, "9999");
    assert_eq!(bank.name, None);
}

#[test]
fn czech_bank_ignores_other_countries_and_non_digit_codes() {
    assert_eq!(czech_bank(SK_IBAN), None);
    assert_eq!(czech_bank("CZ65ABCD0000192000145399"), None);
    assert_eq!(czech_bank("CZ65"), None);
}
