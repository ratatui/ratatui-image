//! # Image widgets with multiple graphics protocol backends for [ratatui]
//!
//! **Unify terminal image rendering across Sixels, Kitty, and iTerm2 protocols.**
//!
//! [ratatui] is an immediate-mode TUI library.
//! ratatui-image tackles 3 general problems when rendering images with an immediate-mode TUI:
//!
//! **Query the terminal for available graphics protocols**
//!
//! Some terminals may implement one or more graphics protocols, such as Sixels, or the iTerm2 or
//! Kitty graphics protocols. Guess by env vars. If that fails, query the terminal with some
//! control sequences.
//! Fallback to "halfblocks" which uses some unicode half-block characters with fore- and
//! background colors.
//!
//! **Query the terminal for the font-size in pixels.**
//!
//! If there is an actual graphics protocol available, it is necessary to know the font-size to
//! be able to map the image pixels to character cell area.
//! Query the terminal with some control sequences for either the font-size directly, or the
//! window-size in pixels and derive the font-size together with row/column count.
//!
//! **Render the image by the means of the guessed protocol.**
//!
//! Some protocols, like Sixels, are essentially "immediate-mode", but we still need to avoid the
//! TUI from overwriting the image area, even with blank characters.
//! Other protocols, like Kitty, are essentially stateful, but at least provide a way to re-render
//! an image that has been loaded, at a different or same position.
//! Since we have the font-size in pixels, we can precisely map the characters/cells/rows-columns
//! that will be covered by the image and skip drawing over the image.
//!
//! # Quick start
//! ```rust
//! use ratatui::{backend::TestBackend, layout::Size, Terminal, Frame};
//! use ratatui_image::{Image, picker::Picker, protocol::Protocol, Resize};
//!
//! struct App {
//!     // We need to hold the image data somewhere.
//!     image: Protocol,
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let backend = TestBackend::new(80, 30);
//!     let mut terminal = Terminal::new(backend)?;
//!
//!     // Should use `Picker::from_query_stdio()?` to get the font size and protocol,
//!     // but we can't put that here because that would break doctests!
//!     let mut picker = Picker::halfblocks();
//!
//!     // Load an image with the image crate.
//!     let dyn_img = image::ImageReader::open("./assets/Ada.png")?.decode()?;
//!
//!     let font_size = picker.font_size();
//!     let size = Size::new(
//!         dyn_img.width().div_ceil(font_size.width as u32) as u16,
//!         dyn_img.height().div_ceil(font_size.height as u32) as u16,
//!     );
//!
//!     // Create the Protocol once, or in other words, transform the image data to Sixels, Kitty
//!     // data, iTerm2 base64 PNG data, or some kind of ASCII-art.
//!     let image = picker.new_protocol(dyn_img, size, Resize::Fit(None))?;
//!
//!     let mut app = App { image };
//!
//!     // This would be your typical `loop {` in a real app:
//!     terminal.draw(|f| {
//!         let image = Image::new(&app.image);
//!         // Rendering the transformed data is now cheap.
//!         f.render_widget(image, f.area());
//!     });
//!
//!     Ok(())
//! }
//! ```
//! While this approach is usually sufficient, and leaves a lot of room for customizing where the
//! image actually gets transformed, for more advanced usage I really recommend using
//! [`thread::ThreadProtocol`] and looking at `excamples/thread.rs` to get an idea how to
//! dynamically resize images to fit into some area but without blocking the UI.
//!
//! The [picker::Picker] helper is there to do all this font-size and graphics-protocol guessing,
//! and also to map character-cell-size to pixel size so that we can e.g. "fit" an image inside
//! a desired columns+rows bound, and so on.
//!
//! # Widget choice
//! * The [`Image`] widget has a fixed size in rows/columns. If the image pixel size exceeds the
//!   pixel area of the rows/columns, the image is scaled down proportionally to "fit" once, at the
//!   creation time of the [`Protocol`].  
//!   The big upside is that this widget is _stateless_ (in terms of ratatui, i.e. immediate-mode),
//!   and thus can never block the rendering thread/task. A lot of ratatui apps only use stateless
//!   widgets, so this factor is also important when chosing.  
//!   What happens when the image does not fit into the render area can be controlled with
//!   [`Image::allow_clipping`].
//! * The [StatefulImage] widget adapts to its render area at render-time. It can be set to fit,
//!   crop, or scale to the available render area.
//!   This means the widget must be stateful, i.e. use `render_stateful_widget` which takes a
//!   mutable state parameter.
//!   The resizing and encoding is blocking, and since it happens at render-time, it should always
//!   be offloaded to another thread or async task, to keep the UI responsive (see
//!   `examples/thread.rs` and `examples/tokio.rs` on how to use [`thread::ThreadProtocol`]).
//!
//! # Examples
//!
//! * `examples/demo.rs` is a fully fledged demo.
//! * `examples/thread.rs` shows how to offload resize and encoding to another thread, to avoid
//!   blocking the UI thread.
//! * `examples/tokio.rs` same as `thread.rs` but with tokio.
//! * `examples/sliced.rs` shows how to use an image that can have "rows" or "horizontal slices"
//!   partially hidden with any protocol.
//!
//! The lib also includes a binary that renders an image file, but it is focused on testing.
//!
//! # Features
//!
//! ### Backend
//!
//! * `crossterm` (default) if this matches your ratatui backend (most likely).
//! * `termion` if this matches your ratatui backend.
//! * `termwiz` is available, but not working correctly with ratatui-image.
//!
//! ### Chafa library
//!
//! * `chafa-dyn` (default) to use the amazing [chafa](https://hpjansson.org/chafa/) library for
//!   rendering without image protocols. Dynamically link against libchafa.so at compile time.
//!   Requires libchafa to be available at runtime in the same way.
//! * `chafa-static` to statically link against libchafa.a at compile time. The library is embedded
//!   in the binary.
//! * If you absolutely don't want to deal with libchafa, then you should use
//!   `--no-default-features --features image-defaults,crossterm` or a variation thereof.
//!
//! Note: The chafa features are mutually exclusive - enable only one at a time.
//!
//! ### Others
//!
//! * `image-defaults` (default) just enables `image/defaults` (`image` has `default-features =
//!   false`). To only support a selection of image formats and cut down dependencies, disable this
//!   feature, add `image` to your crate, and enable its features/formats as desired. See
//!   <https://doc.rust-lang.org/cargo/reference/features.html#feature-unification/>.
//! * `serde` for `#[derive]`s on [picker::ProtocolType] for convenience, because it might be
//!   useful to save it in some user configuration.
//! * `tokio` whether to use tokio's `UnboundedSender` in `ThreadProtocol`.
//!
//!
//! [ratatui]: https://github.com/ratatui-org/ratatui
//! [sixel]: https://en.wikipedia.org/wiki/Sixel
//! [`render_stateful_widget`]: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Frame.html#method.render_stateful_widget
use std::{
    cmp::{max, min},
    marker::PhantomData,
};

