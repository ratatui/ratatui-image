//! Protocol backends for the widgets

use std::{
    collections::hash_map::DefaultHasher,
    fmt::Write,
    hash::{Hash, Hasher},
    num::NonZeroU16,
};

use image::{DynamicImage, ImageBuffer, Rgba, imageops};
use ratatui::{
    buffer::{Buffer, CellDiffOption},
    layout::{Rect, Size},
};

use self::{
    halfblocks::Halfblocks,
    iterm2::Iterm2,
    kitty::{Kitty, StatefulKitty},
    sixel::Sixel,
};
use crate::{FontSize, ResizeEncodeRender, Result};

use super::Resize;

pub mod halfblocks;
pub mod iterm2;
pub mod kitty;
pub mod sixel;

const UNIT_WIDTH: CellDiffOption = CellDiffOption::ForcedWidth(NonZeroU16::new(1).unwrap());

pub(crate) trait ProtocolTrait: Send + Sync {
    /// Render the currently resized and encoded data to the buffer.
    fn render(&self, area: Rect, buf: &mut Buffer);

    // Get the size of the image.
    fn size(&self) -> Size;
}

trait StatefulProtocolTrait: ProtocolTrait {
    /// Resize the image and encode it for rendering. The result should be stored statefully so
    /// that next call for the given area does not need to redo the work.
    ///
    /// This can be done in a background thread, and the result is stored in this [StatefulProtocol].
    fn resize_encode(&mut self, img: DynamicImage, size: Size) -> Result<()>;
}

/// A fixed-size image protocol for the [crate::Image] widget.
#[derive(Clone)]
pub enum Protocol {
    Halfblocks(Halfblocks),
    Sixel(Sixel),
    Kitty(Kitty),
    ITerm2(Iterm2),
}

impl Protocol {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        let inner: &dyn ProtocolTrait = match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        };
        inner.render(area, buf);
    }
    // Get the size of the image.
    pub fn size(&self) -> Size {
        let inner: &dyn ProtocolTrait = match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        };
        inner.size()
    }

    /// Returns a placeholder area, if the image will not render into the given area, or `None`.
    ///
    /// The returned [`ratatui::layout::Rect`] is the area the image would cover, constrained by
    /// the size of `area` argument, if the image does not fit.
    ///
    /// Kitty and Halfblocks can always render partially, so they always return `None`.
    pub fn needs_placeholder(&self, area: Rect) -> Option<Rect> {
        let image_size = self.size();
        if area.width < image_size.width
            || area.height < image_size.height
                && (matches!(self, Self::Sixel(_)) || matches!(self, Self::Halfblocks(_)))
        {
            let mut placeholder_area = area;
            placeholder_area.width = placeholder_area.width.min(image_size.width);
            placeholder_area.height = placeholder_area.height.min(image_size.height);
            return Some(placeholder_area);
        }
        // Kitty and Halfblocks can render into a smaller area.
        None
    }
}

/// A stateful resizing image protocol for the [crate::StatefulImage] widget.
///
/// The [crate::thread::ThreadProtocol] widget also uses this, and is the reason why resizing is
/// split from rendering.
pub struct StatefulProtocol {
    source: ImageSource,
    font_size: FontSize,
    hash: u64,
    protocol_type: StatefulProtocolType,
    last_encoding_result: Option<Result<()>>,
}

#[derive(Clone)]
pub enum StatefulProtocolType {
    Halfblocks(Halfblocks),
    Sixel(Sixel),
    Kitty(StatefulKitty),
    ITerm2(Iterm2),
}

impl StatefulProtocolType {
    fn inner_trait(&self) -> &dyn StatefulProtocolTrait {
        match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        }
    }
    fn inner_trait_mut(&mut self) -> &mut dyn StatefulProtocolTrait {
        match self {
            Self::Halfblocks(halfblocks) => halfblocks,
            Self::Sixel(sixel) => sixel,
            Self::Kitty(kitty) => kitty,
            Self::ITerm2(iterm2) => iterm2,
        }
    }
}

impl StatefulProtocol {
    pub fn new(
        image: DynamicImage,
        font_size: FontSize,
        background_color: Option<Rgba<u8>>,
        protocol_type: StatefulProtocolType,
    ) -> Self {
        let source = ImageSource::new(image, font_size, background_color);
        Self {
            source,
            font_size,
            hash: u64::default(),
            protocol_type,
            last_encoding_result: None,
        }
    }

