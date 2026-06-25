# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [11.0.6](https://github.com/ratatui/ratatui-image/compare/v11.0.5...v11.0.6) - 2026-06-25

### Other

- only filter out '\0'
- add check for ascii control characters returned by `chafa_canvas_get_char_at()`

## [11.0.5](https://github.com/ratatui/ratatui-image/compare/v11.0.4...v11.0.5) - 2026-06-20

### Other

- fix panic with large images: move type casts from u16 to usize to be before multiplication

## [11.0.4](https://github.com/ratatui/ratatui-image/compare/v11.0.3...v11.0.4) - 2026-06-05

### Fixed

- *(sixel, iterm2)* use CellDiffOption::ForcedWidth(1) for escape sequence cell

### Changed

- update ratatui requirement to ^0.30.1

## [11.0.3](https://github.com/ratatui/ratatui-image/compare/v11.0.2...v11.0.3) - 2026-06-02

### Other

- add SlicedProtocol::new_with_resize

## [11.0.2](https://github.com/ratatui/ratatui-image/compare/v11.0.1...v11.0.2) - 2026-05-11

### Other

- add option to query background color with OSC

## [11.0.1](https://github.com/ratatui/ratatui-image/compare/v11.0.0...v11.0.1) - 2026-05-10

### Fixed

- fix SlicedProtocol::Sixel trailing empty bands
- fix changelog

## [11.0.0](https://github.com/ratatui/ratatui-image/compare/v11.0.0...v10.0.8) - 2026-05-10

### Overview

The terminal naturally has vertical scrolling. Many TUIs also follow vertical scrolling. Images
should scroll in and out of a viewport, but `ratatui-image` had no mechanism for doing so natively.
It would theoretically be possible for the consumer to slice images into rows, and threat each row
as its own `Protocol` and then render one `Image` widget for only the visible rows. But in 
practice this would incur a performance and memory footprint hit. Then, sixels use a 256 color
palette, and `chafa` should operate on the whole image, otherwise the output quality degrades
significantly. Kitty also internally already has the image sliced into "cells" with its 
unicode-placeholders protocol, making this a much better natural fit for partial displaying.

This realease adds the `sliced` module, bringing in `SlicedProtocol` and `SlicedImage` structs.

`SlicedProtocol` takes care of the optimal image data for each protocol: Kitty needs nothing 
special, Sixels are split up by their natural "bands" of six pixel tall columns, ITerm2 is simply
split into several base64 PNG slices, and Halfblocks also needs nothing special.

`SlicedImage`'s constructor takes a `SignedPosition`, which is relative to the "render area" given 
to it on render, and may be negative or greater than the render area bottom Y.  
That is, render receives the viewport, and the position may be used to display the image partially,
at least on the vertical axis. This approach is flexible enough to also always create a viewport of
the image's size or smaller, in cases where "viewport" doesn't apply or is not known at render 
time, but there is a size constraint.

### Changed

- `FontSize = (u16, u16)` alias became `struct FontSize { width: u16, height: u16 }`.
- Replaced `Rect` with `Size` everywhere it was used for size, or without positioning
  - `Picker::new_protocol()`
  - `Resize::render_area()` became `Resize::size_for()`
  - `ProtocolTrait::area() -> Rect` became `ProtocolTrait::size() -> Size`
  - `Protocol::area() -> Rect` became `Protocol::size() -> Size`
  - `StatefulProtocol::size_for()`
  - `ThreadProtocol::size_for()`
  - `ResizeEncodeRender::resize_encode()`
  - `ResizeEncodeRender::needs_resize()`
  - `Halfblocks::new()`
  - `Kitty::new()`
  - `Sixel::new()`
  - `ITerm2::new()`
  - `ImageSource::round_pixel_size_to_cells()`
  - `chafa::encode()`
  - `primitive::encode()`
  - Any other instances missed here should be trivial to fix.
- `Image` does not render with clipping under Kitty or Halfblocks, to be consistent with Sixels and
  ITerm2. See new `Image::allow_clipping` to keep existing default behavior.
- `Resize::resize(&self, image: &DynamicImage, font_size: FontSize, size: Size, background_color: Option<Rgba<u8>>) -> DynamicImage`
  resizes a `DynamicImage` to the size, either `Fit`, `Crop`, or `Scale`d.
- `Picker::set_background_color`, `StatefulProtocol::new`, `StatefulProtocol::background_color`,
  `ThreadProtocol::background_color` now take or return an `Option<Rgba[u8]>` for background color.

### Added

- `Image::allow_clipping(bool)` to explicitly allow clipping under Kitty and Halfblocks.
  Previously, Kitty and Halfblocks would automatically clip if the render area was too small, while
  Sixels and ITerm2 can never clip.
- `Resize::natural_size` calculates the "natural" size for an image with the font size.
- `Protocol::needs_placeholder` returns the intersection of the image `Size` and the render `Area`,
  to display some placeholder in that area.
