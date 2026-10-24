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

## `smithay` 0.7.0, commit `19c16d3` (MIT)

Copy of the commit of <https://github.com/Smithay/smithay> that the
compositor pins (the same as niri 26.4.0), without its examples, benches and
workspace members.

One change, in `src/xwayland/xwm/mod.rs` (`SelectionRequest`, marked
`coquille:`): before an X11 client is sent a selection, the window manager
watches property changes on the requestor's window. A selection larger than
64 KiB goes by the INCR protocol, where each chunk waits for the requestor to
delete a property; Smithay only watched the windows created as InputOutput,
while Chromium and Electron ask from an InputOnly window. Pasting an image
copied from a Wayland application (a screenshot) into an X11 Electron
application hung its main process forever (Orca).

The compositor's `Cargo.toml` still names the git commit; the root
`Cargo.toml` substitutes this copy with
`[patch."https://github.com/Smithay/smithay.git"]`. When moving to another
Smithay commit, copy it here again and carry the change over, or delete the
directory and the patch once Smithay fixes it.
