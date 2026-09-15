//! Kitty protocol.
//!
//! Transmits the image once on first render, tracked via an AtomicBool, and then subsequentially
//! renders the image with the [unicode-placeholders] feature of the [kitty protocol].
//!
//! [unicode-placeholders]: https://sw.kovidgoyal.net/kitty/graphics-protocol/#unicode-placeholders
//! [kitty protocol]: https://sw.kovidgoyal.net/kitty/graphics-protocol
use std::borrow::Cow;
use std::fmt::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(any(windows, target_os = "linux")))]
use rustix::mm::{MapFlags, ProtFlags, mmap, munmap};
#[cfg(not(windows))]
use rustix::{
    fs::Mode,
    shm::{self, OFlags as ShmOFlags},
};

use crate::protocol::UNIT_WIDTH;
use crate::{Result, picker::cap_parser::Parser};
use image::DynamicImage;
use ratatui::layout::Size;
use ratatui::style::Color;
use ratatui::{buffer::Buffer, layout::Rect};

use super::{ProtocolTrait, StatefulProtocolTrait};

/// The character kitty reserves for image placeholders.
const PLACEHOLDER: char = '\u{10EEEE}';

#[derive(Default, Clone)]
struct KittyProtoState {
    transmitted: Arc<AtomicBool>,
    transmit_str: Option<String>,
    id: (u32, Color, u16), // Full ID, ID as fg color, ID extra part for diacritic
}

impl KittyProtoState {
    fn new(img: &DynamicImage, id: u32, is_tmux: bool, compress: bool, smo: bool) -> Result<Self> {
        // The only caller that holds a `DynamicImage`: every transmit path below
        // takes raw RGBA bytes plus dimensions, so the conversion happens once,
        // here, rather than once per path.
        let img_rgba8 = img.to_rgba8();
        let transmit_str = transmit_or_shm(
            img_rgba8.as_raw(),
            img.width(),
            img.height(),
            id,
            is_tmux,
            compress,
            smo,
        )?;
        let [id_extra, id_r, id_g, id_b] = id.to_be_bytes();
        Ok(Self {
            transmitted: Arc::new(AtomicBool::new(false)),
            transmit_str: Some(transmit_str),
            id: (id, Color::Rgb(id_r, id_g, id_b), u16::from(id_extra)),
        })
    }

    // Produce the transmit sequence or None if it has already been produced before.
    fn make_transmit(&self) -> Option<&str> {
        let transmitted = self.transmitted.swap(true, Ordering::SeqCst);

        if transmitted {
            None
        } else {
            self.transmit_str.as_deref()
        }
    }
}

#[derive(Clone, Default)]
pub struct Kitty {
    proto_state: KittyProtoState,
    size: Size,
}

impl Kitty {
    pub fn new(
        image: DynamicImage,
        size: Size,
        id: u32,
        is_tmux: bool,
        compress: bool,
        smo: bool,
    ) -> Result<Self> {
        let proto_state = KittyProtoState::new(&image, id, is_tmux, compress, smo)?;
        Ok(Self { proto_state, size })
    }

    /// Only for SlicedImage
    pub(crate) fn render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_line_count: usize) {
        // Transmit only once. This is why self is mut.
        let seq = self.proto_state.make_transmit();

        render(
            area,
            self.size,
            buf,
            &self.proto_state.id,
            seq,
            skip_line_count,
        );
    }
}

impl ProtocolTrait for Kitty {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Transmit only once, track at this point via the AtomicBool in proto_state.
        let seq = self.proto_state.make_transmit();

        render(area, self.size, buf, &self.proto_state.id, seq, 0);
    }

    fn size(&self) -> Size {
        self.size
    }
}

#[derive(Clone)]
pub struct StatefulKitty {
    id: (u32, Color, u16), // Full ID, ID as fg color, ID extra part for diacritic
    size: Size,
    proto_state: KittyProtoState,
    is_tmux: bool,
    compress: bool,
    smo: bool,
}

