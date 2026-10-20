`mitex` 0.2.4 (Apache-2.0, https://github.com/mitex-rs/mitex) with one fix in
`src/converter.rs`: `convert_inner` used the default command spec for output
even when a custom spec was passed, so custom aliases had no effect. Drop this
copy once upstream releases the fix.
