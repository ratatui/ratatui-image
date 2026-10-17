use std::io::{self, IsTerminal, Read, Write};

use image::GenericImageView;
use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};
use ratatui::style::Color;
use ratatui::widgets::Widget;
use ratatui_image::picker::Picker;
use ratatui_image::picker::cap_parser::QueryStdioOptions;
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if io::stdin().is_terminal() {
        eprintln!("Expected image data on stdin, to demonstrate stdio file descriptor usage.");
        eprintln!("For example: cat assets/Jenkins.png | cargo run --example dump | cat");
        std::process::exit(1);
    }
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes)?;
    let image = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    eprint!("Image format: {:?}", image.format());
    let image = image.decode()?;
    eprintln!(", size: {:?}", image.dimensions());

    let picker = Picker::from_query_stdio_with_options(QueryStdioOptions {
        #[cfg(not(windows))]
        stdio: QueryStdioOptions::stdio_dev_tty().ok(),
        ..Default::default()
    })?;

    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let proto = picker.new_protocol(image, Size::new(cols, rows), Resize::Fit(None))?;
    let proto_size = proto.size();
    let area = Rect::new(0, 0, proto_size.width, proto_size.height);

    let mut buf = Buffer::empty(area);
    Image::new(&proto).render(area, &mut buf);

    let mut stdout = io::stdout().lock();

    match &proto {
        Protocol::Sixel(_) => {
            // Sixel written near the bottom causes the terminal to scroll once per line.
            // Pre-scroll by reserving the space first, then move cursor back.
            write!(
                stdout,
                "{}\x1b[{}A{}",
                "\n".repeat(area.height as usize),
                area.height,
                buf[(0u16, 0u16)].symbol()
            )?;
            writeln!(stdout)?;
        }
        _ => {
            // Translate ratatui buffer fg/bg colors to escape sequences.
            for y in 0..area.height {
                for x in 0..area.width {
                    let cell = &buf[(x, y)];
                    if let Color::Rgb(r, g, b) = cell.fg {
                        write!(stdout, "\x1b[38;2;{r};{g};{b}m")?;
                    }
                    if let Color::Rgb(r, g, b) = cell.bg {
                        write!(stdout, "\x1b[48;2;{r};{g};{b}m")?;
                    }
                    write!(stdout, "{}", cell.symbol())?;
                }
                writeln!(stdout, "\x1b[0m")?;
            }
        }
    }
    Ok(())
}
