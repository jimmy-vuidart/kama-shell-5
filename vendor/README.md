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

## `iced_wgpu` 0.14.0 (MIT)

Copy of the crate published on crates.io, by the authors of iced
(<https://github.com/iced-rs/iced>).

One change, in `src/window/compositor.rs`: the surface format is taken among
those with 8 bits of alpha when there is one. The published crate takes the
first format of the driver's list; Mesa lists `Rgb10a2Unorm` first, whose
alpha has 2 bits: a translucent veil or shadow loses its alpha but keeps its
premultiplied colors, and the compositor adds them to what is behind (the
dock turned white).

The root `Cargo.toml` substitutes this copy with `[patch.crates-io]`. Delete
the directory and the patch once iced picks such a format itself.
