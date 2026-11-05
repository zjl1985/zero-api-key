# Third-party notices

The desktop application uses GPUI 0.2.2, developed by Zed Industries and its
contributors, under the Apache License, Version 2.0.

`crates/desktop/src/input.rs` is adapted from GPUI's `examples/input.rs`,
Copyright Zed Industries, Inc. and contributors. Changes include password
masking, blocking password copy/cut, horizontal scrolling, input-method range
handling, theme support, and application change notifications.

Source: https://docs.rs/crate/gpui/0.2.2/source/examples/input.rs
The original source used here is included in the published `gpui` 0.2.2 crate.
The Apache license is reproduced in `licenses/Apache-2.0.txt`.

All other Rust dependency versions are recorded in `Cargo.lock`. No Zed editor
components, editor assets, fonts, JavaScript runtime, or WebView are bundled.
