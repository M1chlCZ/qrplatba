use qrplatba::{Error, parse_spd};

const VALID: &str = "SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK*DT:20261007*MSG:Faktura 2026-001*X-VS:1234567890*X-KS:0308*X-SS:12";

#[test]
fn parse_accepts_a_full_spd_string() {
    let payment = parse_spd(VALID).unwrap();
    assert_eq!(payment.iban, "CZ6508000000192000145399");
    assert_eq!(payment.amount_minor, 125_000);
    assert_eq!(payment.currency, "CZK");
    assert_eq!(payment.due_date.as_deref(), Some("2026-10-07"));
    assert_eq!(payment.message.as_deref(), Some("Faktura 2026-001"));
    assert_eq!(payment.variable_symbol.as_deref(), Some("1234567890"));
    assert_eq!(payment.constant_symbol.as_deref(), Some("0308"));
    assert_eq!(payment.specific_symbol.as_deref(), Some("12"));
}

#[test]
fn parse_round_trips_through_the_builder() {
    assert_eq!(parse_spd(VALID).unwrap().to_spd().unwrap(), VALID);
}

#[test]
fn parse_accepts_any_field_order() {
    let reordered = "SPD*1.0*CC:CZK*X-VS:123*ACC:CZ6508000000192000145399*AM:5.00";
    assert_eq!(
        parse_spd(reordered).unwrap().to_spd().unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*X-VS:123"
    );
}

#[test]
fn parse_decodes_percent_escapes() {
    let input = "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*MSG:Sleva %2A 10%25 %C3%98";
    let payment = parse_spd(input).unwrap();
    assert_eq!(payment.message.as_deref(), Some("Sleva * 10% \u{d8}"));
    assert_eq!(
        payment.to_spd().unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*MSG:Sleva %2A 10%25 %C3%98"
    );
}

#[test]
fn parse_rejects_tampered_iban() {
    let input = VALID.replace("CZ6508000000192000145399", "CZ6508000000192000145398");
    assert_eq!(parse_spd(&input).unwrap_err(), Error::IbanChecksum);
}

#[test]
fn parse_rejects_bad_amounts() {
    let bad = [
        ("AM:12.345", "invalid amount"),
        ("AM:abc", "invalid amount"),
        ("AM:", "invalid amount"),
        ("AM:0", "greater than zero"),
    ];
    for (field, message) in bad {
        let input = format!("SPD*1.0*ACC:CZ6508000000192000145399*CC:CZK*{field}");
        let error = parse_spd(&input).unwrap_err().to_string();
        assert!(
            error.contains(message),
            "error for {field:?} should contain {message:?}, got {error:?}"
        );
    }
}

#[test]
fn parse_rejects_unknown_fields() {
    let input = format!("{VALID}*X-FOO:1");
    assert_eq!(
        parse_spd(&input).unwrap_err(),
        Error::UnknownSpdField {
            field: "X-FOO".to_string()
        }
    );
}

#[test]
fn parse_rejects_duplicate_fields() {
    let input = format!("{VALID}*AM:9.00");
    assert_eq!(
        parse_spd(&input).unwrap_err(),
        Error::DuplicateSpdField { field: "AM" }
    );
}

#[test]
fn parse_rejects_missing_and_malformed_fields() {
    assert_eq!(
        parse_spd("SPD*1.0*AM:5.00*CC:CZK").unwrap_err(),
        Error::MissingSpdField { field: "ACC" }
    );
    assert_eq!(
        parse_spd("SPD*1.0*ACC").unwrap_err(),
        Error::MalformedSpdField {
            field: "ACC".to_string()
        }
    );
    assert!(matches!(
        parse_spd("XYZ*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK"),
        Err(Error::InvalidSpdHeader { .. })
    ));
    assert!(matches!(
        parse_spd("SPD*2.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK"),
        Err(Error::InvalidSpdHeader { .. })
    ));
}

#[test]
fn parse_rejects_raw_control_characters() {
    let input = VALID.replace("CZK", "C\nZK");
    assert_eq!(
        parse_spd(&input).unwrap_err(),
        Error::ControlCharacter { character: '\n' }
    );
}

#[test]
fn parse_rejects_escaped_control_characters() {
    let input = "SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*MSG:line%0Abreak";
    assert_eq!(
        parse_spd(input).unwrap_err(),
        Error::ControlCharacter { character: '\n' }
    );
}

#[test]
fn parse_rejects_malformed_percent_escapes() {
    for value in ["Faktura%2", "Faktura%ZZ", "100%"] {
        let input = format!("SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*MSG:{value}");
        assert_eq!(
            parse_spd(&input).unwrap_err(),
            Error::InvalidPercentEscape {
                input: value.to_string()
            }
        );
    }
}

#[test]
fn parse_rejects_message_over_sixty_characters() {
    let message = "a".repeat(61);
    let input = format!("SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*MSG:{message}");
    assert_eq!(
        parse_spd(&input).unwrap_err(),
        Error::MessageTooLong {
            length: 61,
            max_length: 60
        }
    );
}

#[test]
fn parse_rejects_bad_symbols() {
    for field in ["X-VS:12345678901", "X-VS:12a", "X-KS:12345", "X-SS:12345"] {
        let input = format!("SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*{field}");
        assert!(
            matches!(parse_spd(&input), Err(Error::InvalidSymbol { .. })),
            "field {field:?} should be rejected"
        );
    }
}

#[test]
fn parse_rejects_bad_dates() {
    for field in ["DT:20260230", "DT:2026107", "DT:20261301", "DT:"] {
        let input = format!("SPD*1.0*ACC:CZ6508000000192000145399*AM:5.00*CC:CZK*{field}");
        assert!(
            matches!(parse_spd(&input), Err(Error::InvalidDueDate { .. })),
            "field {field:?} should be rejected"
        );
    }
}

#[test]
fn summary_is_normalized() {
    let payment = parse_spd(VALID).unwrap();
    assert_eq!(
        payment.summary().unwrap(),
        "IBAN: CZ6508000000192000145399\nAmount: 1250.00 CZK\nDue date: 2026-10-07\nMessage: Faktura 2026-001\nVariable symbol: 1234567890\nConstant symbol: 0308\nSpecific symbol: 12"
    );
}
