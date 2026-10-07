use qrplatba::{
    Error, Payment, escape_spd, fold_ascii, format_amount, normalize_currency, normalize_iban,
    parse_amount,
};

const CZ_IBAN: &str = "CZ6508000000192000145399";
const SK_IBAN: &str = "SK3112000000198742637541";

fn minimal() -> Payment {
    Payment::new(CZ_IBAN, 500)
}

#[test]
fn minimal_spd_contains_defaults_in_stable_order() {
    assert_eq!(
        minimal().to_spd().unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK"
    );
}

#[test]
fn spd_field_order_is_acc_am_cc_dt_msg_vs_ks_ss() {
    let payment = Payment::new(CZ_IBAN, 1)
        .with_currency("CZK")
        .with_variable_symbol("1234567890")
        .with_constant_symbol("0308")
        .with_specific_symbol("12")
        .with_message("Příliš žluťoučký kůň")
        .with_due_date("2026-10-07");
    assert_eq!(
        payment.to_spd().unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:0.01*CC:CZK*DT:20261007*MSG:Prilis zlutoucky kun*X-VS:1234567890*X-KS:0308*X-SS:12"
    );
}

#[test]
fn spd_percent_escapes_reserved_characters() {
    let payment = minimal().with_message("Sleva * 10% & vice Ø");
    assert_eq!(
        payment.to_spd().unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*MSG:Sleva %2A 10%25 & vice %C3%98"
    );
}

#[test]
fn escape_spd_escapes_only_reserved_and_non_ascii() {
    assert_eq!(escape_spd("A*B%C"), "A%2AB%25C");
    assert_eq!(escape_spd("Příliš"), "Prilis");
    assert_eq!(escape_spd("€"), "%E2%82%AC");
    assert_eq!(escape_spd("běžný text"), "bezny text");
}

#[test]
fn fold_ascii_removes_czech_diacritics() {
    assert_eq!(
        fold_ascii("Příliš žluťoučký kůň úpěl ďábelské ódy"),
        "Prilis zlutoucky kun upel dabelske ody"
    );
    assert_eq!(fold_ascii("ŽLUŤOUČKÝ KŮŇ"), "ZLUTOUCKY KUN");
    assert_eq!(fold_ascii("bez diakritiky"), "bez diakritiky");
}

#[test]
fn format_amount_uses_two_decimal_places() {
    assert_eq!(format_amount(125_000).unwrap(), "1250.00");
    assert_eq!(format_amount(100).unwrap(), "1.00");
    assert_eq!(format_amount(1).unwrap(), "0.01");
    assert_eq!(format_amount(99).unwrap(), "0.99");
}

#[test]
fn format_amount_rejects_non_positive_values() {
    assert_eq!(format_amount(0).unwrap_err(), Error::AmountNotPositive);
    assert_eq!(format_amount(-1).unwrap_err(), Error::AmountNotPositive);
}

#[test]
fn parse_amount_accepts_dot_and_comma() {
    assert_eq!(parse_amount("1250").unwrap(), 125_000);
    assert_eq!(parse_amount("1250.00").unwrap(), 125_000);
    assert_eq!(parse_amount("1250,00").unwrap(), 125_000);
    assert_eq!(parse_amount("0.01").unwrap(), 1);
    assert_eq!(parse_amount("1.5").unwrap(), 150);
    assert_eq!(parse_amount("92233720368547758.07").unwrap(), i64::MAX);
}

#[test]
fn parse_amount_rejects_malformed_values() {
    let malformed = [
        "", "abc", "1.234", ".5", "5.", "1,2,3", "1 000", "-5", "+5", "1..2", ",", "1,2,", "1.2.3",
    ];
    for input in malformed {
        assert!(
            matches!(parse_amount(input), Err(Error::InvalidAmountFormat { .. })),
            "input {input:?} should be rejected"
        );
    }
}

#[test]
fn parse_amount_rejects_zero() {
    assert_eq!(parse_amount("0").unwrap_err(), Error::AmountNotPositive);
    assert_eq!(parse_amount("0.00").unwrap_err(), Error::AmountNotPositive);
}

#[test]
fn parse_amount_rejects_overflow() {
    assert!(matches!(
        parse_amount("92233720368547758.08"),
        Err(Error::AmountOverflow { .. })
    ));
}

#[test]
fn normalize_iban_accepts_valid_cz_and_sk() {
    assert_eq!(normalize_iban(CZ_IBAN).unwrap(), CZ_IBAN);
    assert_eq!(normalize_iban(SK_IBAN).unwrap(), SK_IBAN);
}

#[test]
fn normalize_iban_removes_spaces_and_uppercases() {
    assert_eq!(
        normalize_iban("cz65 0800 0000 1920 0014 5399").unwrap(),
        CZ_IBAN
    );
    assert_eq!(
        normalize_iban("CZ65\t0800 0000 1920 0014 5399").unwrap(),
        CZ_IBAN
    );
}

