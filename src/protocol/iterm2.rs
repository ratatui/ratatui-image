//! ITerm2 protocol implementation.
//!
//! Delivers the full raw png image on every render.
use image::DynamicImage;
use ratatui::{
    buffer::{Buffer, CellDiffOption},
    layout::{Rect, Size},
};
use std::{cmp::min, fmt::Write, io::Cursor};

use crate::{Result, picker::cap_parser::Parser, protocol::UNIT_WIDTH};

use super::{ProtocolTrait, StatefulProtocolTrait, clear_area};

#[derive(Clone, Default)]
pub struct Iterm2 {
    pub data: String,
    pub size: Size,
    pub is_tmux: bool,
}

impl Iterm2 {
    pub fn new(image: DynamicImage, size: Size, is_tmux: bool) -> Result<Self> {
        let png = encode(&image, size, is_tmux)?;
        Ok(Self {
            data: png,
            size,
            is_tmux,
        })
    }
}

fn encode(img: &DynamicImage, size: Size, is_tmux: bool) -> Result<String> {
    let mut png: Vec<u8> = vec![];
    img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)?;

    let (start, escape, end) = Parser::tmux_start_escape_end(is_tmux);

    let width = size.width;
    let height = size.height;
    let mut seq = String::from(start);
    clear_area(&mut seq, escape, width, height);

    write!(
        seq,
        "{escape}]1337;File=inline=1;size={};width={}px;height={}px;doNotMoveCursor=1:",
        png.len(),
        img.width(),
        img.height(),
    )
    .unwrap();

    base64_simd::STANDARD.encode_append(&png, &mut seq);

    write!(seq, "\x07{end}").unwrap();
    Ok(seq)
}

impl ProtocolTrait for Iterm2 {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        render(self.size, &self.data, area, buf, false)
    }

    fn size(&self) -> Size {
        self.size
    }
}

fn render(size: Size, data: &str, area: Rect, buf: &mut Buffer, overdraw: bool) {
    let render_area = match render_area(size, area, overdraw) {
        None => {
            // If we render out of area, then the buffer will attempt to write regular text (or
            // possibly other sixels) over the image.
            //
            // Note that [StatefulProtocol] forces to ignore this early return, since it will
            // always resize itself to the area.
            return;
        }
        Some(r) => r,
    };

    if let Some(cell) = buf.cell_mut(render_area) {
        cell.set_symbol(data).set_diff_option(UNIT_WIDTH);
    }

    // Skip entire area (except first cell)
    for y in render_area.top()..render_area.bottom() {
        for x in render_area.left()..render_area.right() {
            if x == render_area.left() && y == render_area.top() {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_diff_option(CellDiffOption::Skip);
            }
        }
    }
}

fn render_area(size: Size, area: Rect, overdraw: bool) -> Option<Rect> {
    if overdraw {
        return Some(Rect::new(
            area.x,
            area.y,
            min(size.width, area.width),
            min(size.height, area.height),
        ));
    }

    if size.width > area.width || size.height > area.height {
        return None;
    }
    Some(Rect::new(area.x, area.y, size.width, size.height))
}

impl StatefulProtocolTrait for Iterm2 {
    fn resize_encode(&mut self, img: DynamicImage, size: Size) -> Result<()> {
        let data = encode(&img, size, self.is_tmux)?;
        *self = Iterm2 {
            data,
            size,
            ..*self
        };
        Ok(())
    }
}
