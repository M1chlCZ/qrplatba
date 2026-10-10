# qrplatba

`qrplatba` makes Czech QR payments. It builds the SPAYD (SPD) string for the
payment, and it renders the string as a PNG or SVG QR code. Banks and payment
applications in Czechia read this format. It also builds EPC069-12 (SEPA)
payloads for Euro payments, validates complete SPD strings, and generates one
QR image per CSV row in batch mode.

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
| `--currency CODE` | ISO 4217 currency code. The default is `CZK` for SPD and `EUR` for EPC. |
| `--vs N` | Variable symbol, 1 to 10 digits. |
| `--ks N` | Constant symbol, 1 to 4 digits. |
| `--ss N` | Specific symbol, 1 to 4 digits. |
| `--message TEXT` | Remittance message. The SPD limit is 60 characters, the EPC limit is 140. |
| `--due YYYY-MM-DD` | Due date. |
| `--scheme spd\|epc` | Payment scheme. The default is `spd`. |
| `--name NAME` | Beneficiary name. Required for `--scheme epc`. |
| `--format png\|svg` | QR image format. The default is `png`. |
| `--output PATH` | Write the output to this file. In batch mode this is the output directory. Without this option, the tool writes to standard output. |
| `--scale N` | Pixels per QR module. The default is `8`. |
| `--spd-only` | Print the payment payload and skip the QR image. |
| `--validate STRING` | Validate an SPD string and print a summary. See below. |
| `--json` | Print the validation result as JSON. Requires `--validate`. |
| `--batch INPUT.CSV` | Generate one QR image per CSV row. See below. |
| `--zip FILE.ZIP` | Also write the batch files and `index.json` into a ZIP archive. Requires `--batch`. |

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

## EPC/SEPA payments

Use `--scheme epc` and `--name` for an EPC069-12 (SEPA) payload. The EPC
payload is Euro only and uses the message as the remittance line:

```sh
qrplatba --scheme epc --name "Firma s.r.o." \
  --iban CZ6508000000192000145399 --amount 1250.00 \
  --message "Faktura 2026-001" --output epc.png

qrplatba --scheme epc --name "Firma s.r.o." \
  --iban CZ6508000000192000145399 --amount 1250.00 --spd-only
```

The payload has one line per field:

```text
BCD
002
1
SCT
Firma s.r.o.
CZ6508000000192000145399
EUR1250.00
Faktura 2026-001
```

The rules:

* The currency must be `EUR`.
* The beneficiary name is required and limited to 70 characters.
* The remittance line is limited to 140 characters and the information line
  to 70 characters.
* The amount must be between 0.01 and 999999999.99 EUR.
* Control characters are rejected in every text line.

The library exposes `EpcPayment` and `epc_string`. The builder adds the
optional BIC and information lines:

```rust
use qrplatba::{EpcPayment, Payment};

let payment = Payment::new("CZ6508000000192000145399", 125_000)
    .with_currency("EUR")
    .with_message("Faktura 2026-001");

let payload = EpcPayment::new(&payment, "Firma s.r.o.")
    .with_bic("GIBACZPX")
    .with_information("Objednavka 42")
    .to_payload()
    .unwrap();
```

QR encoding uses the same pipeline as SPD.

## Validate an SPD string

`--validate` checks a complete SPD string and prints a normalized summary:

```sh
qrplatba --validate 'SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK*X-VS:2026001'
qrplatba --validate 'SPD*1.0*ACC:CZ6508000000192000145399*AM:1250.00*CC:CZK' --json
```

The first command prints:

```text
IBAN: CZ6508000000192000145399
Amount: 1250.00 CZK
Variable symbol: 2026001
```

The command exits with code `0` when the string is valid and `2` when it is
not. It validates the IBAN checksum, the amount format, the symbols, the date,
the message length, and all percent escapes. Duplicate and unknown fields are
rejected. The `--json` output contains the parsed payment with the amount in
minor units. Field order is free; the summary uses a stable order.

## Batch mode

`--batch` generates one QR image per CSV row:

```sh
qrplatba --batch payments.csv --output qr
qrplatba --batch payments.csv --output qr --format svg
qrplatba --batch payments.csv --output qr --scheme epc --zip qr.zip
```

The CSV has a header and the columns
`iban,amount,currency,vs,ks,ss,message,due,name`. The `iban` and `amount`
columns are required; all other columns are optional. The `name` column is
required for `--scheme epc`. When the `currency` column is missing, SPD rows
use `CZK` and EPC rows use `EUR`. Empty cells are treated as absent values.

Each generated file is named `<row>-<vs|index>.<ext>`. `<row>` is the 1-based
data row number. The second part is the variable symbol, or the row number
when the variable symbol is empty or not file-safe. The output directory also
receives an `index.json` report:

```json
[
  {
    "row": 1,
    "file": "1-2026001.png",
    "error": null
  },
  {
    "row": 2,
    "file": null,
    "error": "IBAN checksum failed (MOD-97 != 1)"
  }
]
```

Bad rows are recorded in the report and do not stop the batch. The command
exits with code `0` when the CSV file can be processed, even when some rows
fail, and prints a summary with the generated and failed counts. `--zip` also
writes the generated files and `index.json` into a ZIP archive.

## Library

```rust
use qrplatba::{Payment, parse_spd};

let payment = Payment::new("CZ6508000000192000145399", 125_000)
    .with_variable_symbol("2026001")
    .with_message("Faktura 2026-001")
    .with_due_date("2026-10-31");

let spd: String = payment.to_spd().unwrap();
let png: Vec<u8> = payment.to_qr_png(8).unwrap();
let svg: String = payment.to_qr_svg(8).unwrap();
let summary: String = payment.summary().unwrap();

let parsed: Payment = parse_spd(&spd).unwrap();
assert_eq!(parsed, payment);
```

Amounts use minor units. For example, `125_000` means `1250.00`. The functions
`parse_amount` and `format_amount` convert between a string and minor units
without floating point arithmetic.

The QR codes use error correction level M. The quiet zone is 4 modules. The PNG
uses one bit per pixel, and the SVG uses a `crispEdges` path.

## Offline operation

The crate makes no network access. It reads no environment variables, and it
writes no files unless you use `--output`, `--batch` or `--zip`. All results
come from the input values alone.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
cargo publish --dry-run
```

## License

MIT. See [LICENSE](LICENSE). Copyright (c) 2026 m1chlcz.