#[test]
fn normalize_iban_rejects_bad_checksum() {
    assert_eq!(
        normalize_iban("CZ6508000000192000145398").unwrap_err(),
        Error::IbanChecksum
    );
}

#[test]
fn normalize_iban_rejects_wrong_lengths() {
    let short = "CZ650800000019";
    let long = CZ_IBAN.repeat(2);
    assert_eq!(
        normalize_iban(short).unwrap_err(),
        Error::IbanLength { length: 14 }
    );
    assert_eq!(
        normalize_iban(&long).unwrap_err(),
        Error::IbanLength { length: 48 }
    );
    assert_eq!(normalize_iban("").unwrap_err(), Error::EmptyIban);
    assert_eq!(normalize_iban("   ").unwrap_err(), Error::EmptyIban);
}

#[test]
fn normalize_iban_rejects_invalid_characters() {
    assert_eq!(
        normalize_iban("CZ65-8000000192000145399").unwrap_err(),
        Error::IbanCharacter { character: '-' }
    );
}

#[test]
fn normalize_currency_requires_three_letters() {
    assert_eq!(normalize_currency("CZK").unwrap(), "CZK");
    assert_eq!(normalize_currency("eur").unwrap(), "EUR");
    assert!(matches!(
        normalize_currency("CZ"),
        Err(Error::InvalidCurrency { .. })
    ));
    assert!(matches!(
        normalize_currency("CZKK"),
        Err(Error::InvalidCurrency { .. })
    ));
    assert!(matches!(
        normalize_currency("C1K"),
        Err(Error::InvalidCurrency { .. })
    ));
}

#[test]
fn currency_is_upper_cased_in_output() {
    assert_eq!(
        minimal().with_currency("eur").to_spd().unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:EUR"
    );
}

#[test]
fn symbol_limits_are_enforced() {
    assert!(
        minimal()
            .with_variable_symbol("1234567890")
            .to_spd()
            .is_ok()
    );
    assert!(matches!(
        minimal().with_variable_symbol("12345678901").to_spd(),
        Err(Error::InvalidSymbol {
            name: "variable symbol",
            max_length: 10,
            ..
        })
    ));
    assert!(minimal().with_constant_symbol("1234").to_spd().is_ok());
    assert!(matches!(
        minimal().with_constant_symbol("12345").to_spd(),
        Err(Error::InvalidSymbol {
            name: "constant symbol",
            max_length: 4,
            ..
        })
    ));
    assert!(matches!(
        minimal().with_specific_symbol("12345").to_spd(),
        Err(Error::InvalidSymbol {
            name: "specific symbol",
            max_length: 4,
            ..
        })
    ));
    assert!(matches!(
        minimal().with_variable_symbol("").to_spd(),
        Err(Error::InvalidSymbol { .. })
    ));
    assert!(matches!(
        minimal().with_variable_symbol("12a").to_spd(),
        Err(Error::InvalidSymbol { .. })
    ));
}

#[test]
fn message_is_limited_to_60_characters_after_folding() {
    assert!(minimal().with_message("a".repeat(60)).to_spd().is_ok());
    assert_eq!(
        minimal().with_message("a".repeat(61)).to_spd().unwrap_err(),
        Error::MessageTooLong {
            length: 61,
            max_length: 60
        }
    );
    assert!(matches!(
        minimal().with_message("ž".repeat(61)).to_spd(),
        Err(Error::MessageTooLong { length: 61, .. })
    ));
}

#[test]
fn due_date_requires_iso_format_and_real_date() {
    let payment = minimal().with_due_date("2024-02-29");
    assert_eq!(
        payment.to_spd().unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*DT:20240229"
    );
    let invalid = [
        "2026-02-29",
        "2026-02-30",
        "2026-00-10",
        "2026-13-01",
        "07.10.2026",
        "2026-4-07",
        "2026-10-7",
        "not-a-date",
    ];
    for input in invalid {
        assert!(
            matches!(
                minimal().with_due_date(input).to_spd(),
                Err(Error::InvalidDueDate { .. })
            ),
            "input {input:?} should be rejected"
        );
    }
}

#[test]
fn empty_message_and_due_date_are_omitted() {
    let payment = minimal().with_message("").with_due_date("");
    assert_eq!(payment.to_spd().unwrap(), minimal().to_spd().unwrap());
}

#[test]
fn serde_round_trip_preserves_payment() {
    let payment = minimal()
        .with_message("Faktura")
        .with_due_date("2026-10-07");
    let json = serde_json::to_string(&payment).unwrap();
    let parsed: Payment = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, payment);
}

#[test]
fn serde_default_currency_is_czk() {
    let payment: Payment =
        serde_json::from_str(r#"{"iban":"CZ6508000000192000145399","amount_minor":500}"#).unwrap();
    assert_eq!(payment.currency, "CZK");
    assert_eq!(payment.to_spd().unwrap(), minimal().to_spd().unwrap());
}
