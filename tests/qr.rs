use image::ImageDecoder;
use qrplatba::{Error, Payment};

const CZ_IBAN: &str = "CZ6508000000192000145399";

const BYTE_CAPACITIES_M: [usize; 40] = [
    14, 26, 42, 62, 84, 106, 122, 152, 180, 213, 251, 287, 331, 362, 412, 450, 504, 560, 624, 666,
    711, 779, 857, 911, 997, 1059, 1125, 1190, 1264, 1370, 1452, 1538, 1628, 1722, 1809, 1911,
    1989, 2099, 2213, 2331,
];

fn payment() -> Payment {
    Payment::new(CZ_IBAN, 125_000)
        .with_variable_symbol("1234567890")
        .with_message("Faktura 2026-001")
}

fn expected_modules(data_len: usize) -> u32 {
    let version = BYTE_CAPACITIES_M
        .iter()
        .position(|&capacity| data_len <= capacity)
        .expect("data fits into a version 40 QR code")
        + 1;
    17 + 4 * version as u32
}

fn expected_side(data_len: usize, scale: u32) -> u32 {
    (expected_modules(data_len) + 8) * scale
}

fn svg_width(svg: &str) -> u32 {
    let start = svg.find("width=\"").expect("width attribute") + 7;
    let end = start + svg[start..].find('"').expect("width value");
    svg[start..end].parse().expect("numeric width")
}

#[test]
fn png_starts_with_magic_bytes_and_matches_expected_size() {
    let payment = payment();
    let spd = payment.to_spd().unwrap();
    let png = payment.to_qr_png(8).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(&png)).unwrap();
    let side = expected_side(spd.len(), 8);
    assert_eq!(decoder.dimensions(), (side, side));
}

#[test]
fn png_generation_is_deterministic() {
    let payment = payment();
    assert_eq!(payment.to_qr_png(8).unwrap(), payment.to_qr_png(8).unwrap());
}

#[test]
fn svg_starts_with_svg_tag_and_is_deterministic() {
    let payment = payment();
    let spd = payment.to_spd().unwrap();
    let svg = payment.to_qr_svg(8).unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("shape-rendering=\"crispEdges\""));
    let side = expected_side(spd.len(), 8);
    assert!(svg.contains(&format!("width=\"{side}\"")));
    let total = expected_modules(spd.len()) + 8;
    assert!(svg.contains(&format!("viewBox=\"0 0 {total} {total}\"")));
    assert_eq!(svg, payment.to_qr_svg(8).unwrap());
}

#[test]
fn scale_changes_output_size() {
    let payment = payment();
    let small = payment.to_qr_svg(1).unwrap();
    let large = payment.to_qr_svg(8).unwrap();
    assert_eq!(svg_width(&large), svg_width(&small) * 8);
}

#[test]
fn zero_scale_is_rejected() {
    let payment = payment();
    assert_eq!(
        payment.to_qr_png(0).unwrap_err(),
        Error::InvalidScale { scale: 0 }
    );
    assert_eq!(
        payment.to_qr_svg(0).unwrap_err(),
        Error::InvalidScale { scale: 0 }
    );
}
