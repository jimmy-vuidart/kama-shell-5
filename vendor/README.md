# Third-party code carried in the repository

## `iced_exdevtools` 0.19.1 (MIT)

Copy of the crate published on crates.io, by the authors of
`iced_layershell` (<https://github.com/waycrate/exwlshelleventloop>).

One change, in `src/keymap.rs`: `native_key_code` gets a wildcard arm.
`winit-core` made `NativeKeyCode` non-exhaustive in 0.31.0-beta.3, the
version Smithay pins exactly; the published crate was written for beta.2 and
does not compile against it.

The root `Cargo.toml` substitutes this copy with `[patch.crates-io]`. Delete
the directory and the patch once a published `iced_layershell` builds against
the winit version of the pinned Smithay.
