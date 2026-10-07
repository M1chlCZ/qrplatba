# qrplatba

`qrplatba` makes Czech QR payments. It builds the SPAYD (SPD) string for the
payment, and it renders the string as a PNG or SVG QR code. Banks and payment
applications in Czechia read this format.

The command line tool and the library work offline. They make no network
access, and the output is deterministic.

## Install

```sh
cargo install qrplatba
```

## Command line

```sh
qrplatba --iban CZ6508000000192000145399 --amount 1250,00 --vs 2026001 --spd-only
qrplatba --iban CZ6508000000192000145399 --amount 1250.00 --message "Faktura 2026-001" --output faktura.png
qrplatba --iban CZ6508000000192000145399 --amount 1250 --format svg --output faktura.svg
qrplatba --iban CZ6508000000192000145399 --amount 1250 --scale 4 > faktura.png
```

## Command line options

| Option | Meaning |
| --- | --- |
| `--iban IBAN` | Receiving account. Spaces and letter case are ignored. |
| `--amount AMOUNT` | Amount in major units. A dot or a comma is the decimal separator. |
| `--currency CODE` | ISO 4217 currency code. The default is `CZK`. |
| `--vs N` | Variable symbol, 1 to 10 digits. |
| `--ks N` | Constant symbol, 1 to 4 digits. |
| `--ss N` | Specific symbol, 1 to 4 digits. |
| `--message TEXT` | Remittance message, at most 60 characters. |
| `--due YYYY-MM-DD` | Due date. |
| `--format png\|svg` | QR image format. The default is `png`. |
| `--output PATH` | Write the output to this file. Without this option, the tool writes to standard output. |
| `--scale N` | Pixels per QR module. The default is `8`. |
| `--spd-only` | Print the SPD string and skip the QR image. |

The tool exits with code `2` when an input is invalid or an operation fails.

## SPD rules

The string starts with `SPD*1.0`. The fields follow in this fixed order:

| Field | Meaning | Rules |
| --- | --- | --- |
| `ACC` | Receiving account | IBAN with 15 to 34 characters and a valid MOD-97 checksum |
| `AM` | Amount | Decimal number with a dot and exactly two decimal places. The value must be more than zero. |
| `CC` | Currency | Three-letter ISO 4217 code, for example `CZK` |
| `DT` | Due date | `YYYYMMDD` |
| `MSG` | Remittance message | At most 60 characters after ASCII folding |
| `X-VS` | Variable symbol | 1 to 10 digits |
| `X-KS` | Constant symbol | 1 to 4 digits |
| `X-SS` | Specific symbol | 1 to 4 digits |

The builder omits empty fields. It folds Czech diacritics to ASCII first, for
example `Příliš žluťoučký kůň` becomes `Prilis zlutoucky kun`. It then
percent-encodes `*` as `%2A`, `%` as `%25`, and each non-ASCII character as
UTF-8 bytes.

## Library

```rust
use qrplatba::Payment;

let payment = Payment::new("CZ6508000000192000145399", 125_000)
    .with_variable_symbol("2026001")
    .with_message("Faktura 2026-001")
    .with_due_date("2026-10-31");

let spd: String = payment.to_spd().unwrap();
let png: Vec<u8> = payment.to_qr_png(8).unwrap();
let svg: String = payment.to_qr_svg(8).unwrap();
```

Amounts use minor units. For example, `125_000` means `1250.00`. The functions
`parse_amount` and `format_amount` convert between a string and minor units
without floating point arithmetic.

The QR codes use error correction level M. The quiet zone is 4 modules. The PNG
uses one bit per pixel, and the SVG uses a `crispEdges` path.

## Offline operation

The crate makes no network access. It reads no environment variables, and it
writes no files unless you use `--output`. All results come from the input
values alone.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

## License

MIT. See [LICENSE](LICENSE). Copyright (c) 2026 m1chlcz.