impl StatefulKitty {
    pub fn new(id: u32, is_tmux: bool, compress: bool, smo: bool) -> StatefulKitty {
        let [id_extra, id_r, id_g, id_b] = id.to_be_bytes();
        StatefulKitty {
            id: (id, Color::Rgb(id_r, id_g, id_b), u16::from(id_extra)),
            size: Size::default(),
            proto_state: KittyProtoState::default(),
            is_tmux,
            compress,
            smo,
        }
    }
}

impl ProtocolTrait for StatefulKitty {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Transmit only once. This is why self is mut.
        let seq = self.proto_state.make_transmit();

        render(area, self.size, buf, &self.id, seq, 0);
    }

    fn size(&self) -> Size {
        self.size
    }
}

impl StatefulProtocolTrait for StatefulKitty {
    fn resize_encode(&mut self, img: DynamicImage, size: Size) -> Result<()> {
        self.size = size;
        // If resized then we must transmit again.
        self.proto_state =
            KittyProtoState::new(&img, self.id.0, self.is_tmux, self.compress, self.smo)?;
        Ok(())
    }
}

fn render(
    area: Rect,
    size: Size,
    buf: &mut Buffer,
    (_, fg, id_extra): &(u32, Color, u16),
    mut seq: Option<&str>,
    skip_line_count: usize,
) {
    // A position past the end of the diacritic table cannot be encoded, so
    // those cells are left undrawn rather than shown at the wrong offset.
    // Columns need this as much as rows now that each one is explicit.
    let max = DIACRITICS.len() as u16;
    let width = area.width.min(size.width).min(max);
    let height = area.height.min(size.height).min(max);

    let mut symbol = String::with_capacity(64);
    for y in 0..height {
        let row = y + skip_line_count as u16;
        for x in 0..width {
            let Some(cell) = buf.cell_mut((area.left() + x, area.top() + y)) else {
                continue;
            };

            symbol.clear();
            // Only transmit once. In `ProtocolTrait::render()` an `AtomicBool`
            // is marked.
            // Of course, this does not actually mean "rendered to terminal",
            // but rather "has been returned to user once for render".
            if let Some(seq) = seq.take() {
                symbol.push_str(seq);
            }

            symbol.push(PLACEHOLDER);
            if x == 0 {
                symbol.push(diacritic(row));
                symbol.push(diacritic(x));
                symbol.push(diacritic(*id_extra));
            }

            cell.set_symbol(&symbol)
                .set_fg(*fg)
                .set_diff_option(UNIT_WIDTH);
        }
    }
}

/// Deflate `raw` into the RFC 1950 zlib stream the kitty protocol's `o=z` means.
///
/// Level 1: on photographic content it encodes about 3x faster than the
/// default level 6 for about 7% more bytes; on flat 16-colour art it's also
/// about 3x faster but produces roughly TWICE the bytes level 6 would (e.g.
/// 43 KB vs 103 KB base64 for a 720x570 frame) — still 20x+ smaller than the
/// raw 2.2 MB. The encode runs synchronously in `new_protocol`, so encode
/// time, not wire size, is the cost that matters here.
fn zlib(raw: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    // Writing into a `Vec` cannot fail, and neither can finishing one.
    enc.write_all(raw).expect("zlib encoder writing into a Vec");
    enc.finish().expect("zlib encoder writing into a Vec")
}

#[cfg_attr(windows, allow(unused_variables))]
fn transmit_or_shm(
    bytes: &[u8],
    w: u32,
    h: u32,
    id: u32,
    is_tmux: bool,
    compress: bool,
    smo: bool,
) -> Result<String> {
    #[cfg(not(windows))]
    if smo {
        return transmit_shm(bytes, w, h, id, is_tmux);
    }
    Ok(transmit_base64(bytes, w, h, id, is_tmux, compress))
}

