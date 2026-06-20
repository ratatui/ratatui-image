//! Chafa-based halfblocks implementation using compile-time linking.
//!
//! This module uses compile-time linking to libchafa.so (dyn) or libchafa.a (static).

use std::ffi::c_void;
use std::sync::OnceLock;

use image::DynamicImage;
use ratatui::{layout::Size, style::Color};

use super::HalfBlock;

// Opaque pointer types (same as dynamic version)
type ChafaSymbolMap = *mut c_void;
type ChafaCanvasConfig = *mut c_void;
type ChafaCanvas = *mut c_void;

// Constants from chafa.h
const CHAFA_SYMBOL_TAG_ALL: u32 = 0xBFE7FFFF;
const CHAFA_PIXEL_RGB8: u32 = 8;

// FFI declarations - linked via build.rs (static or dynamic based on feature)
#[cfg_attr(feature = "chafa-dyn", link(name = "chafa"))]
unsafe extern "C" {
    fn chafa_symbol_map_new() -> ChafaSymbolMap;
    fn chafa_symbol_map_add_by_tags(symbol_map: ChafaSymbolMap, tags: u32);
    fn chafa_symbol_map_unref(symbol_map: ChafaSymbolMap);
    fn chafa_canvas_config_new() -> ChafaCanvasConfig;
    fn chafa_canvas_config_set_symbol_map(config: ChafaCanvasConfig, symbol_map: ChafaSymbolMap);
    fn chafa_canvas_config_set_geometry(config: ChafaCanvasConfig, width: i32, height: i32);
    fn chafa_canvas_config_unref(config: ChafaCanvasConfig);
    fn chafa_canvas_new(config: ChafaCanvasConfig) -> ChafaCanvas;
    fn chafa_canvas_draw_all_pixels(
        canvas: ChafaCanvas,
        pixel_type: u32,
        pixels: *const u8,
        width: i32,
        height: i32,
        rowstride: i32,
    );
    fn chafa_canvas_get_char_at(canvas: ChafaCanvas, x: i32, y: i32) -> u32;
    fn chafa_canvas_get_colors_at(canvas: ChafaCanvas, x: i32, y: i32, fg: *mut i32, bg: *mut i32);
    fn chafa_canvas_unref(canvas: ChafaCanvas);
}

/// Holds the cached symbol map for reuse across encode calls.
struct ChafaState {
    symbol_map: ChafaSymbolMap,
}

// SAFETY: The chafa library functions are thread-safe for independent canvases.
// The symbol_map is created once and only read afterwards.
unsafe impl Send for ChafaState {}
unsafe impl Sync for ChafaState {}

impl Drop for ChafaState {
    fn drop(&mut self) {
        unsafe {
            chafa_symbol_map_unref(self.symbol_map);
        }
    }
}

static CHAFA: OnceLock<ChafaState> = OnceLock::new();

fn init_chafa() -> ChafaState {
    unsafe {
        let symbol_map = chafa_symbol_map_new();
        chafa_symbol_map_add_by_tags(symbol_map, CHAFA_SYMBOL_TAG_ALL);
        ChafaState { symbol_map }
    }
}

/// Encode using chafa.
pub fn encode(img: &DynamicImage, size: Size) -> Option<Vec<HalfBlock>> {
    let chafa = CHAFA.get_or_init(init_chafa);

    let width = size.width;
    let height = size.height;

    unsafe {
        let config = chafa_canvas_config_new();
        chafa_canvas_config_set_symbol_map(config, chafa.symbol_map);
        chafa_canvas_config_set_geometry(config, width as i32, height as i32);

        let canvas = chafa_canvas_new(config);

        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();

        chafa_canvas_draw_all_pixels(
            canvas,
            CHAFA_PIXEL_RGB8,
            rgb.as_ptr(),
            w as i32,
            h as i32,
            (w * 3) as i32,
        );

        let mut blocks = Vec::with_capacity(width as usize * height as usize);

        for y in 0..height {
            for x in 0..width {
                let c = chafa_canvas_get_char_at(canvas, x as i32, y as i32);
                let symbol = char::from_u32(c).unwrap_or(' ');

                let mut fg_color: i32 = 0;
                let mut bg_color: i32 = 0;
                chafa_canvas_get_colors_at(
                    canvas,
                    x as i32,
                    y as i32,
                    &mut fg_color,
                    &mut bg_color,
                );

                let fg = Color::Rgb(
                    ((fg_color >> 16) & 0xff) as u8,
                    ((fg_color >> 8) & 0xff) as u8,
                    (fg_color & 0xff) as u8,
                );
                let bg = Color::Rgb(
                    ((bg_color >> 16) & 0xff) as u8,
                    ((bg_color >> 8) & 0xff) as u8,
                    (bg_color & 0xff) as u8,
                );

                blocks.push(HalfBlock {
                    upper: fg,
                    lower: bg,
                    char: symbol,
                });
            }
        }

        chafa_canvas_unref(canvas);
        chafa_canvas_config_unref(config);

        Some(blocks)
    }
}