- `sliced` module with `SlicedImage` and `SlicedProtocol` for partially rendering images 
  (horizontal slices).

### Removed

- `Resize::needs_resize`, confusing, internal detail.
- `ImageSource`, confusing, more of an internal detail of `StatefulProtocol`, some functions 
  required both this and `FontSize`.

## [10.0.8](https://github.com/ratatui/ratatui-image/compare/v10.0.7...v10.0.8) - 2026-04-30

### Added

- feature kitty-offset to skip unicode-placeholders in kitty

## [10.0.7](https://github.com/ratatui/ratatui-image/compare/v10.0.6...v10.0.7) - 2026-04-28

### Other

- release-plz must use ratatui as owner now
- unify clear_area() for sixel and iterm2, simplify single row
- add criterion benchmark
- Update README.md
- move repo links from benjajaja/ to ratatui/

### Other
- Unify the "clear_area" function across sixel and iterm2
- Skip cursor movements in "clear_area" if only one row.
- Add a criterion benchmark

## [10.0.6](https://github.com/benjajaja/ratatui-image/compare/v10.0.5...v10.0.6) - 2026-02-19

### Other

- blacklist kitty and sixel detection for WezTerm and Konsole

## [10.0.5](https://github.com/benjajaja/ratatui-image/compare/v10.0.4...v10.0.5) - 2026-02-06

### Fixed

- fix force iterm2 on wezterm+tmux

### Other

- use release-plz
- wrap each kitty "chunk" in tmux passthrough individually
- stop detecting kitty from env vars
- do not use String::insert_str(0, ...) in sixel
- apply same "append" optimization to iterm2
- don't escape non APC sequences wrongly
- lots of tiny optimizations for kitty
- add missing tmux kitty escapes
- unrelated warning fix

* Fix kitty in tmux. Since the image data is chunked, send each chunk as a separate tmux sequence to avoid exceeding some tmux limits.
* Performance improvements related to image data in all protocols.
* Use release-plz from now on.

## [10.0.4] - 2026-01-22

## [10.0.3] - 2026-01-19

* Fix transparent sixel images not having background cleared (same as iterm2).
* Sixels don't need to always be overlayed / padded, since we now have transparency.

## [10.0.2] - 2025-12-28

