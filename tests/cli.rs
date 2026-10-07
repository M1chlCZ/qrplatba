use std::fs;
use std::path::PathBuf;
use std::process::Command;

const CZ_IBAN: &str = "CZ6508000000192000145399";

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
