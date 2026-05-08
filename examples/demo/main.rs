#[cfg(all(
    not(feature = "crossterm"),
    not(feature = "termion"),
    not(feature = "termwiz")
))]
compile_error!("The demo needs one of the crossterm, termion, or termwiz features");

#[cfg(feature = "crossterm")]
mod crossterm;
#[cfg(feature = "termion")]
mod termion;
#[cfg(feature = "termwiz")]
mod termwiz;

use std::{env, error::Error, num::Wrapping as w, path::PathBuf, sync::Once, time::Duration};

use image::DynamicImage;
use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect, Size},
    style::{Color, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use ratatui_image::{
    Image, Resize, StatefulImage,
    picker::Picker,
    protocol::{Protocol, StatefulProtocol},
    sliced::{SignedPosition, SlicedImage, SlicedProtocol},
};

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "crossterm")]
    crate::crossterm::run()?;
    #[cfg(feature = "termion")]
    crate::termion::run()?;
    #[cfg(feature = "termwiz")]
    crate::termwiz::run()?;
    Ok(())
}

static READY: Once = Once::new();

#[derive(Debug, PartialEq, Eq)]
enum ShowImages {
    All,
    Fixed,
    Resized,
}

struct App {
    title: String,
    should_quit: bool,
    tick_rate: Duration,
    background: String,
    split_percent: u16,
    show_images: ShowImages,

    image_source_path: PathBuf,

    picker: Picker,
    image_source: DynamicImage,
    image_static: Protocol,
    image_fit_state: StatefulProtocol,
    image_crop_state: StatefulProtocol,
    image_scale_state: StatefulProtocol,
    image_sliced: SlicedProtocol,
    image_sliced_position: (SignedPosition, bool, bool), // (x,y, is_moving_rightwards, is_animating)
    image_sliced_viewport: Option<Size>,
}

fn size() -> Size {
    Size::new(30, 16)
}

impl App {
    pub fn new<B: Backend>(_: &mut Terminal<B>) -> Self {
        let title = format!(
            "Demo ({})",
            env::var("TERM").unwrap_or("unknown".to_string())
        );

        let image = if env::args().any(|arg| arg == "--tmp-demo-ready") {
            "./assets/Jenkins.png"
        } else {
            "./assets/Ada.png"
        };
        let image_source = image::ImageReader::open(image).unwrap().decode().unwrap();

        let picker = Picker::from_query_stdio().unwrap();

        let image_static = picker
            .new_protocol(image_source.clone(), size(), Resize::Fit(None))
            .expect("demo gets a protocol from image");
        let image_fit_state = picker.new_resize_protocol(image_source.clone());
        let image_crop_state = picker.new_resize_protocol(image_source.clone());
        let image_scale_state = picker.new_resize_protocol(image_source.clone());

        let image_sliced = SlicedProtocol::new(&picker, image_source.clone(), None).unwrap();

        let mut background = String::new();

        let mut r: [u64; 2] = [0x8a5cd789635d2dff, 0x121fd2155c472f96];
        for _ in 0..5_000 {
            let mut s1 = w(r[0]);
            let s0 = w(r[1]);
            let result = s0 + s1;
            r[0] = s0.0;
            s1 ^= s1 << 23;
            r[1] = (s1 ^ s0 ^ (s1 >> 18) ^ (s0 >> 5)).0;
            let c = match result.0 % 4 {
                0 => '.',
                1 => ' ',
                _ => '…',
            };
            background.push(c);
        }

        let image_sliced_position = (
            SignedPosition {
                x: 0,
                y: -((image_sliced.size().height / 2) as i16),
            },
            true,
            false,
        );

        Self {
            title,
            should_quit: false,
            tick_rate: Duration::from_millis(100),
            background,
            show_images: ShowImages::All,
            split_percent: 70,
            picker,
            image_source,
            image_source_path: image.into(),

            image_static,
            image_fit_state,
            image_crop_state,
            image_scale_state,
            image_sliced,
            image_sliced_position,
            image_sliced_viewport: None,
        }
    }
    pub fn on_key(&mut self, c: char) -> bool {
        match c {
            'q' => {
                self.should_quit = true;
            }
            't' => {
                self.show_images = match self.show_images {
                    ShowImages::All => ShowImages::Fixed,
                    ShowImages::Fixed => ShowImages::Resized,
                    ShowImages::Resized => ShowImages::All,
                }
            }
            'i' => {
                // Normally, we *never* would want to switch the detected protocol.
                // This is for some debug session, where you want to test some other protocol than
                // the detected.
                // Changing "live" is also quite hazardous, as this will render some artifacts in
                // between, or even trigger error messages, or crashes.
                // If you need to "downgrade" e.g. to Halfblocks, then do it before any renders.
                let next = self.picker.protocol_type().next();
                self.picker.set_protocol_type(next);
                self.reset_images();
            }
            'o' => {
                let path = match self.image_source_path.to_str() {
                    Some("./assets/Ada.png") => "./assets/Jenkins.png",
                    Some("./assets/Jenkins.png") => "./assets/NixOS.png",
                    _ => "./assets/Ada.png",
                };
                self.image_source = image::ImageReader::open(path).unwrap().decode().unwrap();
                self.image_source_path = path.into();
                self.reset_images();
            }
            'H' => {
                if self.split_percent >= 10 {
                    self.split_percent -= 10;
                }
            }
            'L' => {
                if self.split_percent <= 90 {
                    self.split_percent += 10;
                }
            }
            'h' => {
                let (pos, _, _) = &mut self.image_sliced_position;
                if pos.x > 0 {
                    pos.x -= 1;
                }
            }
            'j' => {
                let (pos, _, _) = &mut self.image_sliced_position;
                if let Some(viewport) = self.image_sliced_viewport
                    && (pos.y < 0 || (pos.y as u16) < viewport.height.saturating_sub(1))
                {
                    pos.y += 1;
                }
            }
            'k' => {
                let (pos, _, _) = &mut self.image_sliced_position;
                if pos.y > 0
                    || pos.y.unsigned_abs() < self.image_sliced.size().height.saturating_sub(1)
                {
                    pos.y -= 1;
                }
            }
            'l' => {
                let (pos, _, _) = &mut self.image_sliced_position;
                if let Some(viewport) = self.image_sliced_viewport
                    && pos.x
                        < (viewport
                            .width
                            .saturating_sub(self.image_sliced.size().width)
                            as i16)
                {
                    pos.x += 1;
                }
            }
            'a' => {
                self.image_sliced_position.2 = !self.image_sliced_position.2;
            }
            _ => {
                return false;
            }
        }
        true
    }

