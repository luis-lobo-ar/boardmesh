# mesh-id

Small identifier conventions shared by Mesh applications. This crate uses
[`ulid`](https://docs.rs/ulid/1.2.1/ulid/) for generation and adds string formatting
helpers and an opaque correlation-value wrapper.

It is an internal compatibility helper for Mesh products. It does not
implement a new identifier algorithm. Applications that need a general ULID
library can use `ulid` directly.

## Usage

```rust
use mesh_id::{CorrelationId, new_lowercase_prefixed_id, new_ulid_string};

let session_id = new_lowercase_prefixed_id("session");
assert!(session_id.starts_with("session-"));

let correlation = CorrelationId::new(session_id);
let propagated = correlation.clone();
assert_eq!(correlation, propagated);

let generated = new_ulid_string();
assert_eq!(generated.len(), 26);
```

Mesh products use a local workspace path. For a product under `opt/`:

```toml
[dependencies]
mesh-id = { path = "../../lib/mesh-id" }
```

## API contract

| Item | Behavior |
| --- | --- |
| `new_ulid_string()` | Generates an uppercase, 26-character ULID string. |
| `new_lowercase_ulid_string()` | Generates the same format with lowercase letters. |
| `new_prefixed_id(prefix)` | Produces `prefix-ULID`, preserving the supplied prefix. |
| `new_lowercase_prefixed_id(prefix)` | Produces `prefix-ulid`, lowercasing only the generated suffix. |
| `CorrelationId::new(value)` | Wraps an existing string without validation or normalization. |

The public `CorrelationId.0` field remains available. The wrapper implements
`Clone`, `Debug`, `PartialEq`, `Eq`, and `Hash`.

Prefixes are caller policy. Empty prefixes, mixed case, Unicode, and existing
hyphens are accepted. This crate does not turn a prefix into a validated type or
reject arbitrary correlation values.

## Guarantees and limits

- Generation delegates to the upstream `ulid` implementation.
- A generated ULID contains a timestamp. Do not treat it as a secret or an access
  credential.
- This crate does not add a monotonic generator, a collision registry, or
  ordering guarantees across threads, processes, or clock changes.
- IDs are returned as owned strings. There is no database, network service,
  configuration file, serialization framework, or Mesh runtime dependency.
- The crate requires the Rust standard library and uses the Mesh workspace's
  Rust toolchain and 2024 edition.

## Compatibility

The helper preserves the existing Mesh identifier formats. Changing separator,
casing, prefix treatment, or the public
correlation wrapper requires an explicit compatibility decision.

## Development

```sh
cargo test -p mesh-id
cargo clippy -p mesh-id --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc -p mesh-id --no-deps
```

Authoritative development stays in the Mesh monorepo under `lib/mesh-id`.
Standalone product exports include a generated copy when they need it. The crate
is not published to a package registry. Its manifest sets `publish = false`.
Mesh code is MIT-licensed. Upstream dependencies retain their own licenses.
