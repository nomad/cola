# Changelog

## [Unreleased]

### Added

- `Eq` impls for `Insertion` and `Deletion`

### Bug fixes

- fixed a bug that would cause `Replica::decode()` to fail if it was encoded
  on a machine with a different pointer size (#1);

[Unreleased]: https://github.com/nomad/cola/compare/v0.1.0...HEAD