    fn reset_images(&mut self) {
        self.image_static = self
            .picker
            .new_protocol(self.image_source.clone(), size(), Resize::Fit(None))
            .unwrap();
        self.image_fit_state = self.picker.new_resize_protocol(self.image_source.clone());
        self.image_crop_state = self.picker.new_resize_protocol(self.image_source.clone());
        self.image_scale_state = self.picker.new_resize_protocol(self.image_source.clone());
        self.image_sliced =
            SlicedProtocol::new(&self.picker, self.image_source.clone(), None).unwrap();
    }

    pub fn on_tick(&mut self) -> bool {
        READY.call_once(|| {
            // This is normally only set by nixosTest.
            if env::args().any(|arg| arg == "--tmp-demo-ready") {
                if let Err(err) = std::fs::File::create("/tmp/demo-ready") {
                    panic!("{err}");
                }
            }
        });

        if let Some(viewport) = self.image_sliced_viewport {
            let (pos, is_moving_rightwards, is_animating) = &mut self.image_sliced_position;
            if *is_animating {
                pos.y += 1;
                if pos.y > 0 && pos.y.unsigned_abs() > viewport.height {
                    pos.y = -10;
                }

                if *is_moving_rightwards {
                    if pos.x
                        >= viewport
                            .width
                            .saturating_sub(self.image_sliced.size().width)
                            as i16
                    {
                        pos.x = viewport
                            .width
                            .saturating_sub(self.image_sliced.size().width + 1)
                            as i16;
                        *is_moving_rightwards = false;
                    } else {
                        pos.x += 1;
                    }
                } else {
                    pos.x = pos.x.saturating_sub(1);
                    if pos.x == 0 {
                        *is_moving_rightwards = true;
                    }
                }
                return true;
            }
        }
        false
    }

