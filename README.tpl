# Ratatui-image

[![CI Badge]][CI]
[![Crate Badge]][Crate]
[![Docs Badge]][Docs]

[CI Badge]: https://img.shields.io/github/actions/workflow/status/ratatui/ratatui-image/ci.yaml?style=flat-square&logo=github
[CI]: https://github.com/ratatui/ratatui-image/actions?query=workflow%3A
[Crate Badge]: https://img.shields.io/crates/v/ratatui-image?logo=rust&style=flat-square
[Crate]: https://crates.io/crates/ratatui-image
[Docs Badge]: https://img.shields.io/docsrs/ratatui-image?logo=rust&style=flat-square
[Docs]: https://docs.rs/ratatui-image/latest/ratatui_image/index.html

### Showcase:

![Screen recording](./assets/showcase.gif)

[Link to screenshot test suite with array of terminals](https://benjajaja.github.io/ratatui-image-screenshots/) (xterm, foot, kitty, wezterm, ghostty, rio, mlterm...)

{{readme}}

### Compatibility matrix

Compatibility and QA:

Terminal  | Protocol | OK | QA | Notes
----------|----------|----|----|------
Xterm     | `Sixel`  | ✔️ | ✔️ | Run with `-ti 340` to make sure sixel support is enabled.
Foot      | `Sixel`  | ✔️ | ✔️ | Wayland.
Kitty     | `Kitty`  | ✔️ | ✔️ | Reference for the `Kitty` protocol (requires Kitty 0.28.0 or later).
Wezterm   | `iTerm2` | ✔️ | ❌ | Also would support `Sixel` and `Kitty`, but only `iTerm2` actually works bug-free.
Ghostty   | `Kitty`  | ✔️ | ✔️ | Implements `Kitty` with unicode placeholders.
iTerm2    | `iTerm2` | ✔️ | -  | Reference for the `iTerm2` protocol. Mac only.
Rio       | `iTerm2` | ✔️ | ✔️ | Also supports `Sixel` but has glitches.
mlterm    | `Sixel`  | ✔️ | ✔️ | Quite slow but no glitches.
Black Box | `Sixel`  | ✔️ | -  | Confirmed only with the flatpak version, most distro packages don't enable Sixel support.
Bobcat    | `iTerm2` | ✔️ | -  | Works on all versions and builds. Falls back to `Sixel` if `TERM_PROGRAM` variable is not set.
Alacritty | `Sixel`  | ❌ | -  | [There is a sixel fork](https://github.com/microo8/alacritty-sixel), but it's probably never getting merged, and does not clear graphics.
Konsole   | `Sixel`  | ❌ | -  | [Not really fixed in 24.12](https://bugs.kde.org/show_bug.cgi?id=456354)
Contour   | `Sixel`  | ❌ | ❌ | Does not clear graphics.
ctx       | `Sixel`  | ❌ | ❌ | Buggy.
Warp      | `iTerm2` | ❌ | -  | iTerm2 does not clear, Kitty unicode-placeholders part not implemented.

"QA" means that there is a flake VM test that runs the demo and takes a screenshot, posted as PR comment.
`-` means it's not possible or applicable to add a screenshot-test.

[Link to latest screenshot of the test-suite on `master`](https://benjajaja.github.io/ratatui-image-screenshots/)

Halfblocks should work in all terminals, even if the font size could not be detected, with a 4:8 pixel ratio.

### Known issues
Summary | Link
--------|---------
Termwiz backend does not work at all | [#1](https://github.com/ratatui/ratatui-image/issues/1)
Sixel image rendered on the last line of terminal causes a scroll | [#57](https://github.com/ratatui/ratatui-image/issues/57)

### Projects that use ratatui-image

* [mdfried](https://github.com/benjajaja/mdfried)
  A markdown viewer that renders headers bigger (as images), and regular images too.
* [iamb](https://github.com/ulyssa/iamb)
  A matrix client with vim keybindings.
* [joshuto](https://github.com/kamiyaa/joshuto)
  A terminal file manager that can preview images.
* [Aerostream](https://github.com/shigepon7/aerostream)
  A Bluesky client using EventStream.
* [Eilmeldung](https://github.com/christo-auer/eilmeldung)
  RSS reader
* [rvIRC](https://github.com/KaraZajac/rvIRC)
  An IRC client with vim keybindings.

### Comparison

* [viuer](https://crates.io/crates/viuer)
  Renders graphics in different terminals/protocols, but "dumps" the image, making it difficult to
  work for TUI programs.
  The terminal protocol guessing code has been adapted to rustix, thus the author of viuer is
  included in the copyright notice.
* [yazi-adapter](https://github.com/sxyazi/yazi/tree/main/yazi-adapter)
  A library part of [yazi](https://github.com/sxyazi/yazi) the terminal file manager that
  implements many graphics protocols and lets you preview image files.
* [Überzug++](https://github.com/jstkdng/ueberzugpp)
  CLI utility that draws images on terminals by using X11/wayland child windows, sixels, kitty,
  and/or iterm2 protocols (any means necessary). There exists several wrapper or bindings crates.
  More battle-tested but essentially stateful, which makes it hard to use with immediate-mode.

### Contributing

PRs and issues/discussions welcome!

There are some specific rules for a PR to be reviewed at all, please see [CONTRIBUTING.md](CONTRIBUTING.md) for reference.

License: {{license}}
