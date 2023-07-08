# 🥤 cola

[![Latest version]](https://crates.io/crates/cola)
[![CI]](https://github.com/nomad/cola/actions)
[![Docs]](https://docs.rs/cola)

[Latest version]: https://img.shields.io/crates/v/cola.svg
[CI]: https://github.com/nomad/cola/actions/workflows/ci.yml/badge.svg
[Docs]: https://docs.rs/cola/badge.svg

cola is a Conflict-free Replicated Data Type ([CRDT]) specialized for real-time
collaborative editing of plain text documents.

It allows multiple peers on a distributed network to concurrently edit the same
text document, making sure that they all converge to the same final state
without relying on any central server to coordinate the edits.

Check out [the docs][docs] to learn about cola's API, or [this blog post][cola]
for a deeper dive into cola's design and implementation.

[CRDT]: https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type
[docs]: https://docs.rs/cola
[cola]: https://www.nomad.foo/blog/cola