    // Calculate the area that this image will ultimately render to, inside the given area.
    pub fn size_for(&self, resize: Resize, size: Size) -> Size {
        resize.size_for(&self.source.image, self.font_size, size)
    }

    pub fn protocol_type(&self) -> &StatefulProtocolType {
        &self.protocol_type
    }

    pub fn protocol_type_owned(self) -> StatefulProtocolType {
        self.protocol_type
    }

    /// This returns the latest Result returned when encoding, and none if there was no encoding since the last result read. It is encouraged but not required to handle it
    pub fn last_encoding_result(&mut self) -> Option<Result<()>> {
        self.last_encoding_result.take()
    }

    // Get the background color that fills in when resizing.
    pub fn background_color(&self) -> Option<Rgba<u8>> {
        self.source.background_color
    }

    fn last_encoding_area(&self) -> Size {
        self.protocol_type.inner_trait().size()
    }
}

impl ResizeEncodeRender for StatefulProtocol {
    fn resize_encode(&mut self, resize: &Resize, size: Size) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        let img = resize.resize(
            &self.source.image,
            self.font_size,
            size,
            self.background_color(),
        );

        // TODO: save err in struct
        let result = self
            .protocol_type
            .inner_trait_mut()
            .resize_encode(img, size);

        if result.is_ok() {
            self.hash = self.source.hash
        }

        self.last_encoding_result = Some(result)
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.protocol_type.inner_trait_mut().render(area, buf);
    }

    fn needs_resize(&self, resize: &Resize, size: Size) -> Option<Size> {
        resize.needs_resize(
            &self.source.image,
            Some(self.source.desired),
            self.font_size,
            Some(self.last_encoding_area()),
            size,
            self.source.hash != self.hash,
        )
    }
}

#[derive(Clone)]
/// Image source for [crate::protocol::StatefulProtocol]s
///
/// A `[StatefulProtocol]` needs to resize the ImageSource to its state when the available area
/// changes. A `[Protocol]` only needs it once.
///
/// # Examples
/// ```text
/// use image::{DynamicImage, ImageBuffer, Rgb};
/// use ratatui_image::ImageSource;
///
/// let image: ImageBuffer::from_pixel(300, 200, Rgb::<u8>([255, 0, 0])).into();
/// let source = ImageSource::new(image, "filename.png", (7, 14));
/// assert_eq!((43, 14), (source.rect.width, source.rect.height));
/// ```
///
struct ImageSource {
    /// The original image without resizing.
    pub image: DynamicImage,
    /// The area that the [`ImageSource::image`] covers, but not necessarily fills.
    pub desired: Size,
    /// TODO: document this; when image changes but it doesn't need a resize, force a render.
    pub hash: u64,
    /// The background color that should be used for padding or background when resizing.
    pub background_color: Option<Rgba<u8>>,
}

impl ImageSource {
    /// Create a new image source
    pub fn new(
        mut image: DynamicImage,
        font_size: FontSize,
        background_color: Option<Rgba<u8>>,
    ) -> ImageSource {
        let desired = Resize::round_pixel_size_to_cells(image.width(), image.height(), font_size);

        let mut state = DefaultHasher::new();
        image.as_bytes().hash(&mut state);
        let hash = state.finish();

        // We only need to underlay the background color here if it's not completely transparent.
        if let Some(background_color) = background_color
            && background_color.0[3] != 0
        {
            let mut bg: DynamicImage =
                ImageBuffer::from_pixel(image.width(), image.height(), background_color).into();
            imageops::overlay(&mut bg, &image, 0, 0);
            image = bg;
        }

        ImageSource {
            image,
            desired,
            hash,
            background_color,
        }
    }
}

// Transparency needs explicit erasing of stale characters, or they stay behind the rendered
// image due to skipping of the following characters _in the terminal buffer_.
// DECERA does not work in WezTerm, however ECH and and cursor CUD and CUU do.
// For each line, erase `width` characters, then move back and place image.
pub(crate) fn clear_area(data: &mut String, escape: &str, width: u16, height: u16) {
    if height == 1 {
        // If the image is a single row then we don't need to move the cursor around at all.
        write!(data, "{escape}[{width}X").unwrap();
    } else {
        for _ in 0..height {
            write!(data, "{escape}[{width}X{escape}[1B").unwrap();
        }
        write!(data, "{escape}[{height}A").unwrap();
    }
}
