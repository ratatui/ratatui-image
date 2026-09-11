//! Terminal stdio query parser module.
use std::fmt::Write;

use crate::picker::{ProtocolType, STDIN_READ_TIMEOUT_MILLIS};

pub struct Parser {
    data: String,
    sequence: ResponseParseState,
}

#[derive(Debug, PartialEq)]
pub enum ResponseParseState {
    Unknown,
    CSIResponse,
    OSCResponse,
    KittyResponse,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Response {
    Kitty,
    Sixel,
    RectangularOps,
    KittyCompression,
    KittySharedMemory,
    CellSize(Option<(u16, u16)>),
    CursorPositionReport(u16, u16),
    Background(u8, u8, u8),
    Status,
}

/// Extra query options
pub struct QueryStdioOptions {
    /// Timeout for the stdio query in milliseconds.
    pub timeout_ms: i32,
    /// Query for [Text Sizing Protocol]. The result can be checked by searching for
    /// [crate::picker::Capability::TextSizingProtocol] in [crate::picker::Picker::capabilities].
    ///
    /// [Text Sizing Protocol] <https://sw.kovidgoyal.net/kitty/text-sizing-protocol//>
    pub text_sizing_protocol: bool,
    /// Query the terminal background color. The result will be
    /// [`crate::picker::Capability::Background`] in the capabilities.
    ///
    /// This can be useful for sixels which have binary transparency instead of an alpha channel.
    pub terminal_background_color_osc: bool,
    /// Blacklist protocols from the detection query. Currently only kitty can be detected, so that
    /// is the only ProtocolType that can have any effect here.
    /// [`crate::picker::Picker`] currently sets ProtocolType::Kitty for WezTerm and Konsole.
    pub blacklist_protocols: Vec<ProtocolType>,
    /// Probe for, and use, kitty's `o=z` zlib transmission compression.
    ///
    /// **Off by default, and you probably want it off.** It optimises for
    /// bandwidth at the cost of render latency: every transmit is deflated
    /// first, which on a large photographic image costs tens to hundreds of
    /// milliseconds of CPU per (re)transmit, and the terminal must inflate it
    /// before it can draw. Turn it on when the link to the terminal is the
    /// bottleneck, such as over SSH, and the images compress well (flat
    /// colour, UI, pixel art).
    pub kitty_compression: bool,
    /// Probe POSIX shared memory objects for kitty image transmission instead of inline base64,
    /// and use it if available.
    ///
    /// The SHM filename is a 128 bit random number, which fits in 22 Base64 characters.
    /// This is below macos' 31 byte filename limit.
    ///
    /// The stdio query writes a one-pixel object under that name and asks the terminal to read it
    /// back with the protocol's own query action (`a=q`).
    ///
    /// [`crate::picker::Capability::KittySharedMemory`] is reported only where the terminal
    /// answered `OK`.
    ///
    /// See <https://sw.kovidgoyal.net/kitty/graphics-protocol/#the-transmission-medium>.
    ///
    /// The probe's own object is unlinked with a drop guard once its replies are read, regardless
    /// of the terminal's answer or behaviour.
    /// For a successful transmit, this is a no-op. If the terminal didn't unlink it, then it also
    /// should not have read it back.
    ///
    /// The filename scheme is defined at [`crate::protocol::kitty::shm_name`].
    pub kitty_shared_memory_object: Option<String>,
}

impl QueryStdioOptions {
    /// Public wrapper around [`crate::protocol::kitty::shm_name`].
    /// Used to configure [`QueryStdioOptions::kitty_shared_memory_object`].
    #[cfg(not(windows))]
    pub fn probe_kitty_smo() -> Option<String> {
        let filename = crate::protocol::kitty::generate_shm_name();
        Some(filename)
    }
}

impl Default for QueryStdioOptions {
    fn default() -> Self {
        Self {
            timeout_ms: STDIN_READ_TIMEOUT_MILLIS,
            text_sizing_protocol: false,
            terminal_background_color_osc: false,
            blacklist_protocols: Vec::new(),
            kitty_compression: false,
            kitty_shared_memory_object: None,
        }
    }
}

impl Default for Parser {
    fn default() -> Self {
        Parser {
            data: String::new(),
            sequence: ResponseParseState::Unknown,
        }
    }
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            data: String::new(),
            sequence: ResponseParseState::Unknown,
        }
    }

    /// Tmux requires escapes to be escaped, and some special start/end sequences.
    ///
    /// Returns start, escape, and end for tmux wrapping.
    pub fn tmux_start_escape_end(is_tmux: bool) -> (&'static str, &'static str, &'static str) {
        match is_tmux {
            false => ("", "\x1b", ""),
            true => ("\x1bPtmux;", "\x1b\x1b", "\x1b\\"),
        }
    }

    pub fn query(is_tmux: bool, options: QueryStdioOptions) -> String {
        let (start, escape, end) = Parser::tmux_start_escape_end(is_tmux);

        let mut buf = String::with_capacity(100);
        buf.push_str(start);

        if !options.blacklist_protocols.contains(&ProtocolType::Kitty) {
            // Kitty graphics
            write!(buf, "{escape}_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA{escape}\\").unwrap();

            if options.kitty_compression {
                // Kitty graphics transmission compression: the same one-pixel query
                // with the payload deflated and `o=z` set. Its own image id tells the
                // two replies apart. A successful reply will add `KittyCompression` to
                // the capabilities.
                const PROBE: &str = "eJxjYGAAAAADAAE="; // base64_simd::STANDARD.encode_to_string(zlib(&[0, 0, 0]))
                write!(
                    buf,
                    "{escape}_Gi=32,s=1,v=1,a=q,t=d,f=24,o=z;{PROBE}{escape}\\"
                )
                .unwrap();
            }

            // Kitty shared memory transmission: probed by writing the same one RGBA
            // pixel a real `t=s` transmit would, through the same two primitives
            // (`kitty::shm_name` + `kitty::shm_write`) a real transmit uses - not
            // `kitty::transmit_shm` itself, since that builds a real `a=T` display
            // escape and this probe needs the protocol's own `a=q` query shape
            // wrapped around the same object instead.
            // A successful reply adds `KittySharedMemory` to the capabilities.
            // The SMO itself is unlinked by the caller or the terminal.
            #[cfg(not(windows))]
            if let Some(filename) = options.kitty_shared_memory_object {
                const PIXEL: [u8; 4] = [0, 0, 0, 0];
                if crate::protocol::kitty::shm_write(&filename, &PIXEL).is_ok() {
                    write!(buf, "{escape}_Gi=33,s=1,v=1,a=q,t=s,f=32;").unwrap();
                    base64_simd::STANDARD.encode_append(filename.as_bytes(), &mut buf);
                    write!(buf, "{escape}\\").unwrap();
                }
            }
        }

        if !options.blacklist_protocols.contains(&ProtocolType::Sixel) {
            // Device Attributes Report 1 (sixel support)
            write!(buf, "{escape}[c").unwrap();
        }

        // Font size in pixels
        write!(buf, "{escape}[16t").unwrap();

        // iTerm2 proprietary, unknown response, untested so far.
        //write!(buf, "{escape}[1337n").unwrap();

        const BEL: &str = "\u{7}";

        if options.terminal_background_color_osc {
            // Background color
            write!(buf, "{escape}]11;?{BEL}").unwrap();
        }

        if options.text_sizing_protocol {
            // Send CPR (Cursor Position Report) and Text Sizing Protocol commands.
            // https://sw.kovidgoyal.net/kitty/text-sizing-protocol/#detecting-if-the-terminal-supports-this-protocol
            // We need to write a CPR, a resized space, and CPR again, to see if it moved the cursor
            // correctly with extra width.
            // Do it again for the scaling part of the protocol.
            // See [Picker::interpret_parser_responses] for how the responses are interpreted - it
            // differs slightly from the spec!
            write!(
                buf,
                "{escape}[6n{escape}]66;w=2; {BEL}{escape}[6n{escape}]66;s=2; {BEL}{escape}[6n"
            )
            .unwrap();
        }

        // End with Device Status Report, implemented by all terminals, ensure that there is some
        // response and we don't hang reading forever.
        write!(buf, "{escape}[5n").unwrap();

        write!(buf, "{end}").unwrap();
        buf
    }

    pub fn push(&mut self, next: char) -> Vec<Response> {
        match self.sequence {
            ResponseParseState::Unknown => {
                match (&self.data[..], next) {
                    (_, '\x1b') => {
                        // If the current sequence hasn't been identified yet, start a new one on Esc.
                        return self.restart();
                    }
                    ("_Gi=31" | "_Gi=32" | "_Gi=33", ';') => {
                        self.sequence = ResponseParseState::KittyResponse;
                    }

                    ("[", _) => {
                        self.sequence = ResponseParseState::CSIResponse;
                    }
                    ("]", _) => {
                        self.sequence = ResponseParseState::OSCResponse;
                    }
                    _ => {}
                };
                self.data.push(next);
            }
            ResponseParseState::CSIResponse => {
                if self.data == "[0" && next == 'n' {
                    self.restart();
                    return vec![Response::Status];
                }
                match next {
                    'c' if self.data.starts_with("[?") => {
                        let mut caps = vec![];
                        let inner: Vec<&str> = (self.data[2..]).split(';').collect();
                        for cap in inner {
                            match cap {
                                "4" => caps.push(Response::Sixel),
                                "28" => caps.push(Response::RectangularOps),
                                _ => {}
                            }
                        }
                        self.restart();
                        return caps;
                    }
                    't' => {
                        let mut cell_size = None;
                        let inner: Vec<&str> = self.data.split(';').collect();
                        if let [_, h, w] = inner[..] {
                            if let (Ok(h), Ok(w)) = (h.parse::<u16>(), w.parse::<u16>()) {
                                if w > 0 && h > 0 {
                                    cell_size = Some((w, h));
                                }
                            }
                        }
                        self.restart();
                        return vec![Response::CellSize(cell_size)];
                    }
                    'R' => {
                        let mut cursor_pos = None;
                        let inner: Vec<&str> = self.data[1..].split(';').collect();
                        if let [x, w] = inner[..] {
                            if let (Ok(x), Ok(y)) = (x.parse::<u16>(), w.parse::<u16>()) {
                                cursor_pos = Some((y, x));
                            }
                        }
                        if let Some((x, y)) = cursor_pos {
                            self.restart();
                            return vec![Response::CursorPositionReport(x, y)];
                        } else {
                            self.restart();
                            return vec![];
                        }
                    }
                    '\x1b' => {
                        // Give up?
                        return self.restart();
                    }
                    _ => {
                        self.data.push(next);
                    }
                };
            }
            ResponseParseState::OSCResponse => {
                self.data.push(next);
                if next == '\u{7}' || self.data.ends_with("\x1b\\") {
                    let Some(rgb) = self.data.split("rgb:").nth(1) else {
                        return self.restart();
                    };
                    let rgb = rgb.trim_matches(|c| c == '\x07' || c == '\x1b' || c == '\\');
                    let parts: Vec<&str> = rgb.split('/').collect();
                    if parts.len() != 3 {
                        return self.restart();
                    }
                    let (Some(r), Some(g), Some(b)) = (
                        u16::from_str_radix(parts[0], 16).ok(),
                        u16::from_str_radix(parts[1], 16).ok(),
                        u16::from_str_radix(parts[2], 16).ok(),
                    ) else {
                        return self.restart();
                    };
                    self.restart();
                    // Scale from 16-bit to 8-bit
                    return vec![Response::Background(
                        (r >> 8) as u8,
                        (g >> 8) as u8,
                        (b >> 8) as u8,
                    )];
                }
            }
            ResponseParseState::KittyResponse => match next {
                '\\' => {
                    let caps = match &self.data[..] {
                        "_Gi=31;OK\x1b" => vec![Response::Kitty],
                        "_Gi=32;OK\x1b" => vec![Response::KittyCompression],
                        "_Gi=33;OK\x1b" => vec![Response::KittySharedMemory],
                        _ => vec![],
                    };
                    self.restart();
                    return caps;
                }
                _ => {
                    self.data.push(next);
                }
            },
        };
        vec![]
    }
    fn restart(&mut self) -> Vec<Response> {
        self.data = String::new();
        self.sequence = ResponseParseState::Unknown;
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use super::{Parser, QueryStdioOptions, Response};

    fn parse(response: &str) -> Vec<Response> {
        let mut parser = Parser::new();
        let mut caps: Vec<Response> = vec![];
        for ch in response.chars() {
            let mut more_caps = parser.push(ch);
            caps.append(&mut more_caps)
        }
        caps
    }

    #[test]
    fn test_parse_all() {
        let caps =
            parse("\x1b_Gi=31;OK\x1b\\\x1b[?64;4c\x1b[6;7;14t\x1b[6;6R\x1b[7;7R\x1b[6;6R\x1b[0n");
        assert_eq!(
            caps,
            vec![
                Response::Kitty,
                Response::Sixel,
                Response::CellSize(Some((14, 7))),
                Response::CursorPositionReport(6, 6),
                Response::CursorPositionReport(7, 7),
                Response::CursorPositionReport(6, 6),
                Response::Status,
            ],
        );
    }

    #[test]
    fn test_parse_only_garbage() {
        let caps = parse("\x1bhonkey\x1btonkey\x1b[42\x1b\\");
        assert_eq!(caps, vec![]);
    }

    #[test]
    fn test_parse_preceding_garbage() {
        let caps = parse("\x1bgarbage...\x1b[?64;5c\x1b[0n");
        assert_eq!(caps, vec![Response::Status]);
    }

    #[test]
    fn test_parse_inner_garbage() {
        let caps = parse("\x1b[6;7;14t\x1bgarbage...\x1b[?64;5c\x1b[0n");
        assert_eq!(
            caps,
            vec![Response::CellSize(Some((14, 7))), Response::Status]
        );
    }

    // #[test]
    // fn test_parse_incomplete_support_in_text_sizing_protocol() {
    // let caps = parse("\x1b[6;7;14t\x1b[6;6R\x1b[7;7R\x1b[6;6R\x1b[0n");
    // assert_eq!(
    // caps,
    // vec![
    // Response::CellSize(Some((14, 7))),
    // Response::CursorPositionReport(6, 6),
    // Response::CursorPositionReport(7, 7),
    // Response::CursorPositionReport(6, 6),
    // Response::Status,
    // ],
    // );
    // }

    /// The compression probe is off by default: it costs nothing when the
    /// caller never asked for it, and `Capability::KittyCompression` can only
    /// ever appear where the probe was sent.
    #[test]
    fn test_query_omits_compression_probe_by_default() {
        let q = Parser::query(false, QueryStdioOptions::default());
        assert!(
            !q.contains("_Gi=32"),
            "the compression probe must be opt-in: {q}"
        );
    }

    /// The compression probe is the kitty probe with a deflated payload, and the
    /// terminal is asked rather than assumed: one that answers the plain query
    /// but cannot inflate would drop every compressed image on the floor.
    #[test]
    fn test_query_carries_a_compressed_probe_when_opted_in() {
        let q = Parser::query(
            false,
            QueryStdioOptions {
                kitty_compression: true,
                ..Default::default()
            },
        );
        let (_, rest) = q
            .split_once("\x1b_Gi=32,s=1,v=1,a=q,t=d,f=24,o=z;")
            .expect("the compressed probe is in the query");
        let (payload, _) = rest.split_once("\x1b\\").expect("the probe is terminated");

        // The payload has to be a real zlib stream of the one pixel `s=1,v=1`
        // and `f=24` promise, or the answer means nothing.
        let bytes = base64_simd::STANDARD
            .decode_to_vec(payload)
            .expect("the probe payload is base64");
        let mut raw = Vec::new();
        std::io::copy(&mut flate2::read::ZlibDecoder::new(&bytes[..]), &mut raw)
            .expect("the probe payload is one zlib stream");
        assert_eq!(raw, vec![0, 0, 0], "one RGB pixel, as f=24 s=1 v=1 says");
    }

    #[test]
    fn test_parse_compression_response() {
        assert_eq!(
            parse("\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;OK\x1b\\\x1b[0n"),
            vec![
                Response::Kitty,
                Response::KittyCompression,
                Response::Status
            ],
        );
    }

    /// A terminal that does kitty graphics but not compression answers the
    /// second probe with an error code, and must yield no capability at all —
    /// this is the case the whole probe exists for.
    #[test]
    fn test_parse_compression_refused() {
        assert_eq!(
            parse("\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;EINVAL:bad\x1b\\\x1b[0n"),
            vec![Response::Kitty, Response::Status],
        );
    }

    /// The shared memory probe is off by default, so
    /// `Capability::KittySharedMemory` can only ever appear where the probe was
    /// sent, and no object is created to clean up either.
    #[test]
    fn test_query_omits_shared_memory_probe_by_default() {
        let q = Parser::query(false, QueryStdioOptions::default());
        assert!(
            !q.contains("_Gi=33"),
            "the shared memory probe must be opt-in: {q}"
        );
    }

    /// The shared memory probe is the kitty probe with `t=s`, and its payload is
    /// the NAME of a real object this call just wrote a pixel into — not a
    /// placeholder — rather than pixels: a terminal on another machine cannot
    /// open it, which is the whole point of asking. `query` hands the name back
    /// so the caller can clean it up once the replies are read (see
    /// `QueryStdioOptions::kitty_shared_memory_object`'s doc).
    #[test]
    #[cfg(not(windows))]
    fn test_query_shared_memory_probe_writes_a_real_object_for_cleanup() {
        let probe =
            QueryStdioOptions::probe_kitty_smo().expect("this platform writes shared memory");
        let name = probe.clone();

        let q = Parser::query(
            false,
            QueryStdioOptions {
                kitty_shared_memory_object: Some(probe),
                ..Default::default()
            },
        );

        let (_, rest) = q
            .split_once("\x1b_Gi=33,s=1,v=1,a=q,t=s,f=32;")
            .expect("the shared memory probe is in the query");
        let (payload, _) = rest.split_once("\x1b\\").expect("the probe is terminated");
        let decoded = base64_simd::STANDARD
            .decode_to_vec(payload)
            .expect("the probe payload is base64");
        let decoded = String::from_utf8(decoded).expect("the probe payload is an object name");
        assert_eq!(
            decoded, name,
            "the escape names the exact object this call created"
        );
        assert!(
            name.starts_with("/rtui-"),
            "the same prefix a real transmit uses: {name}"
        );

        // The object is real, and holds the one RGBA pixel the probe promises
        // (`f=32,s=1,v=1`) — not merely a name nothing backs.
        let fd = rustix::shm::open(
            name.as_str(),
            rustix::shm::OFlags::RDONLY,
            rustix::fs::Mode::empty(),
        )
        .expect("the query wrote the object before naming it");
        let size = rustix::fs::fstat(&fd).expect("stat the object").st_size;
        assert!(size >= 4, "one RGBA pixel, got {size} bytes");
        drop(fd);

        // `query` only writes; cleanup is the caller's job (`Picker` unlinks it
        // once the query's replies are read). Play that part here.
        rustix::shm::unlink(name.as_str()).expect("the caller can unlink what query wrote");
    }

    #[test]
    fn test_parse_shared_memory_response() {
        assert_eq!(
            parse("\x1b_Gi=31;OK\x1b\\\x1b_Gi=33;OK\x1b\\\x1b[0n"),
            vec![
                Response::Kitty,
                Response::KittySharedMemory,
                Response::Status
            ],
        );
    }

    /// A terminal that cannot reach the object — anything remote — answers with
    /// an error, and must yield no capability: transmitting to it anyway would
    /// draw nothing at all.
    #[test]
    fn test_parse_shared_memory_refused() {
        assert_eq!(
            parse("\x1b_Gi=31;OK\x1b\\\x1b_Gi=33;EBADF:no such file\x1b\\\x1b[0n"),
            vec![Response::Kitty, Response::Status],
        );
    }
}
