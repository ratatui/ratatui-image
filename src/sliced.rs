//! Sliced image widget and protocol wrapper.
use crate::{
    FontSize, Resize,
    errors::Errors,
    picker::{Picker, ProtocolType},
    protocol::{
        ImageSource, Protocol, ProtocolTrait, halfblocks::Halfblocks, kitty::Kitty, sixel::Sixel,
    },
    sliced::sixel_slice::SlicedSixel,
};
use image::DynamicImage;
use ratatui::{
    layout::{Rect, Size},
    widgets::Widget,
};

/// An image "sliced" into rows for partially displaying, for example in vertical scrolling.
///
/// Uses a specialized [`SlicedProtocol`] with specialized operations based on the protocol.
pub struct SlicedImage<'a> {
    sliced_protocol: &'a SlicedProtocol,
    position: i16,
}
impl<'a> SlicedImage<'a> {
    /// Create a sliced image that will render with the given size at the given position.
    ///
    /// The position is relative to the `area` parameter of [`SlicedImage::render`], which is
    /// either a direct argument or stems from `frame.render_widget(w, area)`.
    ///
    /// Example that renders an image as if starting at 3 lines *above* the terminal viewport:
    ///
    /// ```rust
    /// # use ratatui_image::picker::Picker;
    /// # use ratatui::layout::Size;
    /// # use ratatui_image::sliced::{SlicedProtocol, SlicedImage};
    /// # let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))?;
    /// # let picker = Picker::halfblocks();
    /// let dyn_img = image::ImageReader::open("./assets/NixOS.png")?.decode()?;
    ///
    /// // This example would render the image at its actual pixel size.
    /// let sliced = SlicedProtocol::new(&picker, dyn_img, None)?;
    ///
    /// terminal.draw(|f| {
    ///     let position = -3;
    ///     f.render_widget(SlicedImage::new(&sliced, position), f.area());
    /// });
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// The same works for e.g. ending N lines below viewport, or within any other inner area of
    /// the TUI.
    pub fn new(sliced_protocol: &'a SlicedProtocol, position: i16) -> SlicedImage<'a> {
        SlicedImage {
            sliced_protocol,
            position,
        }
    }
}

impl Widget for SlicedImage<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        use crate::protocol::ProtocolTrait;

        let mut image_area: Rect = self.sliced_protocol.size().into();
        image_area.x = area.x;
        image_area.y = area.y;

        match &self.sliced_protocol {
            SlicedProtocol::Kitty(kitty) => {
                let skip_line_count = if self.position < 0 {
                    image_area.height = image_area
                        .height
                        .saturating_sub(self.position.unsigned_abs());
                    self.position.unsigned_abs()
                } else {
                    image_area.y += self.position as u16;
                    image_area.height =
                        (area.height - self.position.unsigned_abs()).min(kitty.size().height);
                    0
                };
                if image_area.height > 0 {
                    kitty.render_with_skip(image_area, buf, skip_line_count);
                }
            }
            SlicedProtocol::Sliced(proto_slice) => {
                let range = if self.position < 0 {
                    (self.position.unsigned_abs() as usize)..proto_slice.len()
                } else {
                    image_area.y += self.position.unsigned_abs();
                    0..((area.height - self.position.unsigned_abs()) as usize)
                        .min(proto_slice.len())
                };

                let mut area = image_area;
                area.height = 1;
                for i in range {
                    proto_slice[i].render(area, buf);
                    area.y += 1;
                }
            }
            SlicedProtocol::Sixel(sliced_sixel) => {
                let sixel = sliced_sixel.borrow_owner();
                let skip_line_count = if self.position < 0 {
                    image_area.height = image_area
                        .height
                        .saturating_sub(self.position.unsigned_abs());
                    self.position.unsigned_abs()
                } else {
                    image_area.y += self.position as u16;
                    image_area.height = (area.height.saturating_sub(self.position.unsigned_abs()))
                        .min(sixel.size().height);
                    0
                };
                if image_area.height > 0 {
                    if skip_line_count == 0 && image_area.height >= sixel.size().height {
                        sixel.render(image_area, buf);
                    } else {
                        let sliced = sliced_sixel.borrow_dependent();
                        sliced.render(image_area, buf, skip_line_count, image_area.height);
                    }
                }
            }
            SlicedProtocol::Halfblocks(halfblocks) => {
                let skip_line_count = if self.position < 0 {
                    image_area.height = image_area
                        .height
                        .saturating_sub(self.position.unsigned_abs());
                    self.position.unsigned_abs()
                } else {
                    image_area.y += self.position as u16;
                    image_area.height = (area.height.saturating_sub(self.position.unsigned_abs()))
                        .min(halfblocks.size().height);
                    0
                };

                halfblocks.render_with_skip(image_area, buf, skip_line_count);
            }
        }
    }
}

