# Changelog

## [Unreleased]

## [0.3.0] - Jan 9 2024

### Added

- a new `Replica::create_anchor()` method to create an `Anchor` from an offset
  and an `AnchorBias`;

- a new `Replica::resolve_anchor()` method to resolve an `Anchor` into an
  offset;

## [0.2.1] - Jan 2 2024

### Added

- `Debug` impl for `EncodedReplica`;

- `Display` and `Error` impls for `DecodeError`;

### Changed

- `Replica::encode()` makes fewer transient allocations;

- the stack size of `EncodedReplica` was decreased from 56 to 32;

## [0.2.0] - Dec 23 2023

### Added

- `Eq` impls for `Insertion` and `Deletion`

### Bug fixes

- fixed a bug that would cause `Replica::decode()` to fail if it was encoded
  on a machine with a different pointer size (#1);

[Unreleased]: https://github.com/nomad/cola/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/nomad/cola/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/nomad/cola/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/nomad/cola/compare/v0.1.0...v0.2.0
