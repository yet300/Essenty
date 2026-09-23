# Examples

Runnable end-to-end demos live per crate (e.g. `crates/essenty/examples/`),
so `cargo run -p <crate> --example <name>` works without polluting the
workspace root. This directory holds shared notes.

## Counter (`crates/essenty/examples/counter.rs`)

Exercises all four core crates together, mirroring the README quick start:

```bash
cargo run -p essenty --example counter
```

It drives a `LifecycleRegistry` through create → start → resume, saves and
restores a `StateKeeper` counter, retains a model object in an
`InstanceKeeper`, and dispatches a regular + predictive back gesture through
a `BackDispatcher`.