/// Create a shared memory object.
///
/// **Linux writes through the descriptor.** `write(2)` on a POSIX shared memory
/// object allocates the `tmpfs` pages it touches inside the syscall itself —
/// unlike `ftruncate`, which only names a length and lets `tmpfs` allocate on
/// first touch — so there is nothing to reserve up front and no mapping to make:
/// an object bigger than the space left in `/dev/shm` (64 MB by default in a
/// Docker container) answers `write_all` with an ordinary `io::Error` — `ENOSPC`,
/// or a short write — which the caller already turns into a fallback like any
/// other failure. `ftruncate` is unnecessary too, since the write itself sets the
/// object's length. `shm::open` hands back an `OwnedFd`, and `File`'s `From`
/// impl for it needs no `unsafe` at all.
///
/// The object still ends up sized to exactly the payload — a terminal rejects one
/// smaller than `s * v * bpp` (Ghostty: "shared memory size too small") — which
/// falls out of writing exactly `bytes.len()` bytes to a fresh object.
///
/// Linux is the one target with its own function because it is the one target
/// where mapping a shared memory object can fail at runtime instead of at open
/// time: a mapping that outruns a `tmpfs` short on pages faults with `SIGBUS` on
/// first touch, past any `Result` this crate could hand back. `write(2)` turns
/// the same shortage into an ordinary `io::Error` instead, so Linux writes and
/// every other Unix maps (see this function's twin below).
#[cfg(target_os = "linux")]
pub(crate) fn shm_write(name: &str, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;

    let fd = shm::open(
        name,
        ShmOFlags::CREATE | ShmOFlags::RDWR | ShmOFlags::TRUNC,
        Mode::RUSR | Mode::WUSR,
    )?;
    std::fs::File::from(fd).write_all(bytes)?;
    Ok(())
}

/// Create a shared memory object of exactly `bytes.len()` and fill it.
///
/// **Every non-Linux Unix goes in through a mapping instead of `write(2)`.**
/// `mmap` is the one operation POSIX actually promises on a shared memory
/// object; `write` is a per-kernel courtesy on top of it, and this crate has
/// already met both ends of that: macOS refuses it outright with `ENXIO`
/// (there is no read/write path on its shared memory objects at all), while
/// FreeBSD happens to allow it but is untested here. Mapping is also what the
/// terminal itself uses to read the object back, so this is the operation
/// every reader on these platforms is already relying on. `ftruncate` alone
/// reserves what it names — these objects are ordinary anonymous memory with
/// no separate `tmpfs` quota to run out of the way Linux's can (see the
/// `target_os = "linux"` twin of this function, above, for why Linux writes
/// instead). The cfg spells "every Unix but Linux", not "macOS": the crate
/// draws no line between the rest anywhere else, and this function shouldn't
/// either.
///
/// The object is sized to exactly the payload, since a terminal rejects one
/// smaller than `s * v * bpp` (Ghostty: "shared memory size too small").
#[cfg(not(any(windows, target_os = "linux")))]
pub(crate) fn shm_write(name: &str, bytes: &[u8]) -> Result<()> {
    let fd = shm::open(
        name,
        ShmOFlags::CREATE | ShmOFlags::RDWR | ShmOFlags::TRUNC,
        Mode::RUSR | Mode::WUSR,
    )?;
    rustix::fs::ftruncate(&fd, bytes.len() as u64)?;
    // SAFETY: a fresh mapping of a descriptor we just created and sized to
    // `bytes.len()`, written once and unmapped before it can be aliased.
    unsafe {
        let ptr = mmap(
            std::ptr::null_mut(),
            bytes.len(),
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::SHARED,
            &fd,
            0,
        )?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len());
        munmap(ptr, bytes.len())?;
    }
    Ok(())
}