    fn render_resized_image(&mut self, f: &mut Frame<'_>, resize: Resize, area: Rect) {
        let (state, name, color) = match resize {
            Resize::Fit(_) => (&mut self.image_fit_state, "Fit", Color::Magenta),
            Resize::Crop(_) => (&mut self.image_crop_state, "Crop", Color::Green),
            Resize::Scale(_) => (&mut self.image_scale_state, "Scale", Color::Blue),
        };
        let block = block(name);
        let inner_area = block.inner(area);
        f.render_widget(paragraph(self.background.as_str().bg(color)), inner_area);
        if self.show_images != ShowImages::Fixed {
            f.render_stateful_widget(StatefulImage::new().resize(resize), inner_area, state);
        }
        f.render_widget(block, area);
    }
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let outer_block = Block::default()
        .borders(Borders::TOP)
        .title(app.title.as_str());

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.split_percent),
            Constraint::Percentage(100 - app.split_percent),
        ])
        .split(outer_block.inner(f.area()));
    f.render_widget(outer_block, f.area());

    let left_chunks = vertical_layout().split(chunks[0]);
    let right_chunks = vertical_layout().split(chunks[1]);

    let chunks_left_top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(left_chunks[0]);

    let block_left_top = block("Fixed");
    let area = block_left_top.inner(chunks_left_top[0]);
    f.render_widget(
        paragraph(app.background.as_str()).style(Color::Yellow),
        area,
    );
    f.render_widget(block_left_top, chunks_left_top[0]);
    if app.show_images != ShowImages::Resized {
        // Let it be surrounded by styled text.
        let area = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };

        if let Some(placeholder_area) = app.image_static.needs_placeholder(area) {
            let placeholder = Block::bordered()
                .border_type(BorderType::QuadrantOutside)
                .bg(Color::DarkGray);
            f.render_widget(Clear {}, placeholder.inner(placeholder_area));
            f.render_widget(placeholder, placeholder_area);
        } else {
            let image = Image::new(&app.image_static).allow_clipping(true);
            f.render_widget(image, area);
        }
    }

    let block_middle_top = block("Sliced");
    let area = block_middle_top.inner(chunks_left_top[1]);
    app.image_sliced_viewport = Some(area.into());
    f.render_widget(
        paragraph(app.background.as_str()).style(Color::LightBlue),
        area,
    );
    f.render_widget(block_middle_top, chunks_left_top[1]);
    if app.show_images != ShowImages::Resized {
        let (pos, _, _) = app.image_sliced_position;
        let image = SlicedImage::new(&app.image_sliced, pos);
        f.render_widget(image, area);
    }

    let chunks_left_bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(left_chunks[1]);

    app.render_resized_image(f, Resize::Crop(None), chunks_left_bottom[0]);
    app.render_resized_image(f, Resize::Scale(None), chunks_left_bottom[1]);
    app.render_resized_image(f, Resize::Fit(None), right_chunks[0]);

    let block_right_bottom = block("Help");
    let area = block_right_bottom.inner(right_chunks[1]);
    f.render_widget(
        paragraph(vec![
            Line::from(format!(
                "Font size: {}×{}",
                app.picker.font_size().width,
                app.picker.font_size().height
            )),
            Line::from(format!("Protocol: {:?}", app.picker.protocol_type())),
            Line::from("Key bindings:"),
            Line::from(vec![
                Span::from("H").green(),
                Span::from("/"),
                Span::from("L").green(),
                Span::from(": resize panes"),
            ]),
            Line::from(vec![Span::from("o").green(), Span::from(": cycle image")]),
            Line::from(vec![
                Span::from("t").green(),
                Span::from(format!(": toggle ({:?})", app.show_images)),
            ]),
            Line::from(vec![
                Span::from("h").green(),
                Span::from("/"),
                Span::from("j").green(),
                Span::from("/"),
                Span::from("k").green(),
                Span::from("/"),
                Span::from("l").green(),
                Span::from(": move"),
            ]),
            Line::from(vec![
                Span::from("a").green(),
                Span::from(": toggle animation"),
            ]),
        ]),
        area,
    );
    f.render_widget(block_right_bottom, right_chunks[1]);
}

fn paragraph<'a, T: Into<Text<'a>>>(str: T) -> Paragraph<'a> {
    Paragraph::new(str).wrap(Wrap { trim: true })
}

fn vertical_layout() -> Layout {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
}

fn block(name: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(name)
}
