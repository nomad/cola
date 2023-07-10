# 🥤 cola

[![Latest version]](https://crates.io/crates/cola-crdt)
[![CI]](https://github.com/nomad/cola/actions)
[![Docs]](https://docs.rs/cola-crdt)

[Latest version]: https://img.shields.io/crates/v/cola-crdt.svg
[CI]: https://github.com/nomad/cola/actions/workflows/ci.yml/badge.svg
[Docs]: https://docs.rs/cola-crdt/badge.svg

cola is a Conflict-free Replicated Data Type specialized for real-time
collaborative editing of plain text documents.

It allows multiple peers on a distributed network to concurrently edit the same
text document, making sure that they all converge to the same final state
without relying on a central server to coordinate the edits.

Check out [the docs][docs] to learn about cola's API, or [this blog post][cola]
for a deeper dive into its design and implementation.

# A note on the crate's naming scheme

cola's `package.name` is `cola-crdt`, while its `lib.name` is simply `cola`.
This is because the package name has to be unique in order to be published to
[crates.io], but unfortunately `cola` is already taken by a crate squatter.

What this means practically for you, the user of the library, is that you
should import cola as `cola-crdt` in your `Cargo.toml`, and `use` it as
`cola` in your source code.

For example:

```toml
# Cargo.toml
cola-crdt = "0.1"
```

```rust
// main.rs
use cola::Replica;

fn main() {
    println!("{:?}", Replica::default());
}
```

[docs]: https://docs.rs/cola
[cola]: https://www.nomad.foo/blog/cola
[crates.io]: https://www.crates.io