/// The name of the shared memory object filename: `/rtui-<16bit random base64>`.
///
/// POSIX defines _POSIX_NAME_MAX as 14 for filename components, but shm_open() has
/// implementation-defined name-length limits.
/// Linux allows 255, but macOS caps the whole name at 31 bytes including the leading
/// slash (`PSHMNAMLEN`), and anything longer is refused with `ENAMETOOLONG`.
/// Measured on macos 15.
///
/// We cannot re-use the image ID, since all we really do is return some ratatui
/// buffers, and it is quite probable that the terminal is still reading the SMO
/// while we would write a resized (or otherwise updated) image to the same file.
///
/// Generates a 16 byte (128 bit) random number, encodes as base64, which fits well
/// into the macos 31 byte filename length limitation, including the "/rtui-" prefix.
#[cfg(not(windows))]
pub(crate) fn generate_shm_name() -> String {
    let bytes: [u8; 16] = rand::random();
    let filename = format!(
        "/rtui-{}",
        base64_simd::URL_SAFE_NO_PAD.encode_to_string(bytes)
    );
    debug_assert!(filename.len() <= 31);
    filename
}

/// Transmit via POSIX shared memory object (t=s).
///
/// Writes raw RGBA pixels into a named SHM object, then emits a single kitty APC chunk
/// pointing at it. The SHM object is intentionally left alive for kitty to unlink —
/// and if the caller never consumes this transmit with a render (a dropped frame in
/// a threaded caller, say), that object is left for the caller to clean up by hand,
/// the same way an untransmitted base64 image is left marked transmitted but never
/// shown.
#[cfg(not(windows))]
fn transmit_shm(bytes: &[u8], w: u32, h: u32, id: u32, is_tmux: bool) -> Result<String> {
    let shm_name = generate_shm_name();
    shm_write(&shm_name, bytes)?;

    let (start, escape, end) = Parser::tmux_start_escape_end(is_tmux);

    let payload_len = shm_name.len().div_ceil(3) * 4; // base64 upper bound
    let mut data =
        String::with_capacity(start.len() + escape.len() * 2 + 50 + payload_len + end.len());
    data.push_str(start);
    write!(
        data,
        "{escape}_Gq=2,i={id},a=T,U=1,f=32,t=s,s={w},v={h},m=0;"
    )
    .unwrap();
    base64_simd::STANDARD.encode_append(shm_name.as_bytes(), &mut data);
    write!(data, "{escape}\\").unwrap();
    data.push_str(end);

    Ok(data)
}

/// Create a kitty escape sequence for transmitting and virtual-placement.
///
/// The image will be transmitted as RGBA in chunks of 4096 bytes.
/// A "virtual placement" (U=1) is created so that we can place it using unicode placeholders.
/// Removing the placements when the unicode placeholder is no longer there is being handled
/// automatically by kitty.
///
/// With `compress`, the payload is deflated and the transmission says `o=z`.
/// That is the payload's ENCODING and nothing else: compression happens before
/// base64, `f=32` still names the format the terminal finds after inflating, and
/// `s`/`v` still name the uncompressed image's pixel dimensions, because the
/// terminal sizes its buffer from them. Only chunk boundaries move, since it is
/// the compressed stream that gets chunked. `S` is for PNG-plus-compression and
/// has no place here.
///
/// `compress` must only be set when the terminal answered the `o=z` capability
/// probe: a terminal that cannot inflate refuses the transmission outright, and
/// every placement naming the image then draws nothing at all.
fn transmit_base64(bytes: &[u8], w: u32, h: u32, id: u32, is_tmux: bool, compress: bool) -> String {
    let bytes: Cow<[u8]> = if compress {
        Cow::Owned(zlib(bytes))
    } else {
        Cow::Borrowed(bytes)
    };
    let compression = if compress { "o=z," } else { "" };

    let (start, escape, end) = Parser::tmux_start_escape_end(is_tmux);

    // Max chunk size is 4096 bytes of base64 encoded data
    const CHARS_PER_CHUNK: usize = 4096;
    const CHUNK_SIZE: usize = (CHARS_PER_CHUNK / 4) * 3;
    let chunks = bytes.chunks(CHUNK_SIZE);
    let chunk_count = chunks.len();

    // rough estimation for the worst-case size of what'll be written into `data` in the following
    // loop
    const WORST_CASE_ADDITIONAL_CHUNK_0_LEN: usize = 50;
    let bytes_written_per_chunk = 11 + CHARS_PER_CHUNK + (escape.len() * 2);
    let reserve_size =
        (chunk_count * bytes_written_per_chunk) + WORST_CASE_ADDITIONAL_CHUNK_0_LEN + end.len();

    let mut data = String::with_capacity(reserve_size);

    for (i, chunk) in chunks.enumerate() {
        data.push_str(start);
        // tmux seems to only allow a limited amount of data in each passthrough sequence, since
        // we're already chunking the data for the kitty protocol that's a good enough chunk size to
        // use for the passthrough chunks too.
        write!(data, "{escape}_Gq=2,").unwrap();

        if i == 0 {
            write!(data, "i={id},a=T,U=1,f=32,{compression}t=d,s={w},v={h},").unwrap();
        }

        // m=0 means over
        let more = u8::from(chunk_count > (i + 1));
        write!(data, "m={more};").unwrap();

        base64_simd::STANDARD.encode_append(chunk, &mut data);

        write!(data, "{escape}\\").unwrap();
        data.push_str(end);
    }

    data
}

