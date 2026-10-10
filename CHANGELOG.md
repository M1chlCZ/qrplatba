# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-10-10

### Added

- EPC069-12 (SEPA) payload support through `EpcPayment`, `epc_string` and the
  command line options `--scheme epc` and `--name`.
- SPD string validation through `parse_spd` and the command line option
  `--validate`, with a human readable summary or JSON output through `--json`.
- Batch generation from CSV data through `process_batch` and `zip_batch`, with
  the command line options `--batch` and `--zip`.
