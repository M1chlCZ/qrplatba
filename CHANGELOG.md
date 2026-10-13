# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Czech bank code lookup for `--validate`: the four-digit bank code from
  positions 5 to 8 of a Czech IBAN resolves through an offline table of ten
  established bank codes, and the summary prints the bank name when the table
  knows the code. Unknown codes print `Bank: unknown` and do not fail
  validation.
- The `--json` validation output gains the `bank_code` and `bank_name` fields.
  Both are `null` for a non-Czech IBAN; `bank_name` is also `null` when the
  code is unknown.

### Changed

- `Payment::summary()` prints the bank code and, when known, the bank name
  between the IBAN and the amount for Czech IBANs.

## [0.2.0] - 2026-10-10

### Added

- EPC069-12 (SEPA) payload support through `EpcPayment`, `epc_string` and the
  command line options `--scheme epc` and `--name`.
- SPD string validation through `parse_spd` and the command line option
  `--validate`, with a human readable summary or JSON output through `--json`.
- Batch generation from CSV data through `process_batch` and `zip_batch`, with
  the command line options `--batch` and `--zip`.
