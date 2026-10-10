use std::fs;
use std::io::{Cursor, Read};
use std::path::PathBuf;

use qrplatba::Error;
use qrplatba::batch::{BatchEntry, PaymentScheme, QrFormat, process_batch, zip_batch};

const CSV: &str = "iban,amount,currency,vs,ks,ss,message,due,name\n\
CZ6508000000192000145399,1250.00,CZK,123,308,,Faktura 2026-001,2026-10-07,Firma\n\
CZ6508000000192000145398,10.00,CZK,,,,,\n\
CZ6508000000192000145399,1.00,EUR,7,,,,,Firma\n";

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("qrplatba-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn batch_writes_files_and_index_with_row_errors() {
    let output = temp_dir("batch-spd");
    let entries = process_batch(
        Cursor::new(CSV),
        &output,
        PaymentScheme::Spd,
        QrFormat::Png,
        4,
    )
    .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].row, 1);
    assert_eq!(entries[0].file.as_deref(), Some("1-123.png"));
    assert!(entries[0].error.is_none());
    assert!(output.join("1-123.png").is_file());
    assert_eq!(entries[1].row, 2);
    assert!(entries[1].file.is_none());
    assert!(entries[1].error.as_deref().unwrap().contains("IBAN"));
    assert_eq!(entries[2].row, 3);
    assert_eq!(entries[2].file.as_deref(), Some("3-7.png"));
    assert!(entries[2].error.is_none());
    let index: Vec<BatchEntry> =
        serde_json::from_str(&fs::read_to_string(output.join("index.json")).unwrap()).unwrap();
    assert_eq!(index, entries);
    let png = fs::read(output.join("1-123.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let _ = fs::remove_dir_all(&output);
}

#[test]
fn batch_falls_back_to_the_row_number_without_a_variable_symbol() {
    let output = temp_dir("batch-index-name");
    let csv = "iban,amount\nCZ6508000000192000145399,5\n";
    let entries = process_batch(
        Cursor::new(csv),
        &output,
        PaymentScheme::Spd,
        QrFormat::Svg,
        4,
    )
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file.as_deref(), Some("1-1.svg"));
    let svg = fs::read_to_string(output.join("1-1.svg")).unwrap();
    assert!(svg.starts_with("<svg"));
    let _ = fs::remove_dir_all(&output);
}

#[test]
fn batch_requires_iban_and_amount_columns() {
    let output = temp_dir("batch-columns");
    let error = process_batch(
        Cursor::new("iban,currency\nCZ6508000000192000145399,CZK\n"),
        &output,
        PaymentScheme::Spd,
        QrFormat::Png,
        4,
    )
    .unwrap_err();
    assert_eq!(error, Error::MissingCsvColumn { column: "amount" });
    let _ = fs::remove_dir_all(&output);
}

#[test]
fn batch_epc_uses_name_and_defaults_to_euro() {
    let output = temp_dir("batch-epc");
    let csv = "iban,amount,name\n\
CZ6508000000192000145399,10.00,Firma\n\
CZ6508000000192000145399,10.00,\n";
    let entries = process_batch(
        Cursor::new(csv),
        &output,
        PaymentScheme::Epc,
        QrFormat::Png,
        4,
    )
    .unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].error.is_none());
    assert!(entries[0].file.as_deref().unwrap().ends_with(".png"));
    assert!(
        entries[1]
            .error
            .as_deref()
            .unwrap()
            .contains("beneficiary name")
    );
    let _ = fs::remove_dir_all(&output);
}

#[test]
fn zip_contains_the_generated_files_and_index() {
    let output = temp_dir("batch-zip");
    let entries = process_batch(
        Cursor::new(CSV),
        &output,
        PaymentScheme::Spd,
        QrFormat::Png,
        4,
    )
    .unwrap();
    let zip_path = output.join("bundle.zip");
    zip_batch(&zip_path, &output, &entries).unwrap();
    let file = fs::File::open(&zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut names: Vec<String> = archive.file_names().map(str::to_string).collect();
    names.sort();
    assert_eq!(names, vec!["1-123.png", "3-7.png", "index.json"]);
    let mut index_text = String::new();
    archive
        .by_name("index.json")
        .unwrap()
        .read_to_string(&mut index_text)
        .unwrap();
    let index: Vec<BatchEntry> = serde_json::from_str(&index_text).unwrap();
    assert_eq!(index, entries);
    let mut archived_png = Vec::new();
    archive
        .by_name("1-123.png")
        .unwrap()
        .read_to_end(&mut archived_png)
        .unwrap();
    assert_eq!(archived_png, fs::read(output.join("1-123.png")).unwrap());
    drop(archive);
    let _ = fs::remove_dir_all(&output);
}
