use std::fs;
use std::path::PathBuf;
use std::process::Command;

const CZ_IBAN: &str = "CZ6508000000192000145399";
const CZ_UNKNOWN_BANK_IBAN: &str = "CZ7799990000192000145399";
const SK_IBAN: &str = "SK3112000000198742637541";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qrplatba"))
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("qrplatba-test-{}-{name}", std::process::id()))
}

#[test]
fn missing_arguments_exit_with_code_2() {
    let output = command().output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn spd_only_prints_spd_string() {
    let output = command()
        .args([
            "--iban",
            CZ_IBAN,
            "--amount",
            "1250,00",
            "--vs",
            "123",
            "--spd-only",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK*X-VS:123\n"
    );
}

#[test]
fn png_goes_to_stdout() {
    let output = command()
        .args(["--iban", CZ_IBAN, "--amount", "1250", "--format", "png"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(&output.stdout[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn svg_goes_to_stdout() {
    let output = command()
        .args(["--iban", CZ_IBAN, "--amount", "1250", "--format", "svg"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.starts_with(b"<svg"));
}

#[test]
fn output_file_is_written() {
    let path = temp_path("output.png");
    let output = command()
        .args([
            "--iban",
            CZ_IBAN,
            "--amount",
            "1250",
            "--output",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let _ = fs::remove_file(&path);
}

#[test]
fn spd_only_can_be_written_to_a_file() {
    let path = temp_path("output.txt");
    let output = command()
        .args([
            "--iban",
            CZ_IBAN,
            "--amount",
            "1250",
            "--spd-only",
            "--output",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        "SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK\n"
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn bad_iban_exits_with_code_2() {
    let output = command()
        .args([
            "--iban",
            "CZ6508000000192000145398",
            "--amount",
            "1",
            "--spd-only",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("IBAN"));
}

#[test]
fn three_decimals_exit_with_code_2() {
    let output = command()
        .args(["--iban", CZ_IBAN, "--amount", "1.234", "--spd-only"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn negative_amount_exits_with_code_2() {
    let output = command()
        .args(["--iban", CZ_IBAN, "--amount", "-1", "--spd-only"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn unknown_format_exits_with_code_2() {
    let output = command()
        .args(["--iban", CZ_IBAN, "--amount", "1", "--format", "gif"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn scale_zero_exits_with_code_2() {
    let output = command()
        .args(["--iban", CZ_IBAN, "--amount", "1", "--scale", "0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

const VALID_SPD: &str = "SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK*DT:20261007*MSG:Faktura 2026-001*X-VS:123";

#[test]
fn validate_valid_string_exits_zero_with_a_summary() {
    let output = command().args(["--validate", VALID_SPD]).output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("IBAN: CZ6508000000192000145399"));
    assert!(text.contains("Amount: 1250.00 CZK"));
    assert!(text.contains("Due date: 2026-10-07"));
    assert!(text.contains("Message: Faktura 2026-001"));
    assert!(text.contains("Variable symbol: 123"));
}

#[test]
fn validate_json_exits_zero_with_parsed_fields() {
    let output = command()
        .args(["--validate", VALID_SPD, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["iban"], CZ_IBAN);
    assert_eq!(value["amount_minor"], 125_000);
    assert_eq!(value["currency"], "CZK");
    assert_eq!(value["due_date"], "2026-10-07");
    assert_eq!(value["variable_symbol"], "123");
}

#[test]
fn validate_known_bank_code_prints_code_and_name() {
    let output = command()
        .args([
            "--validate",
            "SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "IBAN: CZ6508000000192000145399\nBank code: 0800\nBank: Česká spořitelna\nAmount: 1250.00 CZK\n"
    );
}

#[test]
fn validate_unknown_bank_code_prints_unknown_and_stays_valid() {
    let output = command()
        .arg("--validate")
        .arg(format!("SPD*1.0*ACC:{CZ_UNKNOWN_BANK_IBAN}*AM:1.00*CC:CZK"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("IBAN: {CZ_UNKNOWN_BANK_IBAN}\nBank code: 9999\nBank: unknown\nAmount: 1.00 CZK\n")
    );
}

#[test]
fn validate_non_czech_iban_prints_no_bank_lines() {
    let output = command()
        .arg("--validate")
        .arg(format!("SPD*1.0*ACC:{SK_IBAN}*AM:1.00*CC:CZK"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("IBAN: {SK_IBAN}\nAmount: 1.00 CZK\n")
    );
}

#[test]
fn validate_json_includes_the_resolved_bank() {
    let output = command()
        .args(["--validate", VALID_SPD, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["bank_code"], "0800");
    assert_eq!(value["bank_name"], "Česká spořitelna");
}

#[test]
fn validate_json_reports_unknown_banks_and_other_countries_as_null() {
    let unknown = command()
        .arg("--validate")
        .arg(format!("SPD*1.0*ACC:{CZ_UNKNOWN_BANK_IBAN}*AM:1.00*CC:CZK"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(unknown.status.success());
    let value: serde_json::Value = serde_json::from_slice(&unknown.stdout).unwrap();
    assert_eq!(value["bank_code"], "9999");
    assert_eq!(value["bank_name"], serde_json::Value::Null);
    let foreign = command()
        .arg("--validate")
        .arg(format!("SPD*1.0*ACC:{SK_IBAN}*AM:1.00*CC:CZK"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(foreign.status.success());
    let value: serde_json::Value = serde_json::from_slice(&foreign.stdout).unwrap();
    assert_eq!(value["bank_code"], serde_json::Value::Null);
    assert_eq!(value["bank_name"], serde_json::Value::Null);
}

#[test]
fn validate_tampered_iban_exits_with_code_2() {
    let output = command()
        .args([
            "--validate",
            "SPD*1.0*ACC:CZ6508000000192000145398*AM:1.00*CC:CZK",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("IBAN"));
}

#[test]
fn validate_unknown_field_exits_with_code_2_with_a_specific_error() {
    let output = command()
        .args(["--validate", &format!("{VALID_SPD}*X-FOO:1")])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("X-FOO"));
}

#[test]
fn validate_ignores_missing_generation_arguments() {
    let output = command()
        .args([
            "--validate",
            "SPD*1.0*ACC:CZ6508000000192000145399*AM:1*CC:CZK",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn epc_scheme_requires_a_name() {
    let output = command()
        .args(["--scheme", "epc", "--iban", CZ_IBAN, "--amount", "1250"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--name"));
}

#[test]
fn epc_scheme_generates_a_png() {
    let output = command()
        .args([
            "--scheme",
            "epc",
            "--name",
            "Firma s.r.o.",
            "--iban",
            CZ_IBAN,
            "--amount",
            "1250",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(&output.stdout[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn epc_scheme_prints_the_payload_with_spd_only() {
    let output = command()
        .args([
            "--scheme",
            "epc",
            "--name",
            "Firma s.r.o.",
            "--iban",
            CZ_IBAN,
            "--amount",
            "1250",
            "--spd-only",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "BCD\n002\n1\nSCT\nFirma s.r.o.\nCZ6508000000192000145399\nEUR1250.00\n"
    );
}

#[test]
fn batch_cli_writes_files_index_and_zip() {
    let directory = temp_path("batch-dir");
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let csv = directory.join("input.csv");
    fs::write(
        &csv,
        "iban,amount,vs\nCZ6508000000192000145399,10,42\nCZ6508000000192000145398,5,\n",
    )
    .unwrap();
    let output_dir = directory.join("out");
    let zip_path = directory.join("out.zip");
    let output = command()
        .args([
            "--batch",
            csv.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--zip",
            zip_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output_dir.join("1-42.png").is_file());
    assert!(output_dir.join("index.json").is_file());
    assert!(zip_path.is_file());
    let summary = String::from_utf8(output.stdout).unwrap();
    assert!(summary.contains("1 generated, 1 failed"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("row 2"));
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn batch_requires_an_output_directory() {
    let path = temp_path("batch-no-output.csv");
    fs::write(&path, "iban,amount\nCZ6508000000192000145399,10\n").unwrap();
    let output = command()
        .args(["--batch", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--output"));
    let _ = fs::remove_file(&path);
}

#[test]
fn zip_without_batch_exits_with_code_2() {
    let output = command()
        .args([
            "--iban",
            CZ_IBAN,
            "--amount",
            "1",
            "--zip",
            "/tmp/unused.zip",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
