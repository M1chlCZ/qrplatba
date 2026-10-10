use qrplatba::{EpcPayment, Error, Payment, epc_string};

const CZ_IBAN: &str = "CZ6508000000192000145399";

fn payment() -> Payment {
    Payment::new(CZ_IBAN, 125_000)
        .with_currency("EUR")
        .with_message("Faktura 2026-001")
}

#[test]
fn epc_payload_matches_the_golden_string() {
    assert_eq!(
        epc_string(&payment(), "Firma s.r.o.").unwrap(),
        "BCD\n002\n1\nSCT\nFirma s.r.o.\nCZ6508000000192000145399\nEUR1250.00\nFaktura 2026-001"
    );
}

#[test]
fn epc_optional_lines_follow_the_remittance_line() {
    let payload = EpcPayment::new(&payment(), "Firma s.r.o.")
        .with_bic("gibaczpx")
        .with_information("Objednavka 42")
        .to_payload()
        .unwrap();
    assert_eq!(
        payload,
        "BCD\n002\n1\nSCT\nGIBACZPX\nFirma s.r.o.\nCZ6508000000192000145399\nEUR1250.00\nFaktura 2026-001\nObjednavka 42"
    );
}

#[test]
fn epc_minimal_payload_omits_optional_lines() {
    let minimal = Payment::new(CZ_IBAN, 1).with_currency("EUR");
    assert_eq!(
        epc_string(&minimal, "Firma").unwrap(),
        "BCD\n002\n1\nSCT\nFirma\nCZ6508000000192000145399\nEUR0.01"
    );
}

#[test]
fn epc_rejects_non_euro_currency() {
    let usd = payment().with_currency("USD");
    assert_eq!(
        epc_string(&usd, "Firma").unwrap_err(),
        Error::UnsupportedEpcCurrency {
            input: "USD".to_string()
        }
    );
}

#[test]
fn epc_rejects_empty_beneficiary() {
    assert_eq!(
        epc_string(&payment(), "   ").unwrap_err(),
        Error::EpcTextEmpty {
            name: "beneficiary name"
        }
    );
}

#[test]
fn epc_rejects_long_remittance() {
    let long = payment().with_message("a".repeat(141));
    assert_eq!(
        epc_string(&long, "Firma").unwrap_err(),
        Error::EpcTextTooLong {
            name: "remittance",
            length: 141,
            max_length: 140
        }
    );
}

#[test]
fn epc_rejects_long_beneficiary() {
    let long = "a".repeat(71);
    assert_eq!(
        epc_string(&payment(), &long).unwrap_err(),
        Error::EpcTextTooLong {
            name: "beneficiary name",
            length: 71,
            max_length: 70
        }
    );
}

#[test]
fn epc_rejects_control_characters_in_beneficiary() {
    assert_eq!(
        epc_string(&payment(), "Firma\ns.r.o.").unwrap_err(),
        Error::ControlCharacter { character: '\n' }
    );
}

#[test]
fn epc_rejects_invalid_bic() {
    let error = EpcPayment::new(&payment(), "Firma")
        .with_bic("GIBACZP")
        .to_payload()
        .unwrap_err();
    assert_eq!(
        error,
        Error::InvalidBic {
            input: "GIBACZP".to_string()
        }
    );
}

#[test]
fn epc_accepts_boundary_amounts() {
    let maximum = Payment::new(CZ_IBAN, 99_999_999_999).with_currency("EUR");
    assert!(
        epc_string(&maximum, "Firma")
            .unwrap()
            .ends_with("EUR999999999.99")
    );
    let over = Payment::new(CZ_IBAN, 100_000_000_000).with_currency("EUR");
    assert_eq!(
        epc_string(&over, "Firma").unwrap_err(),
        Error::EpcAmountOutOfRange {
            amount_minor: 100_000_000_000
        }
    );
}

#[test]
fn epc_rejects_invalid_iban() {
    let invalid = Payment::new("CZ6508000000192000145398", 100).with_currency("EUR");
    assert_eq!(
        epc_string(&invalid, "Firma").unwrap_err(),
        Error::IbanChecksum
    );
}

#[test]
fn epc_qr_png_and_svg_are_generated() {
    let payment = payment();
    let epc = EpcPayment::new(&payment, "Firma s.r.o.");
    let png = epc.to_qr_png(4).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let svg = epc.to_qr_svg(4).unwrap();
    assert!(svg.starts_with("<svg"));
    assert_eq!(svg, epc.to_qr_svg(4).unwrap());
}
