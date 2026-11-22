# Versioned API samples (plugin API v1)

One file per method a plugin may call: `method`, `kind`, the `permission` it needs, an `input`
and an `output` exactly as the host hands them over (JSON). They are **checked**: the app's test
`plugin_api_samples_match_the_api` (`crates/fc-app/src/modules/plugins/tests.rs`) parses every
sample into the API's own request/response types and writes it back unchanged, and compares
the method's kind and permission. A sample that stops matching means the v1 API changed, which
it must not (add fields only through a new API version).

Ids and timestamps are made up; `next_cursor` is opaque.
