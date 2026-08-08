# Third-party licenses

The Rust dependencies resolved in `Cargo.lock` retain their own licenses. The direct dependencies are licensed under one or more of MIT, Apache-2.0, ISC, or similarly permissive terms.

The transitive dependency set also includes MPL-2.0 (`option-ext`), Unicode-3.0, Zlib, BSD, CDLA-Permissive-2.0, and Unlicense terms. `option-ext` does not declare itself incompatible with MPL secondary licenses; MPL 2.0 permits combination in a Larger Work under AGPLv3 while its own notices and source terms remain available.

Before distributing a binary release, generate the dependency license inventory from the exact locked dependency graph and bundle the applicable notices and license texts. The repository's AGPL license does not replace third-party licenses.

To inspect the machine-readable license expressions in the current graph:

```sh
cargo metadata --format-version 1 --locked |
  jq -r '.packages[] | [.name, .version, (.license // "UNKNOWN")] | @tsv' |
  sort -u
```