/// The sliced image for [`SlicedImage`].
///
/// Contains the sliced data specialized for the protocol.
pub enum SlicedProtocol {
    /// Generic, simply a list of image slices (or rows).
    /// Not suitable for Sixel, as the foot terminal has some striding glitch. In practice, this is
    /// only used for [`crate::protocol::iterm2::Iterm2`].
    Sliced(Vec<Protocol>),
    /// Takes full advantage of the unicode-placeholder mechanism.
    Kitty(Kitty),
    /// Strips sixel "bands" at render time to display only relevant parts, since the sixel format
    /// already is row based. Not pixel accurate, but good enough. Stores font-height to match
    /// against sixel "bands" height.
    ///
    /// TODO: deconstruct at encode-time instead of render-time.
    Sixel(SlicedSixel),
    /// Renders the full image (with chafa if available) for best ASCII art results, then just
    /// renders the relevant rows.
    Halfblocks(Halfblocks),
}

impl SlicedProtocol {
    /// Create a `SlicedProtocol` for the target [`ratatui::layout::Size`].
    ///
    /// If `size` is omitted, it will be calculated based on `dyn_img`'s image-pixel-size and
    /// `picker.font_size()`.
    pub fn new(
        picker: &Picker,
        dyn_img: DynamicImage,
        size: Option<Size>,
    ) -> Result<SlicedProtocol, Errors> {
        let size = size.unwrap_or_else(|| {
            let font_size = picker.font_size();
            Size::new(
                dyn_img.width().div_ceil(font_size.width as u32) as u16,
                dyn_img.height().div_ceil(font_size.height as u32) as u16,
            )
        });
        match picker.protocol_type() {
            ProtocolType::Kitty => {
                let Protocol::Kitty(kitty) =
                    picker.new_protocol(dyn_img, size, Resize::Fit(None))?
                else {
                    unreachable!("ProtocolType::Kitty must produce Protocol::Kitty");
                };
                Ok(SlicedProtocol::Kitty(kitty))
            }
            ProtocolType::Sixel => {
                let font_size = picker.font_size();
                let source = ImageSource::new(dyn_img, font_size, image::Rgba([0, 0, 0, 0]));
                let resize = Resize::Fit(None);

                let (dyn_img, _area) =
                    match resize.needs_resize(&source, font_size, source.desired, size, false) {
                        Some(area) => {
                            let dyn_img =
                                resize.resize(&source, font_size, area, image::Rgba([0, 0, 0, 0]));
                            (dyn_img, area)
                        }
                        None => (source.image, source.desired),
                    };

                let sixel = Sixel::new(dyn_img, size, picker.is_tmux)?;

                let sliced = SlicedSixel::from_sixel(sixel, font_size.height, picker.is_tmux);

                Ok(SlicedProtocol::Sixel(sliced))
            }
            ProtocolType::Halfblocks => {
                let Protocol::Halfblocks(halfblocks) =
                    picker.new_protocol(dyn_img, size, Resize::Fit(None))?
                else {
                    unreachable!("ProtocolType::Halfblocks must produce Protocol::Halfblocks");
                };
                Ok(SlicedProtocol::Halfblocks(halfblocks))
            }
            _ => {
                let (slices, image_size) = Self::slice_rows(dyn_img, picker.font_size(), size);
                let row_count = slices.len() as u16;
                let mut row_size = image_size;
                row_size.height /= row_count;
                let rows = slices
                    .into_iter()
                    .map(|row| picker.new_protocol_raw(row, row_size))
                    .collect::<Result<Vec<Protocol>, Errors>>()?;

                Ok(SlicedProtocol::Sliced(rows))
            }
        }
    }

    pub fn size(&self) -> Size {
        match self {
            SlicedProtocol::Sliced(protos) => Size::new(
                protos.first().map(|p| p.size().width).unwrap_or_default(),
                protos.len() as u16,
            ),
            SlicedProtocol::Halfblocks(hb) => hb.size(),
            SlicedProtocol::Kitty(kitty) => kitty.size(),
            SlicedProtocol::Sixel(sixel_slice) => sixel_slice.borrow_owner().size(),
        }
    }

    /// Simply slices the DynamicImage into rows.
    ///
    /// Could work for any protocol, but:
    /// * Kitty would transmit multiple times.
    /// * Halfblocks would not render as good with chafa.
    /// * Sixel glitches in foot, would otherwise be okay.
    ///
    /// So this only is used for Iterm2.
    fn slice_rows(
        image: DynamicImage,
        font_size: FontSize,
        size: Size,
    ) -> (Vec<DynamicImage>, Size) {
        let image = image.resize(
            (size.width * font_size.width).into(),
            (size.height * font_size.height).into(),
            image::imageops::FilterType::Nearest,
        );

        let height = image.height();
        let width = image.width();

        let row_count = (height as f64 / font_size.height as f64).ceil() as u16;
        let mut rows = Vec::new();

        let font_height = font_size.height as u32;
        for i in 0..row_count {
            let y = i as u32 * font_height;
            let row_height = font_height.min(height - y);
            let cropped = image.crop_imm(0, y, width, row_height);
            rows.push(cropped);
        }

        let col_count = (width as f64 / font_size.width as f64).ceil() as u16;
        (rows, Size::new(col_count, row_count))
    }
}