use image::{DynamicImage, ImageBuffer, Rgba, imageops};
use protocol::Protocol;
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    widgets::{StatefulWidget, Widget},
};

pub mod errors;
pub mod picker;
pub mod protocol;
pub mod sliced;
pub mod thread;
pub use image::imageops::FilterType;

type Result<T> = std::result::Result<T, errors::Errors>;

/// The terminal's font size in `(width, height)`
#[derive(Copy, Clone, Debug)]
pub struct FontSize {
    pub width: u16,
    pub height: u16,
}

impl FontSize {
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

impl From<(u16, u16)> for FontSize {
    fn from((width, height): (u16, u16)) -> Self {
        Self::new(width, height)
    }
}

/// Fixed size image widget that uses [Protocol].
///
/// The widget does **not** react to area resizes.
/// Its advantage lies in that the [Protocol] needs only one initial resize.
///
/// The image won't render if it doesn't fit, unless [`Image::allow_clipping`] has been set.
/// ```rust
/// # use ratatui_image::picker::Picker;
/// # use ratatui::layout::Size;
/// # use ratatui_image::{*, sliced::{SlicedProtocol, SlicedImage}};
/// # let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))?;
/// # let picker = Picker::halfblocks(); // Note: use from_query_studio
/// // let picker = Picker::from_query_studio()?;
/// let image = image::ImageReader::open("./assets/NixOS.png")?.decode()?;
/// let proto = picker.new_protocol(image, Size::new(20, 10), Resize::Fit(None))?;
///
/// terminal.draw(|f| {
///     f.render_widget(Image::new(&proto), f.area());
/// });
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Image<'a> {
    image: &'a Protocol,
    allow_clipping: bool,
}

impl<'a> Image<'a> {
    pub fn new(image: &'a Protocol) -> Self {
        Self {
            image,
            allow_clipping: false,
        }
    }