/// From https://sw.kovidgoyal.net/kitty/_downloads/1792bad15b12979994cd6ecc54c967a6/rowcolumn-diacritics.txt
/// See https://sw.kovidgoyal.net/kitty/graphics-protocol/#unicode-placeholders for further explanation.
static DIACRITICS: [char; 297] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

#[inline]
fn diacritic(y: u16) -> char {
    *DIACRITICS
        .get(usize::from(y))
        .unwrap_or_else(|| &DIACRITICS[0])
}

#[cfg(test)]
mod tests {
    use super::{DIACRITICS, PLACEHOLDER, diacritic, render, transmit_base64};
    use image::{DynamicImage, RgbaImage};
    use ratatui::buffer::Buffer;
    use ratatui::layout::{Rect, Size};
    use ratatui::style::Color;

    /// The id state tuple as the protocol builds it: full id, low three bytes
    /// as the foreground colour, high byte as the third diacritic.
    fn id_state(id: u32) -> (u32, Color, u16) {
        let [id_extra, id_r, id_g, id_b] = id.to_be_bytes();
        (id, Color::Rgb(id_r, id_g, id_b), u16::from(id_extra))
    }

    /// The diacritics on one placeholder cell, after the placeholder itself.
    fn marks(buf: &Buffer, x: u16, y: u16) -> Vec<char> {
        let mut chars = buf[(x, y)].symbol().chars();
        assert_eq!(
            chars.next(),
            Some(PLACEHOLDER),
            "cell {x},{y} is not a placeholder"
        );
        chars.collect()
    }

    /// Only the first cell of a row states a position; the rest inherit the
    /// row and increment the column, which kitty allows because every cell
    /// carries the same foreground colour.
    #[test]
    fn only_the_first_cell_of_a_row_states_a_position() {
        let area = Rect::new(0, 0, 4, 3);
        let mut buf = Buffer::empty(area);
        render(
            area,
            Size::new(4, 3),
            &mut buf,
            &id_state(0x0102_0304),
            None,
            0,
        );

        for y in 0..3 {
            assert_eq!(
                marks(&buf, 0, y),
                vec![diacritic(y), diacritic(0), diacritic(1)],
                "row {y} must state its position"
            );
            for x in 1..4 {
                assert!(
                    marks(&buf, x, y).is_empty(),
                    "cell {x},{y} repeats a position it could inherit"
                );
            }
        }
    }

