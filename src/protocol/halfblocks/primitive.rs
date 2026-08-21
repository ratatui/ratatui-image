//! Primitive halfblocks implementation using unicode character `▀`.

use image::DynamicImage;
use image::imageops::FilterType;
use ratatui::{layout::Size, style::Color};

use super::HalfBlock;

const HALF_UPPER: char = '▀';
const HALF_LOWER: char = '▄';
const SPACE: char = ' ';

pub fn encode(img: &DynamicImage, size: Size) -> Vec<HalfBlock> {
    let img = img.resize_exact(
        size.width as u32,
        u32::from(size.height) * 2,
        FilterType::Triangle,
    );

    let mut data = vec![
        HalfBlock {
            upper: Color::Rgb(0, 0, 0),
            lower: Color::Rgb(0, 0, 0),
            char: HALF_UPPER,
        };
        usize::from(size.width) * usize::from(size.height)
    ];

    for (y, row) in img.to_rgb8().rows().enumerate() {
        for (x, pixel) in row.enumerate() {
            let position = x + (size.width as usize) * (y / 2);
            if y % 2 == 0 {
                data[position].upper = Color::Rgb(pixel[0], pixel[1], pixel[2]);
            } else {
                data[position].lower = Color::Rgb(pixel[0], pixel[1], pixel[2]);
            }
        }
    }
    for hb in &mut data {
        hb.pick_side();
    }
    data
}

impl HalfBlock {
    fn pick_side(&mut self) {
        if self.upper == self.lower {
            self.char = SPACE;
            return;
        }
        if HalfBlock::luminance(HalfBlock::rgb(self.lower))
            > HalfBlock::luminance(HalfBlock::rgb(self.upper))
        {
            std::mem::swap(&mut self.upper, &mut self.lower);
            self.char = HALF_LOWER;
        }
    }

    fn luminance((r, g, b): (u8, u8, u8)) -> u32 {
        2126 * r as u32 + 7152 * g as u32 + 722 * b as u32
    }

    fn rgb(color: Color) -> (u8, u8, u8) {
        match color {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Indexed(i) => HalfBlock::indexed_to_rgb(i),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 0, 0),
            Color::Green => (0, 205, 0),
            Color::Yellow => (205, 205, 0),
            Color::Blue => (0, 0, 238),
            Color::Magenta => (205, 0, 205),
            Color::Cyan => (0, 205, 205),
            Color::Gray => (229, 229, 229),
            Color::DarkGray => (127, 127, 127),
            Color::LightRed => (255, 0, 0),
            Color::LightGreen => (0, 255, 0),
            Color::LightYellow => (255, 255, 0),
            Color::LightBlue => (92, 92, 255),
            Color::LightMagenta => (255, 0, 255),
            Color::LightCyan => (0, 255, 255),
            Color::White => (255, 255, 255),
            Color::Reset => (255, 255, 255), // assume light background, or pick a default
        }
    }

    fn indexed_to_rgb(i: u8) -> (u8, u8, u8) {
        match i {
            0..=15 => match i {
                0 => (0, 0, 0),
                1 => (205, 0, 0),
                2 => (0, 205, 0),
                3 => (205, 205, 0),
                4 => (0, 0, 238),
                5 => (205, 0, 205),
                6 => (0, 205, 205),
                7 => (229, 229, 229),
                8 => (127, 127, 127),
                9 => (255, 0, 0),
                10 => (0, 255, 0),
                11 => (255, 255, 0),
                12 => (92, 92, 255),
                13 => (255, 0, 255),
                14 => (0, 255, 255),
                15 => (255, 255, 255),
                _ => unreachable!(),
            },
            16..=231 => {
                // 6x6x6 color cube
                let i = i - 16;
                let r = (i / 36) % 6;
                let g = (i / 6) % 6;
                let b = i % 6;
                let to_val = |c: u8| if c == 0 { 0 } else { 55 + c * 40 };
                (to_val(r), to_val(g), to_val(b))
            }
            232..=255 => {
                // grayscale ramp
                let gray = 8 + (i - 232) * 10;
                (gray, gray, gray)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    /// A composite larger than `u16::MAX` cells must still encode.
    ///
    /// The buffer was allocated with `(size.width * size.height) as usize` — a
    /// `u16` multiply, which wraps — while the write index was computed as
    /// `x + (size.width as usize) * (y / 2)`, in `usize`, which does not. Below
    /// the ceiling the two agree exactly; one cell past it the allocation is made
    /// modulo 65536 while the loop still walks the true extent.
    ///
    /// Reported from a 458x144 pane (65952 cells): the allocation wrapped to 416
    /// and the encoder panicked with "index out of bounds: the len is 416 but the
    /// index is 416".
    #[test]
    fn encodes_a_grid_of_more_than_u16_max_cells() {
        let img: DynamicImage =
            ImageBuffer::from_pixel(64, 64, Rgba::<u8>([255, 0, 0, 255])).into();
        let size = Size::new(458, 144);
        assert!(
            u32::from(size.width) * u32::from(size.height) > u32::from(u16::MAX),
            "the case is only meaningful past the ceiling",
        );

        let data = encode(&img, size);

        assert_eq!(
            data.len(),
            458 * 144,
            "one entry per cell, not modulo 65536"
        );
    }
}