    /// Allow clipping the image if the render area is smaller than the image, and if the protocol
    /// supports it ([`protocol::kitty`] and [`protocol::halfblocks`]).
    ///
    /// This is disabled by default to make the behavior consistent.
    ///
    /// See also [`protocol::Protocol::needs_placeholder`], which is an excellent complement if you
    /// need to render *something* when the image couldn't.
    ///
    /// ```rust
    /// # use ratatui_image::picker::Picker;
    /// # use ratatui::layout::Size;
    /// # use ratatui_image::{*, sliced::*};
    /// # let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))?;
    /// # let picker = Picker::halfblocks();
    /// # let dyn_img = image::ImageReader::open("./assets/NixOS.png")?.decode()?;
    /// # let proto = picker.new_protocol(dyn_img, (20, 10).into(), Resize::Fit(None))?;
    /// terminal.draw(|f| {
    ///     if let Some(placeholder_area) = proto.needs_placeholder(f.area()) {
    ///         // Render `Box` or something with placeholder_area.
    ///     } else {
    ///         f.render_widget(Image::new(&proto).allow_clipping(true), f.area());
    ///     }
    /// });
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn allow_clipping(mut self, allow: bool) -> Self {
        self.allow_clipping = allow;
        self
    }
}

impl Widget for Image<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        if !self.allow_clipping
            && (self.image.size().width > area.width || self.image.size().height > area.height)
        {
            return;
        }

        self.image.render(area, buf);
    }
}

pub trait ResizeEncodeRender {
    /// Resize and encode if necessary, and render immediately.
    fn resize_encode_render(&mut self, resize: &Resize, area: Rect, buf: &mut Buffer) {
        if let Some(rect) = self.needs_resize(resize, area.into()) {
            self.resize_encode(resize, rect);
        }
        self.render(area, buf);
    }

    /// Resize the image and encode it for rendering. The result should be stored statefully so
    /// that next call for the given area does not need to redo the work.
    ///
    /// This can be done in a background thread, and the result is stored in this [protocol::StatefulProtocol].
    fn resize_encode(&mut self, resize: &Resize, size: Size);

    /// Render the currently resized and encoded data to the buffer.
    fn render(&mut self, area: Rect, buf: &mut Buffer);

    /// Check if the current image state would need resizing (grow or shrink) for the given area.
    ///
    /// This can be called by the UI thread to check if this [protocol::StatefulProtocol] should be sent off
    /// to some background thread/task to do the resizing and encoding, instead of rendering. The
    /// thread should then return the [protocol::StatefulProtocol] so that it can be rendered.
    fn needs_resize(&self, resize: &Resize, size: Size) -> Option<Size>;
}

/// Resizeable image widget that uses a [protocol::StatefulProtocol] state.
///
/// This stateful widget resizes the image at render time.
///
/// **Do not use it withou [`thread::ThreadProtocol`] in a reactive UI**. Rendering the widget
/// **will** block the UI thread if the image has not been resized by another thread.
///
/// ```rust
/// # use ratatui::Frame;
/// # use ratatui_image::{Resize, StatefulImage, protocol::{StatefulProtocol}};
/// struct App {
///     image_state: StatefulProtocol,
/// }
/// fn ui(f: &mut Frame<'_>, app: &mut App) {
///     let image = StatefulImage::default().resize(Resize::Crop(None));
///     f.render_stateful_widget(
///         image,
///         f.area(),
///         &mut app.image_state,
///     );
/// }
/// ```
pub struct StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    resize: Resize,
    phantom: PhantomData<T>,
}

impl<T> Default for StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    fn default() -> Self {
        Self::new()
    }
}
impl<T> StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    pub const fn resize(self, resize: Resize) -> Self {
        Self { resize, ..self }
    }

    pub const fn new() -> Self {
        Self {
            resize: Resize::Fit(None),
            phantom: PhantomData,
        }
    }
}

impl<T> StatefulWidget for StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    type State = T;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        state.resize_encode_render(&self.resize, area, buf);
    }
}

#[derive(Debug, Clone)]
/// Resize method
pub enum Resize {
    /// Fit to area.
    ///
    /// If the width or height is smaller than the area, the image will be resized maintaining
    /// proportions.
    ///
    /// The [FilterType] (re-exported from the [image] crate) defaults to [FilterType::Nearest].
    Fit(Option<FilterType>),
    /// Crop to area.
    ///
    /// If the width or height is smaller than the area, the image will be cropped.
    /// The behaviour is the same as using [`Image`] widget with the overhead of resizing,
    /// but some terminals might misbehave when overdrawing characters over graphics.
    /// For example, the sixel branch of Alacritty never draws text over a cell that is currently
    /// being rendered by some sixel sequence, not necessarily originating from the same cell.
    ///
    /// The [CropOptions] defaults to clipping the bottom and the right sides.
    Crop(Option<CropOptions>),
    /// Scale the image
    ///
    /// Same as `Resize::Fit` except it resizes the image even if the image is smaller than the render area
    Scale(Option<FilterType>),
}