    /// The id's low three bytes are the foreground colour. Carrying it as a
    /// cell attribute rather than an escape inside one cell's symbol is what
    /// lets it survive being re-emitted cell by cell.
    #[test]
    fn the_id_rides_in_the_foreground_colour() {
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        render(
            area,
            Size::new(2, 1),
            &mut buf,
            &id_state(0x00AA_BBCC),
            None,
            0,
        );

        for x in 0..2 {
            assert_eq!(buf[(x, 0)].style().fg, Some(Color::Rgb(0xAA, 0xBB, 0xCC)));
        }
    }

    /// The transmission is prefixed to the first cell drawn and to no other,
    /// so it reaches the terminal exactly once per render.
    #[test]
    fn the_transmission_is_emitted_once() {
        let area = Rect::new(0, 0, 3, 2);
        let mut buf = Buffer::empty(area);
        render(
            area,
            Size::new(3, 2),
            &mut buf,
            &id_state(1),
            Some("<TRANSMIT>"),
            0,
        );

        assert!(
            buf[(0, 0)].symbol().starts_with("<TRANSMIT>"),
            "the first cell carries the transmission"
        );
        for (x, y) in [(1, 0), (2, 0), (0, 1), (1, 1), (2, 1)] {
            assert!(
                !buf[(x, y)].symbol().contains("<TRANSMIT>"),
                "cell {x},{y} repeated the transmission"
            );
        }
    }

    /// Rows and columns are both encoded by table lookup, so both have to stop
    /// at the end of the table rather than silently drawing a wrong position.
    #[test]
    fn positions_past_the_diacritic_table_are_left_undrawn() {
        let over = DIACRITICS.len() as u16 + 4;
        let area = Rect::new(0, 0, over, over);
        let mut buf = Buffer::empty(area);
        render(area, Size::new(over, over), &mut buf, &id_state(1), None, 0);

        let last = DIACRITICS.len() as u16 - 1;
        assert_eq!(buf[(last, 0)].symbol().chars().next(), Some(PLACEHOLDER));
        assert_ne!(
            buf[(last + 1, 0)].symbol().chars().next(),
            Some(PLACEHOLDER),
            "a column with no diacritic must not be drawn"
        );

        assert_eq!(buf[(0, last)].symbol().chars().next(), Some(PLACEHOLDER));
        assert_ne!(
            buf[(0, last + 1)].symbol().chars().next(),
            Some(PLACEHOLDER),
            "a row with no diacritic must not be drawn"
        );
    }

    /// SlicedImage renders a window onto a taller placement, so the row
    /// diacritic has to be the row within the image, not within the area.
    #[test]
    fn skip_line_count_offsets_the_row_diacritic() {
        let area = Rect::new(0, 0, 1, 2);
        let mut buf = Buffer::empty(area);
        render(area, Size::new(1, 2), &mut buf, &id_state(1), None, 5);

        assert_eq!(marks(&buf, 0, 0)[0], diacritic(5));
        assert_eq!(marks(&buf, 0, 1)[0], diacritic(6));
    }

