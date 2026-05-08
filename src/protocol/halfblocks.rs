//! Halfblocks protocol implementations.
//!
//! Uses the unicode character `▀` combined with foreground and background color. Assumes that the
//! font aspect ratio is roughly 1:2. Should work in all terminals.
//!
//! If chafa is available, uses chafa for much richer rendering than primitive halfblocks:
//! - `chafa-static`: statically linked at compile time (requires static libchafa.a)
//! - `chafa-dyn`: dynamically linked at compile time via pkg-config

// Ensure only one chafa feature is enabled at a time
#[cfg(all(feature = "chafa-static", feature = "chafa-dyn"))]
compile_error!("features `chafa-static` and `chafa-dyn` are mutually exclusive");

use image::DynamicImage;
use ratatui::{
    buffer::{Buffer, Cell},
    layout::{Rect, Size},
    style::Color,
};

use super::{ProtocolTrait, StatefulProtocolTrait};
use crate::Result;

#[cfg(any(feature = "chafa-dyn", feature = "chafa-static",))]
mod chafa;

#[cfg(not(any(feature = "chafa-dyn", feature = "chafa-static",)))]
mod primitive;

#[derive(Clone, Default)]
pub struct Halfblocks {
    data: Vec<HalfBlock>,
    size: Size,
}

#[derive(Clone, Debug)]
pub struct HalfBlock {
    pub upper: Color,
    pub lower: Color,
    pub char: char,
}

impl HalfBlock {
    fn set_cell(&self, cell: &mut Cell) {
        cell.set_fg(self.upper)
            .set_bg(self.lower)
            .set_char(self.char);
    }
}

impl Halfblocks {
    /// Create a FixedHalfblocks from an image.
    ///
    /// The "resolution" is determined by the font size of the terminal. Smaller fonts will result
    /// in more half-blocks for the same image size. To get a size independent of the font size,
    /// the image could be resized in relation to the font size beforehand.
    /// Also note that the font-size is probably just some arbitrary size with a 1:2 ratio when the
    /// protocol is Halfblocks, and not the actual font size of the terminal.
    pub fn new(image: DynamicImage, size: Size) -> Result<Self> {
        let data = encode(&image, size);
        Ok(Self { data, size })
    }

    /// Specialized render for [`crate::sliced::SlicedImage`].
    pub(crate) fn render_with_skip(
        &self,
        area: Rect,
        buf: &mut Buffer,
        skip_line_count: usize,
        drop_line_count: usize,
    ) {
        let start = self.size.width as usize * skip_line_count;
        let end = start
            + self.size.width as usize
                * (self.size.height as usize - skip_line_count - drop_line_count);
        let hbs = &self.data[start..end];
        self.render_halfblocks(hbs, area, buf);
    }

    fn render_halfblocks(&self, hbs: &[HalfBlock], area: Rect, buf: &mut Buffer) {
        for (i, hb) in hbs.iter().enumerate() {
            let x = i as u16 % self.size.width;
            let y = i as u16 / self.size.width;
            if x >= area.width || y >= area.height {
                continue;
            }

            if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                hb.set_cell(cell);
            }
        }
    }
}

// chafa-static and chafa-dyn: always use chafa (no fallback needed/possible)
#[cfg(any(feature = "chafa-static", feature = "chafa-dyn"))]
fn encode(img: &DynamicImage, size: Size) -> Vec<HalfBlock> {
    chafa::encode(img, size).expect("chafa is always available with compile-time linking")
}

// no chafa feature: use primitive only
#[cfg(not(any(feature = "chafa-dyn", feature = "chafa-static")))]
fn encode(img: &DynamicImage, size: Size) -> Vec<HalfBlock> {
    primitive::encode(img, size)
}

impl ProtocolTrait for Halfblocks {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_halfblocks(&self.data, area, buf);
    }
    fn size(&self) -> Size {
        self.size
    }
}

impl StatefulProtocolTrait for Halfblocks {
    fn resize_encode(&mut self, img: DynamicImage, size: Size) -> Result<()> {
        let data = encode(&img, size);
        *self = Halfblocks { data, size };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use image::{Rgb, RgbImage};
    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend, layout::Size};

    use crate::{
        Image,
        protocol::{Protocol, halfblocks::Halfblocks},
    };

    #[test]
    fn render_image() {
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, Rgb([255, 0, 0])); // red
        img.put_pixel(1, 0, Rgb([0, 255, 0])); // green
        img.put_pixel(0, 1, Rgb([0, 0, 255])); // blue
        img.put_pixel(1, 1, Rgb([255, 255, 0])); // yellow

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal
            .draw(|frame| {
                let image = image::ImageReader::open("./assets/NixOS.png")
                    .unwrap()
                    .decode()
                    .unwrap();
                let area = Size::new(40, 20);
                let hbs = Halfblocks::new(image, area).unwrap();
                frame.render_widget(Image::new(&Protocol::Halfblocks(hbs)), frame.area());
            })
            .unwrap();

        #[cfg(any(feature = "chafa-static", feature = "chafa-dyn",))]
        {
            assert_snapshot!("chafa", terminal.backend());
        }
        #[cfg(not(any(feature = "chafa-static", feature = "chafa-dyn",)))]
        assert_snapshot!("halfblocks", terminal.backend());
        #[cfg(any(feature = "chafa-static", feature = "chafa-dyn",))]
        assert_snapshot!("chafa", terminal.backend());
    }
}
