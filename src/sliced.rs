//! Sliced image widget and protocol wrapper.
use crate::{
    FontSize, Resize,
    errors::Errors,
    picker::{Picker, ProtocolType},
    protocol::{Protocol, ProtocolTrait, halfblocks::Halfblocks, kitty::Kitty, sixel::Sixel},
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
    position: SignedPosition,
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
    /// # use ratatui_image::sliced::{SignedPosition, SlicedProtocol, SlicedImage};
    /// # let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))?;
    /// # let picker = Picker::halfblocks();
    /// let dyn_img = image::ImageReader::open("./assets/NixOS.png")?.decode()?;
    ///
    /// // This example would render the image at its actual pixel size.
    /// let sliced = SlicedProtocol::new(&picker, dyn_img, None)?;
    ///
    /// terminal.draw(|f| {
    ///     let position = SignedPosition::from((0, -3));
    ///     f.render_widget(SlicedImage::new(&sliced, position), f.area());
    /// });
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// The same works for e.g. ending N lines below viewport, or within any other inner area of
    /// the TUI.
    pub fn new(sliced_protocol: &'a SlicedProtocol, position: SignedPosition) -> SlicedImage<'a> {
        SlicedImage {
            sliced_protocol,
            position,
        }
    }

    fn skip_and_drop(size: Size, position: SignedPosition, area: Rect) -> Option<(usize, usize)> {
        if area.height == 0 || area.width == 0 {
            return None;
        }
        let top = position.y;
        let bottom = position.y + size.height as i16;
        let area_top = 0;
        let area_bottom = area.height as i16;

        if top >= area_bottom || bottom <= area_top {
            return None;
        }

        let skip = (area_top - top).max(0) as usize;
        let drop = (bottom - area_bottom).max(0) as usize;

        Some((skip, drop))
    }
}

impl Widget for SlicedImage<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let size = self.sliced_protocol.size();
        let Some((skip_line_count, drop_line_count)) =
            Self::skip_and_drop(size, self.position, area)
        else {
            return;
        };

        let x = self.position.x.max(0) as u16;
        let y = self.position.y.max(0) as u16;
        let image_area = Rect::new(
            area.x + x.min(area.width),
            area.y + y.min(area.height),
            (x + size.width).min(area.width) - x,
            size.height
                .saturating_sub((skip_line_count + drop_line_count) as u16),
        );

        match &self.sliced_protocol {
            SlicedProtocol::Kitty(kitty) => {
                kitty.render_with_skip(image_area, buf, skip_line_count);
            }
            SlicedProtocol::Sliced(slices) => {
                let mut image_area = image_area;
                image_area.height = 1;
                for slice in slices
                    .iter()
                    .skip(skip_line_count)
                    .take(slices.len().saturating_sub(drop_line_count))
                {
                    slice.render(image_area, buf);
                    image_area.y += 1;
                }
            }
            SlicedProtocol::Sixel(sliced_sixel) => {
                let sliced = sliced_sixel.borrow_dependent();
                sliced.render(image_area, buf, skip_line_count, drop_line_count);
            }
            SlicedProtocol::Halfblocks(halfblocks) => {
                halfblocks.render_with_skip(image_area, buf, skip_line_count, drop_line_count);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SignedPosition {
    pub x: i16,
    pub y: i16,
}

impl From<(i16, i16)> for SignedPosition {
    fn from((x, y): (i16, i16)) -> Self {
        Self { x, y }
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
        let size = size.unwrap_or_else(|| Resize::natural_size(&dyn_img, picker.font_size()));
        SlicedProtocol::new_with_resize(picker, dyn_img, size, Resize::Fit(None))
    }

    /// Create a `SlicedProtocol` for the target [`ratatui::layout::Size`] with the given
    /// [`Resize`] option.
    pub fn new_with_resize(
        picker: &Picker,
        dyn_img: DynamicImage,
        size: Size,
        resize: Resize,
    ) -> Result<SlicedProtocol, Errors> {
        match picker.protocol_type() {
            ProtocolType::Kitty => {
                let Protocol::Kitty(kitty) = picker.new_protocol(dyn_img, size, resize)? else {
                    unreachable!("ProtocolType::Kitty must produce Protocol::Kitty");
                };
                Ok(SlicedProtocol::Kitty(kitty))
            }
            ProtocolType::Sixel => {
                let font_size = picker.font_size();

                let dyn_img = resize.resize(&dyn_img, font_size, size, None);

                let sixel = Sixel::new(dyn_img, size, picker.is_tmux)?;

                let sliced = SlicedSixel::from_sixel(sixel, font_size.height, picker.is_tmux);

                Ok(SlicedProtocol::Sixel(sliced))
            }
            ProtocolType::Halfblocks => {
                let Protocol::Halfblocks(halfblocks) =
                    picker.new_protocol(dyn_img, size, resize)?
                else {
                    unreachable!("ProtocolType::Halfblocks must produce Protocol::Halfblocks");
                };
                Ok(SlicedProtocol::Halfblocks(halfblocks))
            }
            _ => {
                let (slices, image_size) = slice_rows(dyn_img, picker.font_size(), size);
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
}

/// Simply slices the DynamicImage into rows.
///
/// Could work for any protocol, but:
/// * Kitty would transmit multiple times.
/// * Halfblocks would not render as good with chafa.
/// * Sixel glitches in foot, would otherwise be okay.
///
/// So this only is used for Iterm2.
fn slice_rows(image: DynamicImage, font_size: FontSize, size: Size) -> (Vec<DynamicImage>, Size) {
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

/// Sixel "slicing" functions
///
/// Generated with an LLM, seems to work, it's just an implementation detail.
/// Sixel data consists of some start and end data, and in between are "bands" of sixels, which are
/// six pixel columns of data. Therefore it's easy to remove some sixel bands anywhere in the
/// image, for vertical clipping.
mod sixel_slice {
    use ratatui::layout::Size;
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
            skip_line_count: usize,
            drop_line_count: usize,
        ) {
            if self.size.width > area.width {
                return;
            }

            let data = self.to_sequence(skip_line_count, drop_line_count, area.width, area.height);
            sixel::render(&data, area, buf);
        }

        fn bands(&self, skip_line_count: usize, drop_line_count: usize) -> Vec<&str> {
            let skip_bands = (skip_line_count * self.font_height as usize).div_ceil(6);

            let bands: Vec<&str> = self.bands.to_vec();
            let take_bands = (((self.size.height.saturating_sub(drop_line_count as u16))
                * self.font_height)
                / 6) as usize;

            let sliced_bands: Vec<&str> = bands
                .iter()
                .skip(skip_bands)
                .take(take_bands)
                .copied()
                .collect();

            let trimmed = &sliced_bands[..sliced_bands
                .iter()
                .rposition(|s| !s.is_empty())
                .map(|i| i + 1)
                .unwrap_or(0)];

            trimmed.into()
        }

        pub fn to_sequence(
            &self,
            skip_line_count: usize,
            drop_line_count: usize,
            width: u16,
            height: u16,
        ) -> String {
            let (start, escape, end) = Parser::tmux_start_escape_end(self.is_tmux);

            let mut data = String::from(start);
            clear_area(&mut data, escape, width, height);
            data.push_str(self.header);

            let sliced_bands = self.bands(skip_line_count, drop_line_count);

            data.push_str(&sliced_bands.join("-"));

            if !sliced_bands.is_empty() {
                data.push('-');
            }
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
        use ratatui::layout::Size;

        use crate::{
            FontSize, Resize,
            picker::{Picker, ProtocolType},
            protocol::sixel::Sixel,
            sliced::{SlicedProtocol, sixel_slice::SlicedSixel},
        };

        #[test]
        fn test_sixel_slice_bands() {
            // TODO: is there always a `-` before `<esc>\`?
            let data = String::from("\x1b[6X\x1bPq\"1;1;8;16#0band1-band2-band3-\x1b\\");
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
                let dyn_img = Resize::Fit(None).resize(&dyn_img, font_size, size, None);
                let sixel = Sixel::new(dyn_img, size, false).unwrap();
                (p, SlicedSixel::from_sixel(sixel, font_size.height, false))
            });
            for (path, sliced_sixel) in sliced_sixels {
                let mut source = sliced_sixel.borrow_owner().data.as_str();
                source = source.strip_suffix("\x1b\\").unwrap();
                source = source.trim_end_matches('-');
                let source = format!("{source}-\x1b\\");

                let sliced_sixel_data = sliced_sixel.borrow_dependent();
                let sliced = sliced_sixel_data.to_sequence(0, 0, size.height, size.width);
                if sliced != source {
                    let mut surrounding = String::new();
                    for (i, char) in source.chars().enumerate() {
                        if surrounding.len() > 20 {
                            surrounding = surrounding.split_off(19);
                        }
                        surrounding.push(char);

                        let Some(sliced_char) = sliced.chars().nth(i) else {
                            panic!("sliced is shorter after {i}");
                        };
                        assert_eq!(
                            char,
                            sliced_char,
                            "{path} index #{i} (surrounding: \"{}\")",
                            surrounding.replace('\x1b', "<esc>")
                        );
                    }
                    panic!("should have found the first different char");
                }
            }
        }

        #[test]
        fn test_bands_from_image() {
            let image = image::ImageReader::open("./assets/Ada.png")
                .unwrap()
                .decode()
                .unwrap();

            assert_eq!(225, image.height());

            // Picker::halfblocks() font-height is hardcoded to 20.
            let mut picker = Picker::halfblocks();
            picker.set_protocol_type(ProtocolType::Sixel);

            let SlicedProtocol::Sixel(proto) = SlicedProtocol::new(&picker, image, None).unwrap()
            else {
                panic!("expected SlicedProtocol::Sixel");
            };
            let sixel = proto.borrow_owner();

            assert_eq!(12, sixel.size.height);

            let sliced = proto.borrow_dependent();

            // ceil(225 / 6) = 38, full image, no matter what font-size
            assert_eq!(38, sliced.bands(0, 0).len());

            // one row is 20px, so 3 bands make 18px
            assert_eq!(3, sliced.bands(0, 11).len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_and_drop() {
        struct TCase {
            y: i16,
            size: u16,                    // height
            area: u16,                    // height
            want: Option<(usize, usize)>, // (skip, drop)
        }
        for TCase {
            y,
            size,
            area,
            want,
        } in [
            TCase {
                y: -1,
                size: 12,
                area: 10,
                want: Some((1, 1)),
            },
            TCase {
                y: 0,
                size: 10,
                area: 10,
                want: Some((0, 0)),
            },
            TCase {
                y: 0,
                size: 5,
                area: 10,
                want: Some((0, 0)),
            },
            TCase {
                y: 2,
                size: 5,
                area: 10,
                want: Some((0, 0)),
            },
            TCase {
                y: -1,
                size: 10,
                area: 10,
                want: Some((1, 0)),
            },
            TCase {
                y: -5,
                size: 10,
                area: 10,
                want: Some((5, 0)),
            },
            TCase {
                y: 0,
                size: 20,
                area: 10,
                want: Some((0, 10)),
            },
            TCase {
                y: 5,
                size: 10,
                area: 10,
                want: Some((0, 5)),
            },
            TCase {
                y: 9,
                size: 1,
                area: 10,
                want: Some((0, 0)),
            },
            TCase {
                y: -2,
                size: 14,
                area: 10,
                want: Some((2, 2)),
            },
            TCase {
                y: -10,
                size: 10,
                area: 10,
                want: None,
            },
            TCase {
                y: 10,
                size: 10,
                area: 10,
                want: None,
            },
            TCase {
                y: 11,
                size: 10,
                area: 10,
                want: None,
            },
            TCase {
                y: 0,
                size: 1,
                area: 10,
                want: Some((0, 0)),
            },
            TCase {
                y: -1,
                size: 1,
                area: 10,
                want: None,
            },
            TCase {
                y: 10,
                size: 1,
                area: 10,
                want: None,
            },
        ] {
            assert_eq!(
                want,
                SlicedImage::skip_and_drop(
                    (100, size).into(),
                    (0, y).into(),
                    Rect::new(0, 0, 100, area),
                ),
                "position.y:{y}, size.y:{size}, area.height:{area}",
            );
        }
    }

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

        let (rows, image_size) = slice_rows(dyn_img, font_size, size);

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

        let (rows, image_size) = slice_rows(dyn_img, font_size, size);

        assert_eq!(rows.len(), 4); // 4 rows
        assert_eq!(image_size, Size::new(4, 4));
        // Each row should be 2 pixels tall (font height)
        for row in &rows {
            assert_eq!(row.height(), 2);
        }
    }
}