* Updated to latest [icy_sixel](https://github.com/mkrueger/icy_sixel/pull/14) v0.5.0 with 
  transparency support. If you have been overriding the Picker's background color because there
  were black bars with Sixels at the bottom or right of an image, you don't need to do that anymore.

## [10.0.1] - 2025-12-27

## [10.0.0] - 2025-12-27

## [9.0.0] - 2025-12-25

* Split `chafa` feature into two mutually exclusive features:
  * `chafa-dyn` (default): Dynamically links against `libchafa.so` at compile time.
    Requires libchafa to be available at runtime the same way it was at compile time.
  * `chafa-static`: Statically links against `libchafa.a` at compile time.
    The library is embedded in the binary (see flake.nix for an example on how to build with a
    static chafa).
  * Removed libloading, because if it would be available at runtime, it should also be available
    at compile time.
* Minor stdin-read-timeout adjustments
* Deprecate `Picker::from_fontsize`, add `Picker::halfblocks`
* Added timeout option to `Picker::from_query_stdio_with_options`.

## [8.1.1] - 2025-12-18

* [Chafa](https://hpjansson.org/chafa/) support
  The Halfblocks protocol now supports chafa, if the feature is enabled.
  This "ASCII Art"-like renderer is a million times better than the basic Halfblocks implementation.
  The library is loaded dynamically, if the feature is enabled, at runtime.
  That means no linking acrobacies are required, it should "just work" if the user has libchafa installed on their system.
  It could still be "wrapped", see how the flake.nix handles `LD_LIBRARY_PATH` to magically include this in the `ratatui-image` binary.
  This is still gated behind a feature, because calling chafa uses `unsafe`.
* Improved Halfblocks renderer
  Picks upper block, lower block, or space, depending on pixel colors.
  This gives some vague feedback when the output is rendered without colors, e.g. in insta testing snapshots.

# [8.0.2] - 2025-09-03

* Performance improvements in kitty and iterm2 protocols
* Use base64-simd
* Picker::from_query_stdio returns a halfblocks picker when: no capabilities, stdio query 
  timeout, or no font-size received.
* Add screenshot test suite to CI
* Add tokio support to ThreadProtocol
* Updated to rust 2024
* Add support for terminals: BobCat, warp

# [8.0.1] - 2025-05-30

Fix halfblocks not having the same size as other protocols.

# [8.0.0] - 2025-05-18

Fix `TextSizingProtocol` detection (was incorrectly detecting support on Foot terminal).

Separate "stdio query response" from "capability", as they do not match one-to-one.
Moved `Capability` into the `picker` module, as the interpretation is done there.

# [7.0.0] - 2025-05-18

`Picker` has a new field / method `capabilities`, which return the precise capabilities detected by `Picker::from_query_stdio`.
Since the struct now holds a `Vec`, `Picker` is no longer derives `Copy` (but still derives `Clone`).
This uncovered some unnecessary copying on a method that previously consumed self for no good reason.

Add `Picker::from_query_stdio_with_options` to specifically query for [Text Sizing Protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/#detecting-if-the-terminal-supports-this-protocol).
Not used in this library, but rather an optional convencience feature so that stdio must not be queried twice by some programs.
The capability, if detected, is `Capability::TextSizingProtocol` and can be read from `Picker::capabilities()`.

# [6.0.0] - 2025-05-17

`Image::new` uses a non-mutable borrow.

The mutability was only required for the Kitty protocol, necessary to track wether the image has already been transmitted or only needs placement.

Overall this change should remove the need to use locks when using threads or tokio.

# [5.0.0] - 2025-03-01

Add `StatefulProtocol::size_for`, that can be used to get the size that an image will be rendered to.
This allows positioning the image before it has been rendered, for example centering the image widget with the usual ratatui layout options.

ThreadImage and ThreadProtocol work with `ResizeRequest` and `ResizeResponse` instead of some tuples.
They internally track an ID so that a response for a stale area is discarded correctly.

Huge internal refactor that removes duplicated code across image protocols.

- `Errors` variant case names have been fixed.
- `StatefulProtocol` becomes a struct (was enum).
- `StatefulProtocol::area` which returned the last rendered area has been removed, use `size_for` for accurate results.
- `StatefulProtocol` methods has some parameters removed.
- All protocols except kitty lose their `Stateful...` struct implementation, as one struct can share both protocol implementations.

# [4.2.0] - 2024-12-31 🎆

Fix Sixel and iTerm2 not working with tmux.

# [4.1.2] - 2024-12-30

Add a release job to the CI that makes a github release when a `v*.*.*` is pushed.
The tag itself and pushing to crates.io is done locally.

# [4.1.0] - 2024-12-23 🎄

### Transparency support for Kitty and iTerm2

The image data gains an alpha channel, as well as the background color, which is now transparent by default when the protocol is not Sixel.
The area behind the image is cleared with the control sequences `ECH`, `CUD`, and `CUU` repeatedly for each row and column.
This is not particularly efficient, but it works in most terminals.
`DECERA`, which should erase the entire rectangle with only one sequence, is not implemented correctly (or not at all) in some major terminals.

Sixels could also support transparency, as the spec directly supports it.
A palette color could be set to transparent, and `icy_sixel` in fact supports this, however this is after the fact that the image has been encoded from an API perspective.
In other words, we could set a palette color index to be transparent, but we don't control that this color index would match any transparency of the input image.
This is something that would have to be added to `icy_sixel`.

### Resize: Scale

This feature is brought to you by <taduradnik@proton.me>!
The scale option scales the image, keeping the aspect ratio, to fit the full size of the given area.
It is shown in the demo.

### Capability detection

The control sequence parser has improved its "capability parsing".
In addition to querying Kitty and Sixel protocol support, the "font-size in pixels" is also queried, instead of using `tcgetattr`.
The motivation is that we already need to query stdin, and `tcgetattr` is not supported on Windows Termial, but the control sequence is.

### Minor fixes and changes

- Foreground color is restored after displaying an image with Kitty protocol.
- `Picker` must no longer be mutable.
  It only was mutable so that the kitty image ids would have some sequence, but the start was based off a random number anyway to avoid clashes, so we can also just use a new random id every time.
  This avoids confusion when a new protocol is created, mutating the picker, but discarding the result.
- `area() -> Rect` method on protocols.
  Sometimes it definitely is useful to know how much of the given area a protocol will render to.
- Some logic fixes unnecessary image "needs-resize" calls.

# [3.0.0] - 2024-11-11

Compatible with `ratatui`: `v0.29.0`.

### Windows support

Thanks to Terakomari <im.terakomari@icloud.com>, windows with WSL is now supported.

The latest terminal supports sixels and necessary CSI sequences.
The CI now runs (almost) fully on Windows.

### Font size guessing and CSI parsing

Font size is queried with CSI sequence instead of tcgetattr.
The CSI response parsing has been extended into a full parser module.

### Miscellaneous improvements

- `rustix` is only used on non-windows, is not a crate feature anymore
- Improve magic env var guessing
- Kitty resets only background color
- iTerm2 use PNG intermediate encoding instead of Jpeg
- Use direnv with flake for development

### Static dispatch

Inspired by Uttarayan Mondal <email@uttarayan.me>, changed to static dispatch with an enum instead of a `Box<dyn Protocol>`.
Adding a custom protocol isn't really a use case. Errors are also static now instead of `Box<dyn Error>`.

# [2.0.1] - 2024-10-07

(See git log)