/// Sixel "slicing" functions
///
/// Generated with an LLM, seems to work, it's just an implementation detail.
/// Sixel data consists of some start and end data, and in between are "bands" of sixels, which are
/// six pixel columns of data. Therefore it's easy to remove some sixel bands anywhere in the
/// image, for vertical clipping.
mod sixel_slice {
    use std::cmp::min;

    use ratatui::layout::{Rect, Size};
    use self_cell::self_cell;

    use crate::{
        picker::cap_parser::Parser,
        protocol::{
            clear_area,
            sixel::{self, Sixel},
        },
    };

    self_cell!(
        pub struct SlicedSixel {
            owner: Sixel,
            #[covariant]
            dependent: SlicedSixelData,
        }
    );

    pub struct SlicedSixelData<'a> {
        size: Size,
        font_height: u16,
        is_tmux: bool,
        header: &'a str,
        bands: Vec<&'a str>,
    }
    impl<'a> SlicedSixelData<'a> {
        pub fn render(
            &self,
            area: ratatui::prelude::Rect,
            buf: &mut ratatui::prelude::Buffer,
            skip_line_count: u16,
            area_height: u16,
        ) {
            if self.size.width > area.width {
                return;
            }
            let area = Rect::new(
                area.x,
                area.y,
                min(self.size.width, area.width),
                min(self.size.height, area.height),
            );

            let data = self.to_sequence(skip_line_count, area_height, area.width);
            sixel::render(&data, area, buf);
        }

        pub fn to_sequence(&self, skip_line_count: u16, area_height: u16, width: u16) -> String {
            let (start, escape, end) = Parser::tmux_start_escape_end(self.is_tmux);

            let skip_bands = (skip_line_count * self.font_height).div_ceil(6) as usize;
            let take_bands = ((area_height * self.font_height) / 6) as usize;

            let available = self.bands.len().saturating_sub(skip_bands);
            let take_bands = take_bands.min(available);

            let mut data = String::from(start);
            clear_area(&mut data, escape, width, area_height);
            data.push_str(self.header);

            let sliced_bands: Vec<&str> = self
                .bands
                .iter()
                .skip(skip_bands)
                .take(take_bands)
                .copied()
                .collect();

            data.push_str(&sliced_bands.join("-"));

            if !sliced_bands.is_empty() {
                data.push('-');
            }
            data.push('-');
            data.push('\x1b');
            data.push('\\');
            data.push_str(end);

            data
        }
    }

    impl SlicedSixel {
        pub fn from_sixel(sixel: Sixel, font_height: u16, is_tmux: bool) -> SlicedSixel {
            SlicedSixel::new(sixel, |s| {
                let size = s.size;
                let dcs_start = s.data.find("\u{1b}P").unwrap_or(0);
                let data = &s.data[dcs_start..];
                let header_end = find_sixel_data_start(data);
                let (header, body) = data.split_at(header_end);
                let mut bands: Vec<&str> = body.split('-').collect();
                bands.pop();
                SlicedSixelData {
                    size,
                    font_height,
                    is_tmux,
                    header,
                    bands,
                }
            })
        }
    }

    fn find_sixel_data_start(data: &str) -> usize {
        let bytes = data.as_bytes();
        let mut i = 0;

        // Step 1: find ESC P
        while i + 1 < bytes.len() {
            if bytes[i] == 0x1B && bytes[i + 1] == b'P' {
                break;
            }
            i += 1;
        }

        // Step 2: skip past `q`
        while i < bytes.len() && bytes[i] != b'q' {
            i += 1;
        }
        if i < bytes.len() {
            i += 1;
        }

        // Step 3: skip raster attrs and color *definitions* only
        while i < bytes.len() {
            match bytes[i] {
                b'"' => {
                    // raster attribute line, skip to next `#` or sixel data char
                    i += 1;
                    while i < bytes.len()
                        && bytes[i] != b'#'
                        && bytes[i] != b'-'
                        && !(63..=126).contains(&bytes[i])
                    {
                        i += 1;
                    }
                }
                b'-' => break,
                b'#' => {
                    // peek ahead: is this `#digits;` (color def) or `#digits` followed by data?
                    let start = i;
                    i += 1;
                    // skip digits
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i < bytes.len() && bytes[i] == b';' {
                        // it's a color definition — skip the rest of it
                        while i < bytes.len()
                            && bytes[i] != b'#'
                            && bytes[i] != b'-'
                            && !(63..=126).contains(&bytes[i])
                        {
                            i += 1;
                        }
                    } else {
                        // it's a color selector in band data — rewind to the `#`, we're done
                        i = start;
                        break;
                    }
                }
                63..=126 => break, // sixel data character
                _ => i += 1,
            }
        }

        i
    }

    #[cfg(test)]
    mod tests {
        use image::Rgba;
        use ratatui::layout::Size;

        use crate::{
            FontSize, Resize,
            protocol::{ImageSource, sixel::Sixel},
            sliced::sixel_slice::SlicedSixel,
        };

        #[test]
        fn test_sixel_slice_bands() {
            // Simple data with bands separated by -
            // The slice function strips preamble, so we need ESC P in the data
            let esc = '\u{1b}';
            let bs = '\\';
            // Minimal sixel-like: ESC P q "attrs" header-bands-terminator ESC backslash
            // TODO: is there always a `-` before `<esc>\`?
            let data = format!("{esc}[6X{esc}Pq\"1;1;8;16#0band1-band2-band3-{esc}{bs}");
            // Skip 1 row, show 1 row, font height 6 means 1 band per row
            let sixel = Sixel {
                data,
                size: Size::default(),
                is_tmux: false,
            };
            let sliced = SlicedSixel::from_sixel(sixel, 6, false);
            let sliced = sliced.borrow_dependent();
            // band1 should be skipped, band2 should be present
            assert_eq!(sliced.bands, vec!["#0band1", "band2", "band3"]);
        }

        #[test]
        fn test_idempotence() {
            let images = [
                "./assets/Screenshot.png",
                "./assets/NixOS.png",
                "./assets/Ada.png",
            ];
            let size = Size::new(10, 10);
            let font_size = FontSize::new(8, 16);
            let sliced_sixels = images.map(|p| {
                let dyn_img = image::ImageReader::open(p).unwrap().decode().unwrap();
                let source = ImageSource::new(dyn_img, font_size, Rgba([0, 0, 0, 0]));
                let dyn_img =
                    Resize::Fit(None).resize(&source, font_size, size, Rgba([0, 0, 0, 0]));
                let sixel = Sixel::new(dyn_img, size, false).unwrap();
                SlicedSixel::from_sixel(sixel, font_size.height, false)
            });
            for sliced in sliced_sixels {
                let source = &sliced.borrow_owner().data;
                // let sliced = slice(&source, 0, size.height, font_size.height);
                let sliced = sliced
                    .borrow_dependent()
                    .to_sequence(0, size.height, size.width);
                eprintln!("source: {}", source.replace("\x1b", "<esc>"));
                eprintln!("sliced: {}", sliced.replace("\x1b", "<esc>"));
                if sliced != *source {
                    for (i, char) in source.chars().enumerate() {
                        let Some(sliced_char) = sliced.chars().nth(i) else {
                            panic!("sliced is shorter after {i}");
                        };
                        assert_eq!(char, sliced_char, "index #{i}");
                    }
                    panic!("should have found the first different char");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_rows_basic() {
        use image::RgbaImage;

        // Create a 4x4 image (4 pixels wide, 4 pixels tall)
        let mut img = RgbaImage::new(4, 4);
        for y in 0..4u32 {
            for x in 0..4u32 {
                img.put_pixel(x, y, image::Rgba([(x * 64) as u8, (y * 64) as u8, 0, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);

        let font_size = FontSize::new(1, 1); // 1x1 font means 1 row per pixel row
        let size = Size::new(4, 4);

        let (rows, image_size) = SlicedProtocol::slice_rows(dyn_img, font_size, size);

        assert_eq!(rows.len(), 4); // 4 rows
        assert_eq!(image_size, Size::new(4, 4));
        assert_eq!(rows[0].height(), 1);
        assert_eq!(rows[1].height(), 1);
        assert_eq!(rows[2].height(), 1);
        assert_eq!(rows[3].height(), 1);
    }

    #[test]
    fn test_slice_rows_font_height() {
        use image::RgbaImage;

        // Create a 4x8 image
        let mut img = RgbaImage::new(4, 8);
        for y in 0..8u32 {
            for x in 0..4u32 {
                img.put_pixel(x, y, image::Rgba([(x * 64) as u8, (y * 64) as u8, 0, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);

        let font_size = FontSize::new(1, 2); // font is 2 pixels tall
        let size = Size::new(4, 4); // 4 rows

        let (rows, image_size) = SlicedProtocol::slice_rows(dyn_img, font_size, size);

        assert_eq!(rows.len(), 4); // 4 rows
        assert_eq!(image_size, Size::new(4, 4));
        // Each row should be 2 pixels tall (font height)
        for row in &rows {
            assert_eq!(row.height(), 2);
        }
    }
}
