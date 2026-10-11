# Contributing to ntpprobe

Thank you for your interest in improving `ntpprobe`.

## Guidelines

1. **Standalone Crate**: Keep `ntpprobe` completely self-contained with no workspace dependencies.
2. **Pure Rust Wire Codec**: Maintain the zero-C-dependency RFC 5905 binary parser and calculation logic.
3. **Verification**: Always include unit tests for packet serialization/deserialization and integration tests against real UDP sockets.
4. **Style**: Format code with `cargo fmt` and ensure zero `cargo clippy` warnings.