impl Default for Resize {
    fn default() -> Self {
        Self::Fit(None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Specifies which sides to be clipped when cropping an image.
pub struct CropOptions {
    /// If `true`, the top side should be clipped.
    pub clip_top: bool,
    /// If `true`, the left side should be clipped.
    pub clip_left: bool,
}

impl Resize {
    /// Resize [`image::DynamicImage`] to fit the `size`.
    fn resize(
        &self,
        image: &DynamicImage,
        font_size: FontSize,
        size: Size,
        background_color: Rgba<u8>,
    ) -> DynamicImage {
        let width = (size.width * font_size.width) as u32;
        let height = (size.height * font_size.height) as u32;

        // Resize/Crop/etc., fitting a multiple of font-size, but not necessarily the area.
        let mut image = self.resize_image(image, width, height);

        if image.width() != width || image.height() != height {
            let mut bg: DynamicImage =
                ImageBuffer::from_pixel(width, height, background_color).into();
            imageops::overlay(&mut bg, &image, 0, 0);
            image = bg;
        }
        image
    }

    /// Check if [`image::DynamicImage`]'s "desired" fits into `target` and is different than `current`.
    ///
    /// The returned `Size` is the area the image needs to be resized to, depending on the resize
    /// type, or `None` if the image matches `target` perfectly at the [`FontSize`].
    pub fn needs_resize(
        &self,
        image: &DynamicImage,
        desired: Option<Size>,
        font_size: FontSize,
        current: Option<Size>,
        target: Size,
        force: bool,
    ) -> Option<Size> {
        let desired = desired.unwrap_or_else(|| {
            Self::round_pixel_size_to_cells(image.width(), image.height(), font_size)
        });

        // Check if resize is needed at all.
        if !force
            && !matches!(self, &Resize::Scale(_))
            && desired.width <= target.width
            && desired.height <= target.height
            && (current.is_none() || current == Some(desired))
        {
            let width = (desired.width * font_size.width) as u32;
            let height = (desired.height * font_size.height) as u32;
            if image.width() == width || image.height() == height {
                return None;
            }
        }

        let rect = self.render_area(image, font_size, target);
        debug_assert!(
            rect.width <= target.width,
            "needs_resize exceeds area width"
        );
        debug_assert!(
            rect.height <= target.height,
            "needs_resize exceeds area height"
        );
        if force || Some(rect) != current {
            return Some(rect);
        }
        None
    }

    pub fn render_area(&self, image: &DynamicImage, font_size: FontSize, available: Size) -> Size {
        let (width, height) = self.needs_resize_pixels(
            image,
            (available.width as u32) * (font_size.width as u32),
            (available.height as u32) * (font_size.height as u32),
        );
        Self::round_pixel_size_to_cells(width, height, font_size)
    }

    fn resize_image(&self, image: &DynamicImage, width: u32, height: u32) -> DynamicImage {
        const DEFAULT_FILTER_TYPE: FilterType = FilterType::Nearest;
        const DEFAULT_CROP_OPTIONS: CropOptions = CropOptions {
            clip_top: false,
            clip_left: false,
        };
        match self {
            Self::Fit(filter_type) | Self::Scale(filter_type) => {
                image.resize(width, height, filter_type.unwrap_or(DEFAULT_FILTER_TYPE))
            }
            Self::Crop(options) => {
                let options = options.as_ref().unwrap_or(&DEFAULT_CROP_OPTIONS);
                let y = if options.clip_top {
                    image.height().saturating_sub(height)
                } else {
                    0
                };
                let x = if options.clip_left {
                    image.width().saturating_sub(width)
                } else {
                    0
                };
                image.crop_imm(x, y, width, height)
            }
        }
    }

    fn needs_resize_pixels(&self, image: &DynamicImage, width: u32, height: u32) -> (u32, u32) {
        match self {
            Self::Fit(_) => fit_area_proportionally(
                image.width(),
                image.height(),
                min(width, image.width()),
                min(height, image.height()),
            ),

            Self::Crop(_) => (min(image.width(), width), min(image.height(), height)),
            Self::Scale(_) => fit_area_proportionally(image.width(), image.height(), width, height),
        }
    }

    /// Round an image pixel size to the nearest matching cell size, given a font size.
    fn round_pixel_size_to_cells(img_width: u32, img_height: u32, font_size: FontSize) -> Size {
        let width = (img_width as f32 / font_size.width as f32).ceil() as u16;
        let height = (img_height as f32 / font_size.height as f32).ceil() as u16;
        Size::new(width, height)
    }
}

/// Ripped from https://github.com/image-rs/image/blob/master/src/math/utils.rs#L12
/// Calculates the width and height an image should be resized to.
/// This preserves aspect ratio, and based on the `fill` parameter
/// will either fill the dimensions to fit inside the smaller constraint
/// (will overflow the specified bounds on one axis to preserve
/// aspect ratio), or will shrink so that both dimensions are
/// completely contained within the given `width` and `height`,
/// with empty space on one axis.
fn fit_area_proportionally(width: u32, height: u32, nwidth: u32, nheight: u32) -> (u32, u32) {
    let wratio = nwidth as f64 / width as f64;
    let hratio = nheight as f64 / height as f64;

    let ratio = f64::min(wratio, hratio);

    let nw = max((width as f64 * ratio).round() as u64, 1);
    let nh = max((height as f64 * ratio).round() as u64, 1);

    if nw > u64::from(u16::MAX) {
        let ratio = u16::MAX as f64 / width as f64;
        (u32::MAX, max((height as f64 * ratio).round() as u32, 1))
    } else if nh > u64::from(u16::MAX) {
        let ratio = u16::MAX as f64 / height as f64;
        (max((width as f64 * ratio).round() as u32, 1), u32::MAX)
    } else {
        (nw as u32, nh as u32)
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    use super::*;

    const FONT_SIZE: FontSize = FontSize::new(10, 10);

    fn s(w: u16, h: u16) -> DynamicImage {
        let image: DynamicImage =
            ImageBuffer::from_pixel(w as _, h as _, Rgba::<u8>([255, 0, 0, 255])).into();
        image
    }

    fn r(w: u16, h: u16) -> Size {
        Size::new(w, h)
    }

    #[test]
    fn needs_resize_fit() {
        let resize = Resize::Fit(None);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(10, 10)),
            r(10, 10),
            false,
        );
        assert_eq!(None, to);

        let to = resize.needs_resize(
            &s(101, 101),
            None,
            FONT_SIZE,
            Some(r(10, 10)),
            r(10, 10),
            false,
        );
        assert_eq!(None, to);

        let to = resize.needs_resize(
            &s(80, 100),
            None,
            FONT_SIZE,
            Some(r(8, 10)),
            r(10, 10),
            false,
        );
        assert_eq!(None, to);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(99, 99)),
            r(8, 10),
            false,
        );
        assert_eq!(Some(r(8, 8)), to);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(99, 99)),
            r(10, 8),
            false,
        );
        assert_eq!(Some(r(8, 8)), to);

        let to = resize.needs_resize(
            &s(100, 50),
            None,
            FONT_SIZE,
            Some(r(99, 99)),
            r(4, 4),
            false,
        );
        assert_eq!(Some(r(4, 2)), to);

        let to = resize.needs_resize(
            &s(50, 100),
            None,
            FONT_SIZE,
            Some(r(99, 99)),
            r(4, 4),
            false,
        );
        assert_eq!(Some(r(2, 4)), to);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(8, 8)),
            r(11, 11),
            false,
        );
        assert_eq!(Some(r(10, 10)), to);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(10, 10)),
            r(11, 11),
            false,
        );
        assert_eq!(None, to);
    }

    #[test]
    fn needs_resize_crop() {
        let resize = Resize::Crop(None);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(10, 10)),
            r(10, 10),
            false,
        );
        assert_eq!(None, to);

        let to = resize.needs_resize(
            &s(80, 100),
            None,
            FONT_SIZE,
            Some(r(8, 10)),
            r(10, 10),
            false,
        );
        assert_eq!(None, to);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(10, 10)),
            r(8, 10),
            false,
        );
        assert_eq!(Some(r(8, 10)), to);

        let to = resize.needs_resize(
            &s(100, 100),
            None,
            FONT_SIZE,
            Some(r(10, 10)),
            r(10, 8),
            false,
        );
        assert_eq!(Some(r(10, 8)), to);
    }
}