    fn canvas() -> DynamicImage {
        // Flat bands: artwork, not noise, which is what deflate is for and what
        // every one of these transmissions actually carries.
        let mut img = RgbaImage::new(64, 32);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x / 8 * 32) as u8, 0x40, 0x80, 0xff]);
        }
        DynamicImage::ImageRgba8(img)
    }

    /// Split a transmission into its first command's parameters and every
    /// chunk's payload concatenated, the way a terminal reassembles it.
    fn reassemble(seq: &str) -> (String, Vec<u8>) {
        let mut params = String::new();
        let mut payload = String::new();
        for (i, cmd) in seq.split("\x1b_G").skip(1).enumerate() {
            let cmd = cmd
                .strip_suffix("\x1b\\")
                .expect("each command ends with ST");
            let (p, data) = cmd.split_once(';').expect("each chunk has a payload");
            if i == 0 {
                params = p.to_string();
            } else {
                assert!(
                    p.split(',')
                        .all(|kv| kv.starts_with("m=") || kv.starts_with("q=")),
                    "a continuation chunk may carry only m and q, got `{p}`"
                );
            }
            payload.push_str(data);
        }
        let bytes = base64_simd::STANDARD
            .decode_to_vec(payload)
            .expect("the payload is base64");
        (params, bytes)
    }

    #[test]
    fn transmit_without_compression_is_the_raw_image() {
        let img = canvas();
        let raw = img.to_rgba8();
        let (params, bytes) = reassemble(&transmit_base64(
            raw.as_raw(),
            img.width(),
            img.height(),
            7,
            false,
            false,
        ));
        assert!(
            !params.contains("o=z"),
            "nothing claims to be compressed: {params}"
        );
        assert!(
            params.contains("f=32") && params.contains("s=64,v=32"),
            "{params}"
        );
        assert_eq!(bytes, *img.to_rgba8().as_raw());
    }

    /// `o=z` is the payload's ENCODING: `f` still names the format the terminal
    /// finds after inflating, and `s`/`v` still name the uncompressed image,
    /// because that is what the terminal sizes its buffer from. A transmission
    /// that compressed each chunk separately, or that put the compressed length
    /// in `s`/`v`, would contain `o=z` just the same and draw nothing.
    #[test]
    fn transmit_with_compression_inflates_back_to_the_raw_image() {
        let img = canvas();
        let raw = img.to_rgba8();
        let (params, bytes) = reassemble(&transmit_base64(
            raw.as_raw(),
            img.width(),
            img.height(),
            7,
            false,
            true,
        ));
        assert!(
            params.contains("o=z"),
            "the payload is compressed and says so: {params}"
        );
        assert!(
            params.contains("f=32"),
            "`o=z` is the encoding, `f` is the format: {params}"
        );
        assert!(
            params.contains("s=64,v=32"),
            "the UNCOMPRESSED dimensions: {params}"
        );
        assert!(
            !params.contains("S="),
            "`S` is for PNG-plus-compression, and this is f=32"
        );

        assert!(
            bytes.len() < raw.as_raw().len(),
            "{} vs {}",
            bytes.len(),
            raw.as_raw().len()
        );
        let mut out = Vec::new();
        std::io::copy(&mut flate2::read::ZlibDecoder::new(&bytes[..]), &mut out)
            .expect("the whole payload is one zlib stream");
        assert_eq!(out, *raw.as_raw());
    }

    /// Two transmits of the SAME image id must still land in two different
    /// objects — that is the entire fix: the terminal owns the first object
    /// asynchronously from the moment its escape is written, and a second
    /// transmit reusing that name would recreate it out from under a reader that
    /// has not gotten to it yet. See [`super::transmit_shm`]'s doc comment.
    #[test]
    #[cfg(not(windows))]
    fn consecutive_shm_transmits_of_the_same_id_use_different_objects() {
        let img = canvas();
        let raw = img.to_rgba8();
        let first = super::transmit_shm(raw.as_raw(), img.width(), img.height(), 7, false)
            .expect("this platform writes shared memory");
        let second = super::transmit_shm(raw.as_raw(), img.width(), img.height(), 7, false)
            .expect("this platform writes shared memory");

        let name_of = |seq: &str| {
            let start = seq.find(";").expect("payload starts after the header") + 1;
            let end = seq.rfind("\x1b\\").expect("the ST ends the payload");
            let name = base64_simd::STANDARD
                .decode_to_vec(&seq[start..end])
                .expect("the payload is base64");
            String::from_utf8(name).expect("the object name is ASCII")
        };
        let (first_name, second_name) = (name_of(&first), name_of(&second));
        assert_ne!(
            first_name, second_name,
            "same image id (7), but the object name must still differ so the second \
             transmit can never touch an object the terminal may still be reading \
             from the first: {first_name:?} vs {second_name:?}"
        );

        // Clean up: a successful `t=s` transmit is left for the terminal to
        // unlink, which nothing here plays the part of.
        let _ = rustix::shm::unlink(first_name);
        let _ = rustix::shm::unlink(second_name);
    }
}
