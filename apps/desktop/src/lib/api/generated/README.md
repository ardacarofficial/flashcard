# Generated API types — do not edit

Everything in this folder except this README is generated from the Rust contract crate
`crates/fc-api` (DTOs, method catalog, error codes, events) with `ts-rs`.

```bash
cargo run -p fc-api --bin gen-ts             # regenerate
cargo run -p fc-api --bin gen-ts -- --check  # verify (CI; also run by `cargo test -p fc-api`)
```

- `methods.ts`: `Methods` map (`"decks.create" → { kind, request, response }`), `API_VERSION`,
  `METHODS`. Build a typed `invoke<M extends MethodName>(m: M, p: RequestOf<M>): Promise<ResponseOf<M>>`
  on top of it; the wire name is `` `${API_VERSION}.${m}` ``.
- `index.ts`: re-exports every type.
- Field names are `snake_case` (same JSON on every transport). 64-bit integers are `number`.

Never hand-write DTOs in the frontend: change the Rust type, regenerate, and let `pnpm check`
show what broke.
