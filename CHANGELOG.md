# Changelog

## [Unreleased]

### Added

- a `Clone` impl for `Replica`;

## [0.4.4] - Feb 1 2024

### Changed

- the `Deletion`'s internal `VersionMap` now only includes entries for the
  `ReplicaId`s that were between the start and the end of the deleted range,
  instead of including every single `ReplicaId` the `Replica` had ever seen.
  This results in much smaller `Deletion`s when deleting small ranges in
  buffers edited by many peers ([#9](https://github.com/nomad/cola/pull/9));

## [0.4.3] - Jan 31 2024

### Fixed

- a typo in the docs

## [0.4.2] - Jan 31 2024

### Added

- a new `Deletion::deleted_by()` to get the `ReplicaId` of the peer that
  performed the deletion;

## [0.4.0] - Jan 28 2024

### Changed

- the `Serialize` impl of `Deletion` now produces ~3x smaller payloads,
  depending on the data format used
  ([#7](https://github.com/nomad/cola/pull/7));

- the `Serialize` impl of `EncodedReplica` now produces ~7x smaller payloads,
  depending on the data format used
  ([#6](https://github.com/nomad/cola/pull/6));

- the `Serialize` impl of `Insertion` now produces 3-4x smaller payloads,
  depending on the data format used ([#5](https://github.com/nomad/cola/pull/5));

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

[Unreleased]: https://github.com/nomad/cola/compare/v0.4.4...HEAD
[0.4.4]: https://github.com/nomad/cola/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/nomad/cola/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/nomad/cola/compare/v0.4.0...v0.4.2
[0.4.0]: https://github.com/nomad/cola/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/nomad/cola/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/nomad/cola/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/nomad/cola/compare/v0.1.0...v0.2.0
