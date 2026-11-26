// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure-Rust PDF structural and decompression-bomb pre-validator for
//! `read_bill` (B-461, B-469, D-1004, T-72).
//!
//! Runs in two layers before any PDF text extraction (`pdf-extract`) or page
//! rasterization (`hayro::render`):
//!
//! 1. [`validate_raw_pdf_safety`]: inspects raw PDF bytes *before* `Pdf::new`
//!    or `lopdf::Document::load_mem` parses xref/object streams, capping object
//!    count, stream count, page count, embedded `/XObject /Image` dimensions,
//!    and boundedly inflating `FlateDecode`, `LZWDecode`, `RunLengthDecode`,
//!    `ASCIIHexDecode` and `ASCII85Decode` streams (including chained filters,
//!    per-stage `/DecodeParms`/`/DP`, `/XRef` and `/ObjStm` streams, and
//!    embedded JPEG/JPX/JBIG2/CCITT dimensions).
//! 2. [`validate_parsed_pdf_safety`]: walks the lazily parsed
//!    [`hayro::hayro_syntax::Pdf`] objects (without calling `stream.decoded()`
//!    on unvalidated streams) to catch indirect `/Filter`, `/DecodeParms`/`/DP`,
//!    `/Width`, `/Height` references, object-stream-hidden image XObjects,
//!    embedded JPEG/JPX/JBIG2/CCITT header bombs, and inline images
//!    (`BI ... ID ... EI`) inside page, Form XObject, pattern, and Type 3
//!    content streams before rendering or text extraction.

use std::collections::{HashMap, HashSet};

use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::content::TypedIter;
use hayro::hayro_syntax::content::ops::TypedInstruction;
use hayro::hayro_syntax::object::{
    Array, Dict, MaybeRef, Name, Number, Object, ObjectIdentifier, Stream,
};

use crate::bill_reader::MAX_BILL_DECODE_DIMENSION;
use crate::error::ToolError;

/// Maximum PDF object count accepted by `read_bill` (B-461, T-72).
pub(crate) const MAX_PDF_OBJECTS: usize = 50_000;

/// Maximum PDF stream count accepted by `read_bill` (B-461, T-72).
pub(crate) const MAX_PDF_STREAMS: usize = 10_000;

/// Maximum total PDF pages accepted by `read_bill` (B-461, T-72).
pub(crate) const MAX_PDF_PAGES: usize = 2_000;

/// Maximum PDF page count for full-document `pdf-extract` text extraction;
/// larger documents skip full-document text extraction and rasterize only the
/// requested page window via `hayro` (B-461, T-72).
pub(crate) const MAX_PDF_TEXT_EXTRACT_PAGES: usize = 100;

/// Maximum decompressed size of any single PDF stream (16 MiB, B-461, T-72).
pub(crate) const MAX_PDF_STREAM_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;

/// Maximum cumulative decompressed bytes across all streams in one PDF
/// (64 MiB, B-461, T-72).
pub(crate) const MAX_PDF_TOTAL_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;

/// Maximum pixel count (`width * height`) of any single embedded PDF
/// `/XObject /Image` or inline image (32 MP, ~128 MiB RGBA, B-461, B-469, T-72).
pub(crate) const MAX_PDF_IMAGE_PIXELS: u64 = 32_000_000;

/// Maximum cumulative pixel count across all embedded PDF images
/// (128 MP, B-461, T-72).
pub(crate) const MAX_PDF_TOTAL_IMAGE_PIXELS: u64 = 128_000_000;

/// Maximum filter stages chained on a single PDF stream (B-461, T-72).
const MAX_PDF_FILTER_STAGES: usize = 4;

/// Maximum bits per component accepted for an embedded PDF image.
const MAX_PDF_IMAGE_BPC: u64 = 16;

/// Maximum JPEG 2000 components accepted in a SIZ header (B-469).
const MAX_JPX_COMPONENTS: u16 = 16;

/// Maximum JPEG 2000 tile count accepted in a SIZ header (B-469).
const MAX_JPX_TILES: u64 = 10_000;

/// Maximum JP2 container boxes scanned per stream (B-469).
const MAX_JP2_BOXES: usize = 256;

/// Maximum JBIG2 segments scanned per embedded stream (B-469).
const MAX_JBIG2_SEGMENTS: usize = 4_096;

/// Maximum cumulative bytes spent on failed/speculative scans in Layer 1
/// before rejecting a malformed PDF (prevents quadratic CPU DoS on repeated
/// unclosed `(` or `<<`, B-469).
const MAX_SPECULATIVE_SCAN_BYTES: usize = 32 * 1024 * 1024;

/// Maximum `startxref` / `/Prev` / `/XRefStm` targets validated in Layer 1.
const MAX_XREF_TARGETS: usize = 128;

/// Filter kinds checked by the bounded stream decoder (B-461, B-469).
///
/// Explicitly distinguishes every codec filter recognized by `hayro-syntax`
/// from unrecognized filter names (`Unknown`), which `hayro-syntax` drops
/// while continuing to execute subsequent filters in the array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PdfFilter {
    Flate,
    Lzw,
    RunLength,
    AsciiHex,
    Ascii85,
    Ccitt,
    Dct,
    Jbig2,
    Jpx,
    Crypt,
    Unknown,
}

impl PdfFilter {
    fn from_pdf_name(name: &[u8]) -> Self {
        match name {
            b"FlateDecode" | b"Fl" => Self::Flate,
            b"LZWDecode" | b"LZW" => Self::Lzw,
            b"RunLengthDecode" | b"RL" => Self::RunLength,
            b"ASCIIHexDecode" | b"AHx" => Self::AsciiHex,
            b"ASCII85Decode" | b"A85" => Self::Ascii85,
            b"CCITTFaxDecode" | b"CCF" => Self::Ccitt,
            b"DCTDecode" | b"DCT" => Self::Dct,
            b"JBIG2Decode" => Self::Jbig2,
            b"JPXDecode" => Self::Jpx,
            b"Crypt" => Self::Crypt,
            _ => Self::Unknown,
        }
    }

    fn is_image_codec(self) -> bool {
        matches!(self, Self::Ccitt | Self::Dct | Self::Jbig2 | Self::Jpx)
    }
}

/// Validated predictor state for a `FlateDecode` or `LZWDecode` filter stage
/// (B-469).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PredictorCheck {
    /// No predictor (`Predictor == 1` or omitted).
    None,
    /// Valid TIFF (2) or PNG (10..=15) predictor with computed row buffer size.
    Valid { row_bytes: u64 },
    /// Malformed predictor parameters that would OOM or panic `hayro-syntax`
    /// (`Predictor` outside `{1, 2, 10..=15}`, `Columns <= 0`, `Colors <= 0`,
    /// `BitsPerComponent` outside `{1, 2, 4, 8, 16}`, or `Colors * BPC > 255`).
    Invalid,
}

/// Validates Flate/LZW predictor parameters identically across Layer 1 and
/// Layer 2 before `hayro-syntax`'s `apply_predictor` can OOM or panic (B-469).
fn compute_predictor_check(
    predictor: i64,
    columns: Option<i64>,
    colors: Option<i64>,
    bpc: Option<i64>,
) -> PredictorCheck {
    if predictor == 1 {
        return PredictorCheck::None;
    }
    if !matches!(predictor, 2 | 10..=15) {
        return PredictorCheck::Invalid;
    }
    let cols = columns.unwrap_or(1);
    let colors = colors.unwrap_or(1);
    let bpc = bpc.unwrap_or(8);
    if cols <= 0 || colors <= 0 || !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
        return PredictorCheck::Invalid;
    }
    let bpp = (colors as u64).saturating_mul(bpc as u64);
    if bpp > 255 {
        return PredictorCheck::Invalid;
    }
    let row_bytes = (cols as u64).saturating_mul(bpp).div_ceil(8);
    PredictorCheck::Valid { row_bytes }
}

/// Per-stage `/DecodeParms` (or abbreviated `/DP`) parameters extracted for one
/// filter stage in a PDF stream (B-469).
#[derive(Clone, Copy, Debug)]
struct StageParams {
    early_change: bool,
    ccitt_columns: Option<u64>,
    ccitt_rows: Option<u64>,
    predictor: PredictorCheck,
}

impl Default for StageParams {
    fn default() -> Self {
        Self {
            early_change: true,
            ccitt_columns: None,
            ccitt_rows: None,
            predictor: PredictorCheck::None,
        }
    }
}

/// Image dimension context from the enclosing stream dictionary, used to
/// cross-check CCITT `Columns`/`Rows` and avoid double-charging pixels already
/// accounted for by the image dictionary.
#[derive(Clone, Copy, Debug, Default)]
struct StreamImageContext {
    width: Option<u64>,
    height: Option<u64>,
    pixels_charged: u64,
}

/// Simple indirect object values pre-indexed in pass 1 of raw PDF scanning so
/// indirect `/Length`, `/Width`, `/Height`, `/Filter`, and `/DecodeParms`/`/DP`
/// references are resolved even before `Pdf::new`.
enum SimpleIndirect {
    Int(i64),
    Filters(Vec<PdfFilter>, bool),
    StageParams(Vec<StageParams>, bool),
}

/// Running resource counters across one PDF validation pass.
#[derive(Default)]
struct Budget {
    total_decompressed: usize,
    total_image_pixels: u64,
}

impl Budget {
    fn remaining_decompressed(&self) -> usize {
        MAX_PDF_TOTAL_DECOMPRESSED_BYTES
            .saturating_sub(self.total_decompressed)
            .min(MAX_PDF_STREAM_DECOMPRESSED_BYTES)
    }

    fn charge_decompressed(&mut self, bytes: usize) -> Result<(), ToolError> {
        self.total_decompressed = self.total_decompressed.saturating_add(bytes);
        if self.total_decompressed > MAX_PDF_TOTAL_DECOMPRESSED_BYTES {
            return Err(ToolError::BillTooBig);
        }
        Ok(())
    }

    fn check_image_dims(&mut self, width: u64, height: u64) -> Result<u64, ToolError> {
        self.check_extra_image_dims(width, height, 0)
    }

    /// Validates `(width, height)` against dimension and per-image pixel caps,
    /// charging only pixels beyond `already_charged` to `total_image_pixels`.
    fn check_extra_image_dims(
        &mut self,
        width: u64,
        height: u64,
        already_charged: u64,
    ) -> Result<u64, ToolError> {
        let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
        if width > max_dim || height > max_dim {
            return Err(ToolError::BillTooBig);
        }
        let pixels = width.saturating_mul(height);
        if pixels > MAX_PDF_IMAGE_PIXELS {
            return Err(ToolError::BillTooBig);
        }
        let extra = pixels.saturating_sub(already_charged);
        self.total_image_pixels = self.total_image_pixels.saturating_add(extra);
        if self.total_image_pixels > MAX_PDF_TOTAL_IMAGE_PIXELS {
            return Err(ToolError::BillTooBig);
        }
        Ok(pixels)
    }
}

fn charge_speculative_scan(budget: &mut usize, bytes_scanned: usize) -> Result<(), ToolError> {
    if bytes_scanned > *budget {
        *budget = 0;
        return Err(ToolError::BillUnreadable);
    }
    *budget -= bytes_scanned;
    Ok(())
}

/// Validates a raw PDF stream block starting immediately after its dictionary.
///
/// Returns `Ok(Some(next_pos))` when a compressed stream was fully validated
/// and the caller may advance past `endstream`, or `Ok(None)` when the stream
/// is uncompressed or undecodable (so Pass 2 keeps scanning inside the body
/// for any nested `/XRef`/`/ObjStm` bombs, B-469).
#[allow(clippy::too_many_arguments)]
fn validate_raw_stream_block(
    bytes: &[u8],
    data_start: usize,
    info: &RawDictInfo,
    img_ctx: StreamImageContext,
    has_encrypt: bool,
    force_fail_closed: bool,
    indirects: &HashMap<u32, SimpleIndirect>,
    validated_streams: &mut HashSet<usize>,
    budget: &mut Budget,
    scan_budget: &mut usize,
) -> Result<Option<usize>, ToolError> {
    if info.filter_conflict_or_external {
        return Err(ToolError::BillUnreadable);
    }
    let fail_closed = force_fail_closed || info.is_xref || (info.is_obj_stm && !has_encrypt);
    if fail_closed && (info.unresolved_filter_ref || info.unresolved_params_ref) {
        return Err(ToolError::BillUnreadable);
    }
    if info.invalid_params && !info.filters.is_empty() {
        return Err(ToolError::BillUnreadable);
    }
    if info.filters.is_empty() {
        return Ok(None);
    }
    if info.filters.len() > MAX_PDF_FILTER_STAGES {
        return Err(ToolError::BillTooBig);
    }
    let (stream_slice, next_pos, used_fallback_scan) =
        locate_stream_slice(bytes, data_start, info.length);
    if !validated_streams.insert(data_start) {
        return Ok(Some(next_pos));
    }
    let decoded = check_filter_chain(
        stream_slice,
        &info.filters,
        &info.stage_params,
        img_ctx,
        info.is_obj_stm,
        fail_closed,
        budget,
    )?;
    if let Some((_, opt_obj_stm_bytes)) = decoded {
        if let Some(obj_stm_bytes) = opt_obj_stm_bytes {
            scan_obj_stm_dicts(&obj_stm_bytes, indirects, budget, scan_budget)?;
        }
        return Ok(Some(next_pos));
    }
    if used_fallback_scan {
        charge_speculative_scan(scan_budget, next_pos.saturating_sub(data_start))?;
    }
    Ok(None)
}

/// Explicitly validates every `startxref` target offset and any chained `/Prev`
/// or `/XRefStm` targets before `Pdf::new` can seek to them (B-469).
fn validate_startxref_targets(
    bytes: &[u8],
    indirects: &HashMap<u32, SimpleIndirect>,
    has_encrypt: bool,
    validated_streams: &mut HashSet<usize>,
    budget: &mut Budget,
    scan_budget: &mut usize,
) -> Result<(), ToolError> {
    let mut worklist: Vec<usize> = Vec::new();
    let mut search = 0usize;
    while search < bytes.len() {
        let Some(rel) = find_subslice(&bytes[search..], b"startxref") else {
            break;
        };
        let kw_pos = search + rel;
        search = kw_pos + 9;
        if kw_pos > 0 && !is_pdf_delim_or_ws(bytes[kw_pos - 1]) {
            continue;
        }
        if bytes
            .get(kw_pos + 9)
            .is_some_and(|&b| !is_pdf_delim_or_ws(b))
        {
            continue;
        }
        let num_pos = skip_pdf_ws_and_comments(bytes, kw_pos + 9);
        if let Some((off_u64, _)) = parse_u64_ascii(bytes, num_pos)
            && let Ok(off) = usize::try_from(off_u64)
            && off < bytes.len()
        {
            worklist.push(off);
            if worklist.len() > MAX_XREF_TARGETS {
                return Err(ToolError::BillTooBig);
            }
        }
    }

    let mut visited: HashSet<usize> = HashSet::new();
    while let Some(offset) = worklist.pop() {
        if !visited.insert(offset) {
            continue;
        }
        if visited.len() > MAX_XREF_TARGETS {
            return Err(ToolError::BillTooBig);
        }
        let mut p = skip_pdf_ws_and_comments(bytes, offset);
        if p >= bytes.len() {
            continue;
        }
        if bytes[p..].starts_with(b"xref")
            && bytes.get(p + 4).is_none_or(|&b| is_pdf_delim_or_ws(b))
        {
            let window_end = bytes.len().min(p.saturating_add(4 * 1024 * 1024));
            if let Some(rel) = find_subslice(&bytes[p..window_end], b"trailer") {
                let dict_pos = skip_pdf_ws_and_comments(bytes, p + rel + 7);
                if let Some((_, dict_bytes, _)) =
                    extract_balanced_dict(bytes, dict_pos, scan_budget)?
                {
                    let info = inspect_raw_dict(dict_bytes, indirects, scan_budget)?;
                    if let Some(prev) = info.prev_offset
                        && prev < bytes.len()
                    {
                        worklist.push(prev);
                    }
                    if let Some(xref_stm) = info.xref_stm_offset
                        && xref_stm < bytes.len()
                    {
                        worklist.push(xref_stm);
                    }
                }
            }
            continue;
        }
        if bytes[p].is_ascii_digit()
            && let Some((_, after_obj_kw)) = parse_obj_header(bytes, p)
        {
            p = skip_pdf_ws_and_comments(bytes, after_obj_kw);
        }
        if bytes.get(p) == Some(&b'<')
            && bytes.get(p + 1) == Some(&b'<')
            && let Some((dict_end, dict_bytes, _)) = extract_balanced_dict(bytes, p, scan_budget)?
        {
            let info = inspect_raw_dict(dict_bytes, indirects, scan_budget)?;
            let img_ctx = check_raw_dict_image(&info, budget)?;
            if let Some(prev) = info.prev_offset
                && prev < bytes.len()
            {
                worklist.push(prev);
            }
            if let Some(xref_stm) = info.xref_stm_offset
                && xref_stm < bytes.len()
            {
                worklist.push(xref_stm);
            }
            let after_ws = skip_pdf_ws_and_comments(bytes, dict_end);
            if bytes[after_ws..].starts_with(b"stream")
                && matches!(bytes.get(after_ws + 6), Some(b'\n' | b'\r'))
            {
                let data_start = skip_stream_eol(bytes, after_ws + 6);
                let _ = validate_raw_stream_block(
                    bytes,
                    data_start,
                    &info,
                    img_ctx,
                    has_encrypt,
                    true,
                    indirects,
                    validated_streams,
                    budget,
                    scan_budget,
                )?;
            }
        }
    }
    Ok(())
}

/// Layer 1: Pure-Rust structural and stream-bomb pre-validator run directly on
/// raw PDF bytes *before* invoking `hayro::hayro_syntax::Pdf::new` or
/// `pdf_extract`.
pub(crate) fn validate_raw_pdf_safety(bytes: &[u8]) -> Result<(), ToolError> {
    let header_window = &bytes[..bytes.len().min(1024)];
    if !contains_subslice(header_window, b"%PDF-") {
        return Err(ToolError::BillUnreadable);
    }

    let has_encrypt = contains_subslice(bytes, b"/Encrypt");
    let mut scan_budget = MAX_SPECULATIVE_SCAN_BYTES;

    // Pass 1: count indirect object definitions and capture simple indirect
    // objects (`N G obj <int|/Filter|[...]|<<...>>> endobj`), skipping strings
    // and stream bodies (B-469).
    let indirects = scan_indirect_objects_with_budget(bytes, &mut scan_budget)?;

    // Pass 2: scan all dictionaries and `stream ... endstream` blocks.
    let mut budget = Budget::default();
    let mut validated_streams: HashSet<usize> = HashSet::new();
    let mut stream_count = 0usize;
    let mut page_count = 0usize;
    let mut uncompressed_stream_end = 0usize;
    let mut pos = 0usize;

    while pos < bytes.len() {
        let in_uncompressed_stream = pos < uncompressed_stream_end;
        if !in_uncompressed_stream && bytes[pos] == b'%' {
            // Jump straight to `<<` on the same comment line if present (a
            // crafted `startxref` could point past `%` into a stream dict),
            // otherwise skip the whole line in O(line_len) (B-469).
            let line_end = skip_line(bytes, pos);
            if let Some(rel) = find_subslice(&bytes[pos..line_end], b"<<") {
                pos += rel;
            } else {
                pos = line_end;
            }
            continue;
        }
        if !in_uncompressed_stream && bytes[pos] == b'(' {
            // Skip well-formed PDF literal strings that do not contain `<<` or
            // cross `stream\r/\n` so a crafted balanced string cannot hide an
            // `/XRef` or `/ObjStm` bomb (B-469).
            if let Some((next, _)) =
                skip_pdf_literal_string_checked(bytes, pos, true, &mut scan_budget)?
            {
                pos = next;
            } else {
                pos += 1;
            }
            continue;
        }
        if bytes[pos] == b'<' && bytes.get(pos + 1) == Some(&b'<') {
            let Some((dict_end, dict_bytes, saw_dict_open_in_string)) =
                extract_balanced_dict(bytes, pos, &mut scan_budget)?
            else {
                // Malformed or unclosed `<<`: advance past `<<` only so it
                // cannot swallow following objects or streams (B-469).
                pos += 2;
                continue;
            };
            let info = inspect_raw_dict(dict_bytes, &indirects, &mut scan_budget)?;
            if info.is_page {
                page_count += 1;
                if page_count > MAX_PDF_PAGES {
                    return Err(ToolError::BillTooBig);
                }
            }
            let img_ctx = check_raw_dict_image(&info, &mut budget)?;

            let after_ws = skip_pdf_ws_and_comments(bytes, dict_end);
            if bytes[after_ws..].starts_with(b"stream")
                && matches!(bytes.get(after_ws + 6), Some(b'\n' | b'\r'))
            {
                stream_count += 1;
                if stream_count > MAX_PDF_STREAMS {
                    return Err(ToolError::BillTooBig);
                }
                let data_start = skip_stream_eol(bytes, after_ws + 6);
                if let Some(next_pos) = validate_raw_stream_block(
                    bytes,
                    data_start,
                    &info,
                    img_ctx,
                    has_encrypt,
                    false,
                    &indirects,
                    &mut validated_streams,
                    &mut budget,
                    &mut scan_budget,
                )? {
                    if saw_dict_open_in_string {
                        charge_speculative_scan(&mut scan_budget, dict_end.saturating_sub(pos))?;
                        pos += 2;
                    } else {
                        pos = next_pos;
                    }
                    continue;
                }
                // Uncompressed or undecodable outer stream: do NOT jump over
                // its body so a dummy outer stream cannot conceal an inner
                // `/XRef` or `/ObjStm` bomb (B-469). If `/Length` matches an
                // exact `endstream`, mark the verified raw payload span so
                // top-level `(`/`%` skipping doesn't churn on raw pixels.
                if info.filters.is_empty()
                    && let Some(len) = info.length
                    && let Some(end) = data_start.checked_add(len)
                    && end <= bytes.len()
                    && bytes[skip_pdf_ws(bytes, end)..].starts_with(b"endstream")
                {
                    uncompressed_stream_end = uncompressed_stream_end.max(end);
                }
                if saw_dict_open_in_string {
                    charge_speculative_scan(&mut scan_budget, dict_end.saturating_sub(pos))?;
                    pos += 2;
                } else {
                    pos = data_start;
                }
                continue;
            }
            if saw_dict_open_in_string {
                charge_speculative_scan(&mut scan_budget, dict_end.saturating_sub(pos))?;
                pos += 2;
            } else {
                pos = dict_end;
            }
            continue;
        }
        pos += 1;
    }

    // Pass 3: explicitly validate every `startxref` / `/Prev` / `/XRefStm`
    // target offset in case any outer wrapper syntax obscures an xref stream.
    validate_startxref_targets(
        bytes,
        &indirects,
        has_encrypt,
        &mut validated_streams,
        &mut budget,
        &mut scan_budget,
    )?;

    Ok(())
}

/// Layer 2: Validates a lazily parsed [`Pdf`] before text extraction or page
/// rendering, catching indirect `/Filter`, `/DecodeParms`/`/DP`, `/Width`, or
/// `/Height` references, object-stream-hidden image XObjects, embedded
/// JPEG/JPX/JBIG2/CCITT header bombs, and inline images (`BI ... ID ... EI`)
/// inside page, Form XObject, pattern, and Type 3 content streams (B-461, B-469).
pub(crate) fn validate_parsed_pdf_safety(pdf: &Pdf) -> Result<(), ToolError> {
    if pdf.len() > MAX_PDF_OBJECTS {
        return Err(ToolError::BillTooBig);
    }
    if pdf.pages().len() > MAX_PDF_PAGES {
        return Err(ToolError::BillTooBig);
    }

    let mut budget = Budget::default();
    let mut stream_count = 0usize;
    let mut stream_sizes: HashMap<ObjectIdentifier, usize> = HashMap::new();

    // Pass 1: validate every stream's dimensions, filter chain, per-stage
    // DecodeParms/DP, and embedded image headers before any content stream is
    // decoded.
    for obj in pdf.objects() {
        match obj {
            Object::Stream(stream) => {
                stream_count += 1;
                if stream_count > MAX_PDF_STREAMS {
                    return Err(ToolError::BillTooBig);
                }
                let decoded_len = validate_hayro_stream(&stream, &mut budget, false)?;
                stream_sizes.insert(stream.obj_id(), decoded_len);
            }
            Object::Dict(dict) => {
                let _ = validate_hayro_image_dict(&dict, &mut budget, false)?;
            }
            _ => {}
        }
    }

    // Pass 2: inspect every page's content stream(s) for inline images
    // (`BI ... ID ... EI`), bounding `/Contents` array amplification first.
    let mut scanned_contents: HashSet<ObjectIdentifier> = HashSet::new();
    let mut page_content_budget = 0usize;
    for page in pdf.pages().iter() {
        let page_dict = page.raw();
        let mut page_stream_ids = Vec::new();
        let mut single_already_scanned = false;

        if let Some(arr) = page_dict.get::<Array<'_>>(b"Contents") {
            let mut page_len = 0usize;
            for item in arr.raw_iter() {
                if let MaybeRef::Ref(obj_ref) = item {
                    let id: ObjectIdentifier = obj_ref.into();
                    let sz = stream_sizes.get(&id).copied().unwrap_or(0);
                    page_len = page_len.saturating_add(sz);
                    if page_len > MAX_PDF_STREAM_DECOMPRESSED_BYTES {
                        return Err(ToolError::BillTooBig);
                    }
                    page_stream_ids.push(id);
                }
            }
            page_content_budget = page_content_budget.saturating_add(page_len);
            if page_content_budget > MAX_PDF_TOTAL_DECOMPRESSED_BYTES {
                return Err(ToolError::BillTooBig);
            }
        } else if let Some(stream) = page_dict.get::<Stream<'_>>(b"Contents") {
            let id = stream.obj_id();
            if scanned_contents.contains(&id) {
                single_already_scanned = true;
            } else {
                let sz = stream_sizes.get(&id).copied().unwrap_or(0);
                page_content_budget = page_content_budget.saturating_add(sz);
                if page_content_budget > MAX_PDF_TOTAL_DECOMPRESSED_BYTES {
                    return Err(ToolError::BillTooBig);
                }
                page_stream_ids.push(id);
            }
        }

        if !single_already_scanned && let Some(content_bytes) = page.page_stream() {
            scan_content_stream_inline_images(content_bytes, &mut budget)?;
        }
        for id in page_stream_ids {
            scanned_contents.insert(id);
        }
    }

    // Pass 3: inspect EVERY non-image-codec stream not already scanned as a
    // page content stream (Form XObjects, Annotation Appearances, SoftMask
    // groups, Tiling Patterns, Type 3 `/CharProcs`, etc.) for inline images.
    // Never skip streams based on `/Type`, `/Subtype`, or `/IM` because
    // `hayro-interpret` executes Form/Appearance/Pattern/CharProc streams
    // regardless of extra dictionary tags (B-469).
    for obj in pdf.objects() {
        let Object::Stream(stream) = obj else {
            continue;
        };
        if scanned_contents.contains(&stream.obj_id()) {
            continue;
        }
        let dict = stream.dict();
        let (filters, _) = hayro_dict_filters(dict)?;
        if filters.iter().any(|f| f.is_image_codec()) {
            continue;
        }
        let is_explicit_content = dict
            .get::<Name<'_>>(b"Subtype")
            .is_some_and(|n| n.as_ref() == b"Form")
            || dict.get::<i32>(b"PatternType") == Some(1);

        let Ok(decoded) = stream.decoded_data() else {
            continue;
        };
        if decoded.is_empty() {
            continue;
        }
        if is_explicit_content || may_contain_inline_image_op(decoded.as_ref()) {
            scan_content_stream_inline_images(decoded.as_ref(), &mut budget)?;
        }
    }

    Ok(())
}

/// Exact byte pre-filter for whether decoded stream bytes could possibly yield
/// a `BI` operator in `hayro-syntax`'s `Operator::read` (which decodes `#xx`
/// hex escapes; `'B'` = `0x42`, `'I'` = `0x49`, digits only so no case fold).
fn may_contain_inline_image_op(bytes: &[u8]) -> bool {
    contains_subslice(bytes, b"BI")
        || contains_subslice(bytes, b"B#49")
        || contains_subslice(bytes, b"#42I")
        || contains_subslice(bytes, b"#42#49")
}

/// Walks a decoded PDF content stream via `hayro-syntax`'s [`TypedIter`] and
/// validates every `BI ... ID ... EI` inline image against the exact same image
/// dimension, filter chain, and embedded codec header checks as indirect
/// `/XObject /Image` streams (B-469).
fn scan_content_stream_inline_images(
    content_bytes: &[u8],
    budget: &mut Budget,
) -> Result<(), ToolError> {
    let mut iter = TypedIter::new(content_bytes);
    while let Some(op) = iter.next() {
        if let TypedInstruction::InlineImage(inline) = op {
            let _ = validate_hayro_stream(inline.0, budget, true)?;
        }
    }
    Ok(())
}

fn validate_hayro_stream(
    stream: &Stream<'_>,
    budget: &mut Budget,
    force_image: bool,
) -> Result<usize, ToolError> {
    let dict = stream.dict();
    let img_ctx = validate_hayro_image_dict(dict, budget, force_image)?;

    let (filters, is_filter_array) = hayro_dict_filters(dict)?;
    let raw = stream.raw_data();
    if filters.is_empty() {
        return Ok(raw.len());
    }

    let stage_params = hayro_dict_stage_params(dict, is_filter_array, filters.len())?;

    let decoded = check_filter_chain(&raw, &filters, &stage_params, img_ctx, false, true, budget)?;
    Ok(decoded.map_or(raw.len(), |(len, _)| len))
}

/// Resolves `/F` or `/Filter` from a `hayro-syntax` stream dictionary using the
/// exact lookup order of `hayro-syntax`'s `Stream::filters_and_params`, while
/// rejecting parser-differential conflicts when both `/F` and `/Filter` declare
/// different filter lists or when a filter array contains non-`Name` entries
/// (B-469).
fn hayro_dict_filters(dict: &Dict<'_>) -> Result<(Vec<PdfFilter>, bool), ToolError> {
    let parse_filter_obj = |obj: Object<'_>| -> Result<(Vec<PdfFilter>, bool), ToolError> {
        match obj {
            Object::Name(name) => Ok((vec![PdfFilter::from_pdf_name(name.as_ref())], false)),
            Object::Array(arr) => {
                let mut filters = Vec::new();
                for item in arr.iter::<Object<'_>>() {
                    let Object::Name(name) = item else {
                        return Err(ToolError::BillUnreadable);
                    };
                    filters.push(PdfFilter::from_pdf_name(name.as_ref()));
                    if filters.len() > MAX_PDF_FILTER_STAGES {
                        return Err(ToolError::BillTooBig);
                    }
                }
                Ok((filters, true))
            }
            Object::Null(_) => Ok((Vec::new(), false)),
            _ => Err(ToolError::BillUnreadable),
        }
    };

    // Reject external file specification streams (`/F (path)` or `/F << ... >>`):
    // `hayro-syntax` refuses to decode them, whereas `lopdf`/`pdf-extract`
    // ignores `/F` and still decodes `/Filter`, and external file streams are
    // also a local-file-inclusion hazard for standalone bill PDFs (B-469).
    let f_filters = match dict.get::<Object<'_>>(b"F") {
        Some(Object::String(_) | Object::Dict(_)) => return Err(ToolError::BillUnreadable),
        Some(Object::Null(_)) | None => None,
        Some(obj) => Some(parse_filter_obj(obj)?),
    };
    let full_filters = match dict.get::<Object<'_>>(b"Filter") {
        Some(Object::Null(_)) | None => None,
        Some(obj) => Some(parse_filter_obj(obj)?),
    };

    match (f_filters, full_filters) {
        (Some((f_list, f_arr)), Some((full_list, full_arr))) => {
            if f_list != full_list || f_arr != full_arr {
                return Err(ToolError::BillUnreadable);
            }
            Ok((f_list, f_arr))
        }
        (Some(f), None) => Ok(f),
        (None, Some(full)) => Ok(full),
        (None, None) => Ok((Vec::new(), false)),
    }
}

/// Resolves per-stage `/DP` or `/DecodeParms` (single `Dict` or per-stage
/// `Array` of `Dict`/`null`, direct or indirect) matching `hayro-syntax`'s
/// `Array`-vs-`Dict` lookup so a dummy `/DP << >>` cannot shadow a real
/// `/DecodeParms [ ... ]` array (B-469).
fn hayro_dict_stage_params(
    dict: &Dict<'_>,
    is_filter_array: bool,
    filter_count: usize,
) -> Result<Vec<StageParams>, ToolError> {
    let parse_array = |arr: Array<'_>| -> Result<Vec<StageParams>, ToolError> {
        let mut out = Vec::with_capacity(filter_count);
        for item in arr.iter::<Object<'_>>() {
            if out.len() >= MAX_PDF_FILTER_STAGES {
                break;
            }
            match item {
                Object::Dict(d) => out.push(parse_hayro_stage_params(&d)?),
                Object::Null(_) => out.push(StageParams::default()),
                _ => return Err(ToolError::BillUnreadable),
            }
        }
        Ok(out)
    };

    if is_filter_array {
        if let Some(arr) = dict
            .get::<Array<'_>>(b"DP")
            .or_else(|| dict.get::<Array<'_>>(b"DecodeParms"))
        {
            return parse_array(arr);
        }
        if let Some(d) = dict
            .get::<Dict<'_>>(b"DP")
            .or_else(|| dict.get::<Dict<'_>>(b"DecodeParms"))
        {
            return Ok(vec![parse_hayro_stage_params(&d)?]);
        }
    } else {
        if let Some(d) = dict
            .get::<Dict<'_>>(b"DP")
            .or_else(|| dict.get::<Dict<'_>>(b"DecodeParms"))
        {
            return Ok(vec![parse_hayro_stage_params(&d)?]);
        }
        if let Some(arr) = dict
            .get::<Array<'_>>(b"DP")
            .or_else(|| dict.get::<Array<'_>>(b"DecodeParms"))
        {
            return parse_array(arr);
        }
    }

    for key in [b"DP".as_slice(), b"DecodeParms".as_slice()] {
        if let Some(obj) = dict.get::<Object<'_>>(key)
            && !matches!(obj, Object::Null(_))
        {
            return Err(ToolError::BillUnreadable);
        }
    }
    Ok(Vec::new())
}

fn parse_hayro_stage_params(parms: &Dict<'_>) -> Result<StageParams, ToolError> {
    let early_change = parms
        .get::<Number>(b"EarlyChange")
        .is_none_or(|n| n.as_i64() != 0);

    let columns_raw = parms.get::<Number>(b"Columns").map(|n| n.as_i64());
    let ccitt_columns = match columns_raw {
        Some(v) if v < 0 => return Err(ToolError::BillUnreadable),
        Some(v) => Some(v as u64),
        None => None,
    };

    let ccitt_rows = match parms.get::<Number>(b"Rows") {
        Some(n) => {
            let v = n.as_i64();
            if v < 0 {
                return Err(ToolError::BillUnreadable);
            }
            Some(v as u64)
        }
        None => None,
    };

    let predictor_val = parms.get::<Number>(b"Predictor").map_or(1, |n| n.as_i64());
    let colors_raw = parms.get::<Number>(b"Colors").map(|n| n.as_i64());
    let bpc_raw = parms.get::<Number>(b"BitsPerComponent").map(|n| n.as_i64());
    let predictor = compute_predictor_check(predictor_val, columns_raw, colors_raw, bpc_raw);

    Ok(StageParams {
        early_change,
        ccitt_columns,
        ccitt_rows,
        predictor,
    })
}

fn validate_hayro_image_dict(
    dict: &Dict<'_>,
    budget: &mut Budget,
    force_image: bool,
) -> Result<StreamImageContext, ToolError> {
    let is_image = force_image
        || dict
            .get::<Name<'_>>(b"Subtype")
            .is_some_and(|n| n.as_ref() == b"Image")
        || dict
            .get::<bool>(b"IM")
            .or_else(|| dict.get::<bool>(b"ImageMask"))
            .unwrap_or(false);

    // Match `hayro-interpret`'s `ImageXObject` key precedence (`W`/`H` before
    // `Width`/`Height`, `BPC` before `BitsPerComponent`).
    let width_num = dict
        .get::<Number>(b"W")
        .or_else(|| dict.get::<Number>(b"Width"))
        .map(|n| n.as_i64());
    let height_num = dict
        .get::<Number>(b"H")
        .or_else(|| dict.get::<Number>(b"Height"))
        .map(|n| n.as_i64());

    let has_explicit_image_dims =
        dict.get::<Number>(b"Width").is_some() && dict.get::<Number>(b"Height").is_some();

    if !is_image && !has_explicit_image_dims {
        return Ok(StreamImageContext::default());
    }

    if let Some(bpc) = dict
        .get::<Number>(b"BPC")
        .or_else(|| dict.get::<Number>(b"BitsPerComponent"))
    {
        let v = bpc.as_i64();
        if v < 0 || (v as u64) > MAX_PDF_IMAGE_BPC {
            return Err(ToolError::BillUnreadable);
        }
    }

    let width = match width_num {
        Some(w) if w < 0 => return Err(ToolError::BillUnreadable),
        Some(w) => Some(w as u64),
        None => None,
    };
    let height = match height_num {
        Some(h) if h < 0 => return Err(ToolError::BillUnreadable),
        Some(h) => Some(h as u64),
        None => None,
    };

    let pixels_charged = if let (Some(w), Some(h)) = (width, height) {
        budget.check_image_dims(w, h)?
    } else {
        let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
        if width.is_some_and(|w| w > max_dim) || height.is_some_and(|h| h > max_dim) {
            return Err(ToolError::BillTooBig);
        }
        0
    };

    Ok(StreamImageContext {
        width,
        height,
        pixels_charged,
    })
}

fn check_predictor_params(stage: StageParams) -> Result<(), ToolError> {
    match stage.predictor {
        PredictorCheck::None => Ok(()),
        PredictorCheck::Invalid => Err(ToolError::BillUnreadable),
        PredictorCheck::Valid { row_bytes } => {
            if row_bytes > MAX_PDF_STREAM_DECOMPRESSED_BYTES as u64 {
                return Err(ToolError::BillTooBig);
            }
            Ok(())
        }
    }
}

fn check_ccitt_stage_dims(
    stage: StageParams,
    img_ctx: StreamImageContext,
    budget: &mut Budget,
) -> Result<(), ToolError> {
    let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
    if stage.ccitt_columns.is_some_and(|c| c > max_dim)
        || stage.ccitt_rows.is_some_and(|r| r > max_dim)
    {
        return Err(ToolError::BillTooBig);
    }
    // `hayro-syntax`'s CCITT decoder defaults `Columns` to 1728 and uses the
    // image dictionary's `/Height` whenever `height > 0` (else `/Rows`).
    let eff_cols = stage.ccitt_columns.unwrap_or(1728);
    let max_cols = eff_cols.max(img_ctx.width.unwrap_or(0));
    let max_rows = stage
        .ccitt_rows
        .unwrap_or(0)
        .max(img_ctx.height.unwrap_or(0));
    if max_rows > 0 {
        let _ = budget.check_extra_image_dims(max_cols, max_rows, img_ctx.pixels_charged)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Raw PDF scanning helpers (Pass 1 & Pass 2)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn scan_indirect_objects(bytes: &[u8]) -> Result<HashMap<u32, SimpleIndirect>, ToolError> {
    let mut scan_budget = MAX_SPECULATIVE_SCAN_BYTES;
    scan_indirect_objects_with_budget(bytes, &mut scan_budget)
}

fn scan_indirect_objects_with_budget(
    bytes: &[u8],
    scan_budget: &mut usize,
) -> Result<HashMap<u32, SimpleIndirect>, ToolError> {
    let mut map = HashMap::new();
    let mut object_count = 0usize;
    let mut pos = 0usize;

    while pos < bytes.len() {
        if bytes[pos] == b'%' {
            pos = skip_line(bytes, pos);
            continue;
        }
        if bytes[pos] == b'(' {
            if let Some((next, _)) =
                skip_pdf_literal_string_checked(bytes, pos, false, scan_budget)?
            {
                pos = next;
            } else {
                pos += 1;
            }
            continue;
        }
        if bytes[pos] == b'<' && bytes.get(pos + 1) == Some(&b'<') {
            if let Some((dict_end, dict_bytes, _)) = extract_balanced_dict(bytes, pos, scan_budget)?
            {
                let direct_length = extract_direct_length(dict_bytes);
                let after_ws = skip_pdf_ws_and_comments(bytes, dict_end);
                if bytes[after_ws..].starts_with(b"stream")
                    && matches!(bytes.get(after_ws + 6), Some(b'\n' | b'\r'))
                {
                    let data_start = skip_stream_eol(bytes, after_ws + 6);
                    let (_, next_pos, _) = locate_stream_slice(bytes, data_start, direct_length);
                    pos = next_pos;
                    continue;
                }
                pos = dict_end;
                continue;
            }
            pos += 2;
            continue;
        }
        // Also skip bare `stream ... endstream` bodies so binary payloads
        // cannot forge `N G obj ... endobj` definitions (B-469).
        if bytes[pos..].starts_with(b"stream")
            && (pos == 0 || is_pdf_delim_or_ws(bytes[pos - 1]))
            && matches!(bytes.get(pos + 6), Some(b'\n' | b'\r'))
        {
            let data_start = skip_stream_eol(bytes, pos + 6);
            let (_, next_pos, _) = locate_stream_slice(bytes, data_start, None);
            pos = next_pos;
            continue;
        }
        if bytes[pos].is_ascii_digit()
            && (pos == 0 || is_pdf_delim_or_ws(bytes[pos - 1]))
            && let Some((obj_num, after_obj_kw)) = parse_obj_header(bytes, pos)
        {
            object_count += 1;
            if object_count > MAX_PDF_OBJECTS {
                return Err(ToolError::BillTooBig);
            }
            let body_start = skip_pdf_ws_and_comments(bytes, after_obj_kw);
            if let Some((val, next)) =
                parse_simple_indirect_value(bytes, body_start, &map, scan_budget)?
            {
                map.insert(obj_num, val);
                pos = next;
                continue;
            }
            pos = body_start;
            continue;
        }
        pos += 1;
    }

    Ok(map)
}

fn extract_direct_length(dict: &[u8]) -> Option<usize> {
    let empty = HashMap::new();
    let inner = if dict.len() >= 4 && dict.starts_with(b"<<") && dict.ends_with(b">>") {
        &dict[2..dict.len() - 2]
    } else {
        dict
    };
    let mut i = 0usize;
    let mut depth = 0usize;
    while i < inner.len() {
        i = skip_pdf_ws_and_comments(inner, i);
        if i >= inner.len() {
            break;
        }
        match inner[i] {
            b'<' if inner.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
            }
            b'>' if inner.get(i + 1) == Some(&b'>') => {
                depth = depth.saturating_sub(1);
                i += 2;
            }
            b'(' => i = skip_pdf_literal_string(inner, i),
            b'/' => {
                let Some((key, after_key)) = parse_pdf_name(inner, i) else {
                    i += 1;
                    continue;
                };
                let val_pos = skip_pdf_ws_and_comments(inner, after_key);
                if depth == 0
                    && key == b"Length"
                    && let Some((n, _)) = parse_int_or_ref(inner, val_pos, &empty)
                {
                    return usize::try_from(n).ok();
                }
                i = after_key;
            }
            _ => i += 1,
        }
    }
    None
}

fn parse_obj_header(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    let (obj_num, mut i) = parse_u64_ascii(bytes, start)?;
    let obj_num = u32::try_from(obj_num).ok()?;
    let ws1 = skip_pdf_ws(bytes, i);
    if ws1 == i {
        return None;
    }
    i = ws1;
    let (_gen_num, j) = parse_u64_ascii(bytes, i)?;
    let ws2 = skip_pdf_ws(bytes, j);
    if ws2 == j {
        return None;
    }
    if bytes[ws2..].starts_with(b"obj") && bytes.get(ws2 + 3).is_none_or(|&b| is_pdf_delim_or_ws(b))
    {
        Some((obj_num, ws2 + 3))
    } else {
        None
    }
}

fn parse_simple_indirect_value(
    bytes: &[u8],
    start: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
    scan_budget: &mut usize,
) -> Result<Option<(SimpleIndirect, usize)>, ToolError> {
    if start >= bytes.len() {
        return Ok(None);
    }
    let (val, end) = if bytes[start].is_ascii_digit()
        || matches!(bytes[start], b'-' | b'+')
            && bytes.get(start + 1).is_some_and(u8::is_ascii_digit)
    {
        let Some((n, end)) = parse_i64_ascii(bytes, start) else {
            return Ok(None);
        };
        (SimpleIndirect::Int(n), end)
    } else if bytes[start] == b'/' {
        let Some((name, end)) = parse_pdf_name(bytes, start) else {
            return Ok(None);
        };
        (
            SimpleIndirect::Filters(vec![PdfFilter::from_pdf_name(&name)], false),
            end,
        )
    } else if bytes[start] == b'[' {
        if let RawFilterParse::Parsed(filters, _, end) =
            parse_filter_array_checked(bytes, start, indirects)
        {
            (SimpleIndirect::Filters(filters, true), end)
        } else if let Some((params, unresolved, end)) =
            parse_raw_stage_params_array(bytes, start, indirects, scan_budget)?
        {
            if unresolved {
                return Ok(None);
            }
            (SimpleIndirect::StageParams(params, true), end)
        } else {
            return Ok(None);
        }
    } else if bytes[start] == b'<'
        && bytes.get(start + 1) == Some(&b'<')
        && let Some((end, dict_bytes, _)) = extract_balanced_dict(bytes, start, scan_budget)?
    {
        let after = skip_pdf_ws_and_comments(bytes, end);
        // Only index standalone dictionaries (not stream dictionaries).
        if bytes[after..].starts_with(b"endobj") {
            let (params, unresolved) = parse_raw_stage_params_dict(dict_bytes, indirects);
            if unresolved {
                return Ok(None);
            }
            return Ok(Some((
                SimpleIndirect::StageParams(vec![params], false),
                after + 6,
            )));
        }
        return Ok(None);
    } else {
        return Ok(None);
    };
    let after = skip_pdf_ws_and_comments(bytes, end);
    if bytes[after..].starts_with(b"endobj") {
        Ok(Some((val, after + 6)))
    } else {
        Ok(None)
    }
}

/// Extracted metadata from a raw PDF dictionary `<< ... >>`.
struct RawDictInfo {
    is_page: bool,
    is_image: bool,
    is_obj_stm: bool,
    is_xref: bool,
    width: Option<u64>,
    height: Option<u64>,
    bpc: Option<u64>,
    length: Option<usize>,
    prev_offset: Option<usize>,
    xref_stm_offset: Option<usize>,
    filters: Vec<PdfFilter>,
    stage_params: Vec<StageParams>,
    filter_conflict_or_external: bool,
    unresolved_filter_ref: bool,
    invalid_params: bool,
    unresolved_params_ref: bool,
}

enum RawFilterParse {
    Parsed(Vec<PdfFilter>, bool, usize),
    UnresolvedRef(usize),
    ExternalOrInvalid(usize),
}

enum RawStageParamsParse {
    Dict(StageParams, bool, usize),
    Array(Vec<StageParams>, bool, usize),
    UnresolvedRef(usize),
    Invalid,
}

fn is_pdf_null_at(bytes: &[u8], pos: usize) -> bool {
    bytes[pos..].starts_with(b"null") && bytes.get(pos + 4).is_none_or(|&b| is_pdf_delim_or_ws(b))
}

fn skip_pdf_hex_string(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != b'>' {
        i += 1;
    }
    if i < bytes.len() { i + 1 } else { i }
}

fn inspect_raw_dict(
    dict: &[u8],
    indirects: &HashMap<u32, SimpleIndirect>,
    scan_budget: &mut usize,
) -> Result<RawDictInfo, ToolError> {
    let mut info = RawDictInfo {
        is_page: false,
        is_image: false,
        is_obj_stm: false,
        is_xref: false,
        width: None,
        height: None,
        bpc: None,
        length: None,
        prev_offset: None,
        xref_stm_offset: None,
        filters: Vec::new(),
        stage_params: Vec::new(),
        filter_conflict_or_external: false,
        unresolved_filter_ref: false,
        invalid_params: false,
        unresolved_params_ref: false,
    };

    let mut f_filters: Option<(Vec<PdfFilter>, bool)> = None;
    let mut full_filters: Option<(Vec<PdfFilter>, bool)> = None;
    let mut dp_dict: Option<StageParams> = None;
    let mut dp_array: Option<Vec<StageParams>> = None;
    let mut decode_parms_dict: Option<StageParams> = None;
    let mut decode_parms_array: Option<Vec<StageParams>> = None;

    // Strip outer `<<` and `>>`.
    let inner = if dict.len() >= 4 && dict.starts_with(b"<<") && dict.ends_with(b">>") {
        &dict[2..dict.len() - 2]
    } else {
        dict
    };

    let mut i = 0usize;
    let mut depth = 0usize;
    while i < inner.len() {
        i = skip_pdf_ws_and_comments(inner, i);
        if i >= inner.len() {
            break;
        }
        match inner[i] {
            b'<' if inner.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
                continue;
            }
            b'>' if inner.get(i + 1) == Some(&b'>') => {
                depth = depth.saturating_sub(1);
                i += 2;
                continue;
            }
            b'(' => {
                i = skip_pdf_literal_string(inner, i);
                continue;
            }
            b'/' => {
                let Some((key, after_key)) = parse_pdf_name(inner, i) else {
                    i += 1;
                    continue;
                };
                let val_pos = skip_pdf_ws_and_comments(inner, after_key);
                if depth == 0 {
                    match key.as_slice() {
                        b"Type" => {
                            if let Some((name, end)) = parse_pdf_name(inner, val_pos) {
                                if name == b"Page" {
                                    info.is_page = true;
                                } else if name == b"ObjStm" {
                                    info.is_obj_stm = true;
                                } else if name == b"XRef" {
                                    info.is_xref = true;
                                }
                                i = end;
                                continue;
                            }
                        }
                        b"Subtype" => {
                            if let Some((name, end)) = parse_pdf_name(inner, val_pos) {
                                if name == b"Image" {
                                    info.is_image = true;
                                }
                                i = end;
                                continue;
                            }
                        }
                        b"ImageMask" | b"IM" => {
                            if inner[val_pos..].starts_with(b"true") {
                                info.is_image = true;
                                i = val_pos + 4;
                                continue;
                            }
                        }
                        b"Width" | b"W" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                if key == b"W" || info.width.is_none() {
                                    info.width = Some(n);
                                }
                                i = end;
                                continue;
                            }
                        }
                        b"Height" | b"H" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                if key == b"H" || info.height.is_none() {
                                    info.height = Some(n);
                                }
                                i = end;
                                continue;
                            }
                        }
                        b"BitsPerComponent" | b"BPC" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.bpc = Some(n);
                                i = end;
                                continue;
                            }
                        }
                        b"Length" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.length = usize::try_from(n).ok();
                                i = end;
                                continue;
                            }
                        }
                        b"Prev" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.prev_offset = usize::try_from(n).ok();
                                i = end;
                                continue;
                            }
                        }
                        b"XRefStm" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.xref_stm_offset = usize::try_from(n).ok();
                                i = end;
                                continue;
                            }
                        }
                        b"F" | b"Filter" => {
                            if val_pos < inner.len() && is_pdf_null_at(inner, val_pos) {
                                i = val_pos + 4;
                                continue;
                            }
                            match parse_filters_or_ref_checked(
                                inner,
                                val_pos,
                                indirects,
                                scan_budget,
                            )? {
                                RawFilterParse::Parsed(filters, is_arr, end) => {
                                    if key == b"F" {
                                        f_filters = Some((filters, is_arr));
                                    } else {
                                        full_filters = Some((filters, is_arr));
                                    }
                                    i = end;
                                    continue;
                                }
                                RawFilterParse::UnresolvedRef(end) => {
                                    info.unresolved_filter_ref = true;
                                    i = end;
                                    continue;
                                }
                                RawFilterParse::ExternalOrInvalid(end) => {
                                    info.filter_conflict_or_external = true;
                                    i = end.max(after_key);
                                    continue;
                                }
                            }
                        }
                        b"DP" | b"DecodeParms" => {
                            if val_pos < inner.len() && is_pdf_null_at(inner, val_pos) {
                                i = val_pos + 4;
                                continue;
                            }
                            match parse_raw_stage_params_or_ref_checked(
                                inner,
                                val_pos,
                                indirects,
                                scan_budget,
                            )? {
                                RawStageParamsParse::Dict(params, unresolved, end) => {
                                    if unresolved {
                                        info.unresolved_params_ref = true;
                                    }
                                    if key == b"DP" {
                                        dp_dict = Some(params);
                                    } else {
                                        decode_parms_dict = Some(params);
                                    }
                                    i = end;
                                    continue;
                                }
                                RawStageParamsParse::Array(params, unresolved, end) => {
                                    if unresolved {
                                        info.unresolved_params_ref = true;
                                    }
                                    if key == b"DP" {
                                        dp_array = Some(params);
                                    } else {
                                        decode_parms_array = Some(params);
                                    }
                                    i = end;
                                    continue;
                                }
                                RawStageParamsParse::UnresolvedRef(end) => {
                                    info.unresolved_params_ref = true;
                                    i = end;
                                    continue;
                                }
                                RawStageParamsParse::Invalid => {
                                    info.invalid_params = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                i = after_key;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Resolve `/F` vs `/Filter` identically to Layer 2 (`hayro_dict_filters`):
    // any conflict between `/F` and `/Filter` fails closed on streams (B-469).
    let is_filter_array = match (f_filters, full_filters) {
        (Some((f_list, f_arr)), Some((full_list, full_arr))) => {
            if f_list != full_list || f_arr != full_arr {
                info.filter_conflict_or_external = true;
            }
            info.filters = full_list;
            full_arr
        }
        (Some((list, is_arr)), None) | (None, Some((list, is_arr))) => {
            info.filters = list;
            is_arr
        }
        (None, None) => false,
    };

    // Match Layer 2 (`hayro_dict_stage_params`) Array-vs-Dict precedence so a
    // dummy `/DP << >>` cannot shadow `/DecodeParms [ << /EarlyChange 0 >> ]`.
    info.stage_params = if is_filter_array {
        dp_array
            .or(decode_parms_array)
            .or_else(|| dp_dict.or(decode_parms_dict).map(|p| vec![p]))
            .unwrap_or_default()
    } else {
        dp_dict
            .or(decode_parms_dict)
            .map(|p| vec![p])
            .or(dp_array)
            .or(decode_parms_array)
            .unwrap_or_default()
    };

    Ok(info)
}

fn parse_raw_stage_params_or_ref_checked(
    bytes: &[u8],
    pos: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
    scan_budget: &mut usize,
) -> Result<RawStageParamsParse, ToolError> {
    if pos >= bytes.len() {
        return Ok(RawStageParamsParse::Invalid);
    }
    if bytes[pos] == b'<' && bytes.get(pos + 1) == Some(&b'<') {
        if let Some((end, dict_bytes, _)) = extract_balanced_dict(bytes, pos, scan_budget)? {
            let (params, unresolved) = parse_raw_stage_params_dict(dict_bytes, indirects);
            return Ok(RawStageParamsParse::Dict(params, unresolved, end));
        }
        return Ok(RawStageParamsParse::Invalid);
    }
    if bytes[pos] == b'[' {
        if let Some((params, unresolved, end)) =
            parse_raw_stage_params_array(bytes, pos, indirects, scan_budget)?
        {
            return Ok(RawStageParamsParse::Array(params, unresolved, end));
        }
        return Ok(RawStageParamsParse::Invalid);
    }
    if bytes[pos].is_ascii_digit()
        && let Some((obj_num, after_first)) = parse_u64_ascii(bytes, pos)
    {
        let ws1 = skip_pdf_ws(bytes, after_first);
        if let Some((_gen, after_gen)) = parse_u64_ascii(bytes, ws1) {
            let ws2 = skip_pdf_ws(bytes, after_gen);
            if bytes.get(ws2) == Some(&b'R')
                && bytes.get(ws2 + 1).is_none_or(|&b| is_pdf_delim_or_ws(b))
            {
                let end = ws2 + 1;
                if let Ok(id) = u32::try_from(obj_num)
                    && let Some(SimpleIndirect::StageParams(list, is_arr)) = indirects.get(&id)
                {
                    if *is_arr {
                        return Ok(RawStageParamsParse::Array(list.clone(), false, end));
                    }
                    return Ok(RawStageParamsParse::Dict(
                        list.first().copied().unwrap_or_default(),
                        false,
                        end,
                    ));
                }
                return Ok(RawStageParamsParse::UnresolvedRef(end));
            }
        }
    }
    Ok(RawStageParamsParse::Invalid)
}

fn parse_raw_stage_params_array(
    bytes: &[u8],
    start: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
    scan_budget: &mut usize,
) -> Result<Option<(Vec<StageParams>, bool, usize)>, ToolError> {
    if bytes.get(start) != Some(&b'[') {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut unresolved = false;
    let mut i = start + 1;
    while i < bytes.len() {
        i = skip_pdf_ws_and_comments(bytes, i);
        if i >= bytes.len() {
            return Ok(None);
        }
        if bytes[i] == b']' {
            return Ok(Some((out, unresolved, i + 1)));
        }
        if is_pdf_null_at(bytes, i) {
            if out.len() < MAX_PDF_FILTER_STAGES {
                out.push(StageParams::default());
            }
            i += 4;
            continue;
        }
        if bytes[i] == b'<'
            && bytes.get(i + 1) == Some(&b'<')
            && let Some((end, dict_bytes, _)) = extract_balanced_dict(bytes, i, scan_budget)?
        {
            let (params, dict_unresolved) = parse_raw_stage_params_dict(dict_bytes, indirects);
            unresolved |= dict_unresolved;
            if out.len() < MAX_PDF_FILTER_STAGES {
                out.push(params);
            }
            i = end;
            continue;
        }
        if bytes[i].is_ascii_digit()
            && let Some((obj_num, after_first)) = parse_u64_ascii(bytes, i)
        {
            let ws1 = skip_pdf_ws(bytes, after_first);
            if ws1 > after_first
                && let Some((_gen, after_gen)) = parse_u64_ascii(bytes, ws1)
            {
                let ws2 = skip_pdf_ws(bytes, after_gen);
                if ws2 > after_gen
                    && bytes.get(ws2) == Some(&b'R')
                    && bytes.get(ws2 + 1).is_none_or(|&b| is_pdf_delim_or_ws(b))
                {
                    let stage = match u32::try_from(obj_num)
                        .ok()
                        .and_then(|id| indirects.get(&id))
                    {
                        Some(SimpleIndirect::StageParams(v, false)) => {
                            v.first().copied().unwrap_or_default()
                        }
                        _ => {
                            unresolved = true;
                            StageParams::default()
                        }
                    };
                    if out.len() < MAX_PDF_FILTER_STAGES {
                        out.push(stage);
                    }
                    i = ws2 + 1;
                    continue;
                }
            }
        }
        return Ok(None);
    }
    Ok(None)
}

fn parse_raw_stage_params_dict(
    dict: &[u8],
    indirects: &HashMap<u32, SimpleIndirect>,
) -> (StageParams, bool) {
    let mut params = StageParams::default();
    let mut unresolved = false;
    let mut predictor_val = 1i64;
    let mut columns_raw: Option<i64> = None;
    let mut colors_raw: Option<i64> = None;
    let mut bpc_raw: Option<i64> = None;

    let inner = if dict.len() >= 4 && dict.starts_with(b"<<") && dict.ends_with(b">>") {
        &dict[2..dict.len() - 2]
    } else {
        dict
    };

    let mut i = 0usize;
    let mut depth = 0usize;
    while i < inner.len() {
        i = skip_pdf_ws_and_comments(inner, i);
        if i >= inner.len() {
            break;
        }
        match inner[i] {
            b'<' if inner.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
            }
            b'>' if inner.get(i + 1) == Some(&b'>') => {
                depth = depth.saturating_sub(1);
                i += 2;
            }
            b'(' => i = skip_pdf_literal_string(inner, i),
            b'/' => {
                let Some((key, after_key)) = parse_pdf_name(inner, i) else {
                    i += 1;
                    continue;
                };
                let val_pos = skip_pdf_ws_and_comments(inner, after_key);
                if depth == 0 {
                    match key.as_slice() {
                        b"EarlyChange" => match parse_i64_or_ref_checked(inner, val_pos, indirects)
                        {
                            RawIntParse::Value(n, end) => {
                                params.early_change = n != 0;
                                i = end;
                                continue;
                            }
                            RawIntParse::UnresolvedRef(end) => {
                                unresolved = true;
                                i = end;
                                continue;
                            }
                            RawIntParse::Invalid => {}
                        },
                        b"Columns" => match parse_i64_or_ref_checked(inner, val_pos, indirects) {
                            RawIntParse::Value(n, end) => {
                                columns_raw = Some(n);
                                params.ccitt_columns = u64::try_from(n).ok();
                                i = end;
                                continue;
                            }
                            RawIntParse::UnresolvedRef(end) => {
                                unresolved = true;
                                i = end;
                                continue;
                            }
                            RawIntParse::Invalid => {}
                        },
                        b"Rows" => match parse_i64_or_ref_checked(inner, val_pos, indirects) {
                            RawIntParse::Value(n, end) => {
                                params.ccitt_rows = u64::try_from(n).ok();
                                i = end;
                                continue;
                            }
                            RawIntParse::UnresolvedRef(end) => {
                                unresolved = true;
                                i = end;
                                continue;
                            }
                            RawIntParse::Invalid => {}
                        },
                        b"Predictor" => match parse_i64_or_ref_checked(inner, val_pos, indirects) {
                            RawIntParse::Value(n, end) => {
                                predictor_val = n;
                                i = end;
                                continue;
                            }
                            RawIntParse::UnresolvedRef(end) => {
                                unresolved = true;
                                i = end;
                                continue;
                            }
                            RawIntParse::Invalid => {}
                        },
                        b"Colors" => match parse_i64_or_ref_checked(inner, val_pos, indirects) {
                            RawIntParse::Value(n, end) => {
                                colors_raw = Some(n);
                                i = end;
                                continue;
                            }
                            RawIntParse::UnresolvedRef(end) => {
                                unresolved = true;
                                i = end;
                                continue;
                            }
                            RawIntParse::Invalid => {}
                        },
                        b"BitsPerComponent" => {
                            match parse_i64_or_ref_checked(inner, val_pos, indirects) {
                                RawIntParse::Value(n, end) => {
                                    bpc_raw = Some(n);
                                    i = end;
                                    continue;
                                }
                                RawIntParse::UnresolvedRef(end) => {
                                    unresolved = true;
                                    i = end;
                                    continue;
                                }
                                RawIntParse::Invalid => {}
                            }
                        }
                        _ => {}
                    }
                }
                i = after_key;
            }
            _ => i += 1,
        }
    }

    params.predictor = compute_predictor_check(predictor_val, columns_raw, colors_raw, bpc_raw);
    (params, unresolved)
}

fn check_raw_dict_image(
    info: &RawDictInfo,
    budget: &mut Budget,
) -> Result<StreamImageContext, ToolError> {
    let mut pixels_charged = 0u64;
    if info.is_image {
        if let Some(bpc) = info.bpc
            && bpc > MAX_PDF_IMAGE_BPC
        {
            return Err(ToolError::BillUnreadable);
        }
        if let (Some(w), Some(h)) = (info.width, info.height) {
            pixels_charged = budget.check_image_dims(w, h)?;
        } else {
            let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
            if info.width.is_some_and(|w| w > max_dim) || info.height.is_some_and(|h| h > max_dim) {
                return Err(ToolError::BillTooBig);
            }
        }
    }
    let img_ctx = StreamImageContext {
        width: info.width,
        height: info.height,
        pixels_charged,
    };
    for (idx, &filter) in info.filters.iter().enumerate() {
        let stage = stage_params_for(&info.stage_params, idx);
        if filter == PdfFilter::Ccitt {
            check_ccitt_stage_dims(stage, img_ctx, budget)?;
        } else if matches!(filter, PdfFilter::Flate | PdfFilter::Lzw) {
            check_predictor_params(stage)?;
        }
    }
    Ok(img_ctx)
}

fn scan_obj_stm_dicts(
    bytes: &[u8],
    indirects: &HashMap<u32, SimpleIndirect>,
    budget: &mut Budget,
    scan_budget: &mut usize,
) -> Result<(), ToolError> {
    let mut pos = 0usize;
    while pos < bytes.len() {
        if bytes[pos] == b'%' {
            let line_end = skip_line(bytes, pos);
            if let Some(rel) = find_subslice(&bytes[pos..line_end], b"<<") {
                pos += rel;
            } else {
                pos = line_end;
            }
            continue;
        }
        if bytes[pos] == b'(' {
            if let Some((next, _)) = skip_pdf_literal_string_checked(bytes, pos, true, scan_budget)?
            {
                pos = next;
            } else {
                pos += 1;
            }
            continue;
        }
        if bytes[pos] == b'<' && bytes.get(pos + 1) == Some(&b'<') {
            if let Some((dict_end, dict_bytes, saw_dict_open_in_string)) =
                extract_balanced_dict(bytes, pos, scan_budget)?
            {
                let info = inspect_raw_dict(dict_bytes, indirects, scan_budget)?;
                let _ = check_raw_dict_image(&info, budget)?;
                if saw_dict_open_in_string {
                    charge_speculative_scan(scan_budget, dict_end.saturating_sub(pos))?;
                    pos += 2;
                } else {
                    pos = dict_end;
                }
                continue;
            }
            pos += 2;
            continue;
        }
        pos += 1;
    }
    Ok(())
}

/// Returns whether `bytes[pos..]` starts with a standalone `stream\r` or
/// `stream\n` boundary token.
fn is_stream_boundary(bytes: &[u8], pos: usize) -> bool {
    if pos > 0 && !is_pdf_delim_or_ws(bytes[pos - 1]) {
        return false;
    }
    bytes[pos..].starts_with(b"stream") && matches!(bytes.get(pos + 6), Some(b'\n' | b'\r'))
}

/// Returns whether `bytes[pos..]` starts with a standalone `stream\r`/`stream\n`
/// or `endobj` boundary token (used so unclosed `<<` cannot swallow following
/// indirect objects or streams, B-469).
fn is_object_or_stream_boundary(bytes: &[u8], pos: usize) -> bool {
    if is_stream_boundary(bytes, pos) {
        return true;
    }
    if pos > 0 && !is_pdf_delim_or_ws(bytes[pos - 1]) {
        return false;
    }
    bytes[pos..].starts_with(b"endobj") && bytes.get(pos + 6).is_none_or(|&b| is_pdf_delim_or_ws(b))
}

type BalancedDict<'a> = (usize, &'a [u8], bool);

fn extract_balanced_dict<'a>(
    bytes: &'a [u8],
    start: usize,
    scan_budget: &mut usize,
) -> Result<Option<BalancedDict<'a>>, ToolError> {
    if !bytes[start..].starts_with(b"<<") {
        return Ok(None);
    }
    let mut i = start;
    let mut depth = 0usize;
    let mut saw_dict_open_in_string = false;
    // Cap scan window for a single dictionary to 1 MiB and charge failed scans
    // against `scan_budget` to avoid quadratic scans on repeated unclosed `<<`.
    let limit = bytes.len().min(start.saturating_add(1024 * 1024));
    while i < limit {
        match bytes[i] {
            b'%' => i = skip_line(bytes, i),
            b'(' => {
                // Bail out if an unclosed/boundary-crossing literal string
                // appears inside the candidate dictionary (B-469).
                let Some((next, inner_has_dict_open)) =
                    skip_pdf_literal_string_checked(bytes, i, false, scan_budget)?
                else {
                    charge_speculative_scan(scan_budget, i.saturating_sub(start))?;
                    return Ok(None);
                };
                saw_dict_open_in_string |= inner_has_dict_open;
                i = next;
            }
            b'<' if bytes.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
            }
            b'>' if bytes.get(i + 1) == Some(&b'>') => {
                depth = depth.saturating_sub(1);
                i += 2;
                if depth == 0 {
                    return Ok(Some((i, &bytes[start..i], saw_dict_open_in_string)));
                }
            }
            b's' | b'e' if is_object_or_stream_boundary(bytes, i) => {
                // An unclosed `<<` must never span across `stream` or `endobj`.
                charge_speculative_scan(scan_budget, i.saturating_sub(start))?;
                return Ok(None);
            }
            _ => i += 1,
        }
    }
    charge_speculative_scan(scan_budget, i.saturating_sub(start))?;
    Ok(None)
}

fn locate_stream_slice(
    bytes: &[u8],
    data_start: usize,
    declared_len: Option<usize>,
) -> (&[u8], usize, bool) {
    if let Some(len) = declared_len
        && let Some(end) = data_start.checked_add(len)
        && end <= bytes.len()
    {
        let after = skip_pdf_ws(bytes, end);
        if bytes[after..].starts_with(b"endstream") {
            return (&bytes[data_start..end], after + 9, false);
        }
    }
    if let Some(rel_end) = find_subslice(&bytes[data_start..], b"endstream") {
        let mut end = data_start + rel_end;
        // Strip single trailing CR/LF before `endstream`.
        if end > data_start && bytes[end - 1] == b'\n' {
            end -= 1;
            if end > data_start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
        } else if end > data_start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        return (&bytes[data_start..end], data_start + rel_end + 9, true);
    }
    (&bytes[data_start..], bytes.len(), true)
}

fn skip_stream_eol(bytes: &[u8], pos: usize) -> usize {
    if bytes.get(pos) == Some(&b'\r') && bytes.get(pos + 1) == Some(&b'\n') {
        pos + 2
    } else if matches!(bytes.get(pos), Some(b'\n' | b'\r')) {
        pos + 1
    } else {
        pos
    }
}

enum RawIntParse {
    Value(i64, usize),
    UnresolvedRef(usize),
    Invalid,
}

fn parse_i64_or_ref_checked(
    bytes: &[u8],
    pos: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
) -> RawIntParse {
    if pos >= bytes.len() {
        return RawIntParse::Invalid;
    }
    if bytes[pos].is_ascii_digit() {
        let Some((first, after_first)) = parse_u64_ascii(bytes, pos) else {
            return RawIntParse::Invalid;
        };
        let ws1 = skip_pdf_ws(bytes, after_first);
        if ws1 > after_first
            && let Some((_gen, after_gen)) = parse_u64_ascii(bytes, ws1)
        {
            let ws2 = skip_pdf_ws(bytes, after_gen);
            if ws2 > after_gen
                && bytes.get(ws2) == Some(&b'R')
                && bytes.get(ws2 + 1).is_none_or(|&b| is_pdf_delim_or_ws(b))
            {
                let end = ws2 + 1;
                return match u32::try_from(first).ok().and_then(|id| indirects.get(&id)) {
                    Some(SimpleIndirect::Int(v)) => RawIntParse::Value(*v, end),
                    _ => RawIntParse::UnresolvedRef(end),
                };
            }
        }
        return RawIntParse::Value(i64::try_from(first).unwrap_or(i64::MAX), after_first);
    }
    match parse_i64_ascii(bytes, pos) {
        Some((v, end)) => RawIntParse::Value(v, end),
        None => RawIntParse::Invalid,
    }
}

fn parse_int_or_ref(
    bytes: &[u8],
    pos: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
) -> Option<(u64, usize)> {
    let (first, after_first) = parse_u64_ascii(bytes, pos)?;
    let ws1 = skip_pdf_ws(bytes, after_first);
    if ws1 > after_first
        && let Some((_gen, after_gen)) = parse_u64_ascii(bytes, ws1)
    {
        let ws2 = skip_pdf_ws(bytes, after_gen);
        if ws2 > after_gen
            && bytes.get(ws2) == Some(&b'R')
            && bytes.get(ws2 + 1).is_none_or(|&b| is_pdf_delim_or_ws(b))
        {
            let obj_num = u32::try_from(first).ok()?;
            return match indirects.get(&obj_num) {
                Some(SimpleIndirect::Int(v)) => Some((u64::try_from(*v).ok()?, ws2 + 1)),
                _ => None,
            };
        }
    }
    Some((first, after_first))
}

fn parse_filters_or_ref_checked(
    bytes: &[u8],
    pos: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
    scan_budget: &mut usize,
) -> Result<RawFilterParse, ToolError> {
    if pos >= bytes.len() {
        return Ok(RawFilterParse::ExternalOrInvalid(pos));
    }
    match bytes[pos] {
        b'/' => {
            if let Some((name, end)) = parse_pdf_name(bytes, pos) {
                Ok(RawFilterParse::Parsed(
                    vec![PdfFilter::from_pdf_name(&name)],
                    false,
                    end,
                ))
            } else {
                Ok(RawFilterParse::ExternalOrInvalid(pos + 1))
            }
        }
        b'[' => Ok(parse_filter_array_checked(bytes, pos, indirects)),
        b'(' => Ok(RawFilterParse::ExternalOrInvalid(skip_pdf_literal_string(
            bytes, pos,
        ))),
        b'<' => {
            if bytes.get(pos + 1) == Some(&b'<') {
                if let Some((end, _, _)) = extract_balanced_dict(bytes, pos, scan_budget)? {
                    return Ok(RawFilterParse::ExternalOrInvalid(end));
                }
                return Ok(RawFilterParse::ExternalOrInvalid(pos + 2));
            }
            Ok(RawFilterParse::ExternalOrInvalid(skip_pdf_hex_string(
                bytes, pos,
            )))
        }
        b if b.is_ascii_digit() => {
            if let Some((obj_num, after_first)) = parse_u64_ascii(bytes, pos) {
                let ws1 = skip_pdf_ws(bytes, after_first);
                if let Some((_gen, after_gen)) = parse_u64_ascii(bytes, ws1) {
                    let ws2 = skip_pdf_ws(bytes, after_gen);
                    if bytes.get(ws2) == Some(&b'R')
                        && bytes.get(ws2 + 1).is_none_or(|&b| is_pdf_delim_or_ws(b))
                    {
                        let end = ws2 + 1;
                        if let Ok(id) = u32::try_from(obj_num) {
                            match indirects.get(&id) {
                                Some(SimpleIndirect::Filters(list, is_arr)) => {
                                    return Ok(RawFilterParse::Parsed(list.clone(), *is_arr, end));
                                }
                                Some(
                                    SimpleIndirect::StageParams(_, _) | SimpleIndirect::Int(_),
                                ) => {
                                    return Ok(RawFilterParse::ExternalOrInvalid(end));
                                }
                                None => return Ok(RawFilterParse::UnresolvedRef(end)),
                            }
                        }
                        return Ok(RawFilterParse::UnresolvedRef(end));
                    }
                }
                return Ok(RawFilterParse::ExternalOrInvalid(after_first));
            }
            Ok(RawFilterParse::ExternalOrInvalid(pos + 1))
        }
        _ => Ok(RawFilterParse::ExternalOrInvalid(pos + 1)),
    }
}

fn parse_filter_array_checked(
    bytes: &[u8],
    start: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
) -> RawFilterParse {
    if bytes.get(start) != Some(&b'[') {
        return RawFilterParse::ExternalOrInvalid(start);
    }
    let mut filters = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() {
        i = skip_pdf_ws_and_comments(bytes, i);
        if i >= bytes.len() {
            return RawFilterParse::ExternalOrInvalid(i);
        }
        if bytes[i] == b']' {
            return RawFilterParse::Parsed(filters, true, i + 1);
        }
        if bytes[i] == b'/' {
            let Some((name, end)) = parse_pdf_name(bytes, i) else {
                return RawFilterParse::ExternalOrInvalid(i + 1);
            };
            filters.push(PdfFilter::from_pdf_name(&name));
            if filters.len() > MAX_PDF_FILTER_STAGES + 1 {
                return RawFilterParse::Parsed(filters, true, end);
            }
            i = end;
        } else if bytes[i].is_ascii_digit()
            && let Some((obj_num, after_first)) = parse_u64_ascii(bytes, i)
        {
            let ws1 = skip_pdf_ws(bytes, after_first);
            if let Some((_gen, after_gen)) = parse_u64_ascii(bytes, ws1) {
                let ws2 = skip_pdf_ws(bytes, after_gen);
                if bytes.get(ws2) == Some(&b'R')
                    && bytes.get(ws2 + 1).is_none_or(|&b| is_pdf_delim_or_ws(b))
                {
                    if let Ok(id) = u32::try_from(obj_num)
                        && let Some(SimpleIndirect::Filters(list, false)) = indirects.get(&id)
                        && let Some(&first) = list.first()
                    {
                        filters.push(first);
                        if filters.len() > MAX_PDF_FILTER_STAGES + 1 {
                            return RawFilterParse::Parsed(filters, true, ws2 + 1);
                        }
                        i = ws2 + 1;
                        continue;
                    }
                    return RawFilterParse::UnresolvedRef(ws2 + 1);
                }
            }
            return RawFilterParse::ExternalOrInvalid(after_first);
        } else {
            return RawFilterParse::ExternalOrInvalid(i + 1);
        }
    }
    RawFilterParse::ExternalOrInvalid(i)
}

/// Parses a PDF name starting at `/`, decoding `#xx` hex escapes so
/// obfuscated names like `/Fl#61teDecode` are recognized identically to
/// `/FlateDecode`.
fn parse_pdf_name(bytes: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    if bytes.get(start) != Some(&b'/') {
        return None;
    }
    let mut out = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() && !is_pdf_delim_or_ws(bytes[i]) {
        if bytes[i] == b'#'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
        if out.len() > 256 {
            break;
        }
    }
    Some((out, i))
}

fn parse_u64_ascii(bytes: &[u8], start: usize) -> Option<(u64, usize)> {
    let mut i = start;
    let mut val = 0u64;
    let mut digits = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        val = val
            .saturating_mul(10)
            .saturating_add(u64::from(bytes[i] - b'0'));
        i += 1;
        digits += 1;
    }
    (digits > 0).then_some((val, i))
}

fn parse_i64_ascii(bytes: &[u8], start: usize) -> Option<(i64, usize)> {
    if start >= bytes.len() {
        return None;
    }
    let (neg, digit_start) = match bytes[start] {
        b'-' => (true, start + 1),
        b'+' => (false, start + 1),
        _ => (false, start),
    };
    let (mag, end) = parse_u64_ascii(bytes, digit_start)?;
    let signed = if neg {
        if mag > (i64::MAX as u64) {
            i64::MIN
        } else {
            -(mag as i64)
        }
    } else {
        i64::try_from(mag).unwrap_or(i64::MAX)
    };
    Some((signed, end))
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_pdf_ws(b: u8) -> bool {
    matches!(b, 0x00 | b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

fn is_pdf_delim_or_ws(b: u8) -> bool {
    is_pdf_ws(b)
        || matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
}

fn skip_pdf_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_pdf_ws(bytes[i]) {
        i += 1;
    }
    i
}

fn skip_line(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
        i += 1;
    }
    if i < bytes.len() {
        i += 1;
    }
    i
}

fn skip_pdf_ws_and_comments(bytes: &[u8], mut i: usize) -> usize {
    loop {
        i = skip_pdf_ws(bytes, i);
        if bytes.get(i) == Some(&b'%') {
            i = skip_line(bytes, i);
        } else {
            return i;
        }
    }
}

fn skip_pdf_literal_string(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    let mut depth = 1usize;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'\\' => i = (i + 2).min(bytes.len()),
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    i
}

/// Skips a balanced PDF literal string `(...)` (ISO 32000-1 §7.3.4.2),
/// returning `Ok(None)` if the string is unclosed, exceeds 1 MiB, crosses a
/// `stream\r/\n` boundary, or (when `stop_on_dict_open` is `true`) contains
/// `<<` so a crafted string cannot hide a following `/XRef` or `/ObjStm` bomb
/// (B-469). Failed scans are charged against `scan_budget`.
fn skip_pdf_literal_string_checked(
    bytes: &[u8],
    start: usize,
    stop_on_dict_open: bool,
    scan_budget: &mut usize,
) -> Result<Option<(usize, bool)>, ToolError> {
    if bytes.get(start) != Some(&b'(') {
        return Ok(None);
    }
    let mut i = start + 1;
    let mut depth = 1usize;
    let mut saw_dict_open = false;
    let limit = bytes.len().min(start.saturating_add(1024 * 1024));
    while i < limit && depth > 0 {
        match bytes[i] {
            b'\\' => {
                if i + 1 >= limit {
                    charge_speculative_scan(scan_budget, i.saturating_sub(start))?;
                    return Ok(None);
                }
                i += 2;
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            b'<' if bytes.get(i + 1) == Some(&b'<') => {
                if stop_on_dict_open {
                    charge_speculative_scan(scan_budget, i.saturating_sub(start))?;
                    return Ok(None);
                }
                saw_dict_open = true;
                i += 2;
            }
            b's' if is_stream_boundary(bytes, i) => {
                charge_speculative_scan(scan_budget, i.saturating_sub(start))?;
                return Ok(None);
            }
            _ => i += 1,
        }
    }
    if depth == 0 {
        Ok(Some((i, saw_dict_open)))
    } else {
        charge_speculative_scan(scan_budget, i.saturating_sub(start))?;
        Ok(None)
    }
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    find_subslice(haystack, needle).is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn read_be_u16(data: &[u8], offset: usize) -> Option<u16> {
    let slice: [u8; 2] = data.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
    Some(u16::from_be_bytes(slice))
}

fn read_be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let slice: [u8; 4] = data.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_be_bytes(slice))
}

fn read_be_u64(data: &[u8], offset: usize) -> Option<u64> {
    let slice: [u8; 8] = data.get(offset..offset.checked_add(8)?)?.try_into().ok()?;
    Some(u64::from_be_bytes(slice))
}

// ---------------------------------------------------------------------------
// Pure-Rust embedded image codec header validators (DCT, JPX, JBIG2, B-469)
// ---------------------------------------------------------------------------

/// Parses embedded JPEG (DCTDecode) frame dimensions from SOF0..SOF15 markers
/// using the exact marker scan order of `hayro-syntax`'s `dct.rs`.
fn parse_jpeg_dimensions(data: &[u8]) -> Result<(u64, u64), ToolError> {
    let mut i = 0usize;
    while i.checked_add(1).is_some_and(|next| next < data.len()) {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        match marker {
            // SOF0..SOF15 excluding DHT (0xC4), JPG (0xC8), DAC (0xCC).
            0xC0..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                let seg_len = read_be_u16(data, i + 2).ok_or(ToolError::BillUnreadable)?;
                if seg_len < 8 {
                    return Err(ToolError::BillUnreadable);
                }
                let height = read_be_u16(data, i + 5).ok_or(ToolError::BillUnreadable)?;
                let width = read_be_u16(data, i + 7).ok_or(ToolError::BillUnreadable)?;
                if width == 0 || height == 0 {
                    return Err(ToolError::BillUnreadable);
                }
                return Ok((u64::from(width), u64::from(height)));
            }
            0xFF => {
                i += 1;
            }
            0xD8 | 0xD9 | 0x01 | 0x00 | 0xD0..=0xD7 => {
                i += 2;
            }
            _ => {
                let seg_len = read_be_u16(data, i + 2).ok_or(ToolError::BillUnreadable)?;
                if seg_len < 2 {
                    return Err(ToolError::BillUnreadable);
                }
                i = i
                    .checked_add(2)
                    .and_then(|p| p.checked_add(usize::from(seg_len)))
                    .ok_or(ToolError::BillUnreadable)?;
            }
        }
    }
    Err(ToolError::BillUnreadable)
}

/// Parses embedded JPEG 2000 (JPXDecode) dimensions from either a raw J2K
/// codestream (`FF 4F FF 51`) or a JP2 container (`jP  ` box), checking both
/// `ihdr` (if present) and every `jp2c` codestream `SIZ` header.
fn parse_jpx_dimensions(data: &[u8]) -> Result<(u64, u64), ToolError> {
    const JP2_MAGIC: &[u8] = b"\x00\x00\x00\x0C\x6A\x50\x20\x20";
    const J2K_MAGIC: &[u8] = b"\xFF\x4F\xFF\x51";
    const BOX_JP2H: u32 = 0x6A70_3268;
    const BOX_IHDR: u32 = 0x6968_6472;
    const BOX_JP2C: u32 = 0x6A70_3263;

    if data.starts_with(J2K_MAGIC) {
        return parse_j2k_siz_dimensions(data);
    }
    if !data.starts_with(JP2_MAGIC) {
        return Err(ToolError::BillUnreadable);
    }

    let mut pos = 0usize;
    let mut boxes_seen = 0usize;
    let mut max_w = 0u64;
    let mut max_h = 0u64;
    let mut found_codestream = false;
    let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);

    while pos < data.len() {
        boxes_seen += 1;
        if boxes_seen > MAX_JP2_BOXES {
            return Err(ToolError::BillTooBig);
        }
        let Some((box_type, payload, next_pos)) = read_jp2_box(data, pos) else {
            break;
        };
        pos = next_pos;

        if box_type == BOX_JP2H {
            let mut inner = 0usize;
            let mut sub_boxes = 0usize;
            while inner < payload.len() {
                sub_boxes += 1;
                if sub_boxes > MAX_JP2_BOXES {
                    return Err(ToolError::BillTooBig);
                }
                let Some((sub_type, sub_payload, sub_next)) = read_jp2_box(payload, inner) else {
                    break;
                };
                inner = sub_next;
                if sub_type == BOX_IHDR {
                    let h =
                        u64::from(read_be_u32(sub_payload, 0).ok_or(ToolError::BillUnreadable)?);
                    let w =
                        u64::from(read_be_u32(sub_payload, 4).ok_or(ToolError::BillUnreadable)?);
                    let nc = read_be_u16(sub_payload, 8).ok_or(ToolError::BillUnreadable)?;
                    if w == 0 || h == 0 || nc == 0 {
                        return Err(ToolError::BillUnreadable);
                    }
                    if w > max_dim
                        || h > max_dim
                        || w.saturating_mul(h) > MAX_PDF_IMAGE_PIXELS
                        || nc > MAX_JPX_COMPONENTS
                    {
                        return Err(ToolError::BillTooBig);
                    }
                    max_w = max_w.max(w);
                    max_h = max_h.max(h);
                }
            }
        } else if box_type == BOX_JP2C {
            let (w, h) = parse_j2k_siz_dimensions(payload)?;
            if w > max_dim || h > max_dim || w.saturating_mul(h) > MAX_PDF_IMAGE_PIXELS {
                return Err(ToolError::BillTooBig);
            }
            max_w = max_w.max(w);
            max_h = max_h.max(h);
            found_codestream = true;
        }
    }

    if !found_codestream || max_w == 0 || max_h == 0 {
        return Err(ToolError::BillUnreadable);
    }
    Ok((max_w, max_h))
}

fn read_jp2_box(data: &[u8], pos: usize) -> Option<(u32, &[u8], usize)> {
    let l_box = read_be_u32(data, pos)?;
    let t_box = read_be_u32(data, pos + 4)?;
    match l_box {
        0 => Some((t_box, &data[pos + 8..], data.len())),
        1 => {
            let xl_box = read_be_u64(data, pos + 8)?;
            let payload_len = usize::try_from(xl_box.checked_sub(16)?).ok()?;
            let start = pos + 16;
            let end = start.saturating_add(payload_len).min(data.len());
            Some((t_box, &data[start..end], end))
        }
        _ => {
            let payload_len = usize::try_from(l_box.checked_sub(8)?).ok()?;
            let start = pos + 8;
            let end = start.saturating_add(payload_len).min(data.len());
            Some((t_box, &data[start..end], end))
        }
    }
}

/// Parses a JPEG 2000 codestream `SOC (FF 4F) + SIZ (FF 51)` header (ISO/IEC
/// 15444-1 Annex A.5.1), returning `(width, height)`.
fn parse_j2k_siz_dimensions(data: &[u8]) -> Result<(u64, u64), ToolError> {
    if data.len() < 42 || !data.starts_with(b"\xFF\x4F\xFF\x51") {
        return Err(ToolError::BillUnreadable);
    }
    let lsiz = read_be_u16(data, 4).ok_or(ToolError::BillUnreadable)?;
    if lsiz < 38 {
        return Err(ToolError::BillUnreadable);
    }
    let xsiz = read_be_u32(data, 8).ok_or(ToolError::BillUnreadable)?;
    let ysiz = read_be_u32(data, 12).ok_or(ToolError::BillUnreadable)?;
    let x_osiz = read_be_u32(data, 16).ok_or(ToolError::BillUnreadable)?;
    let y_osiz = read_be_u32(data, 20).ok_or(ToolError::BillUnreadable)?;
    let xt_siz = read_be_u32(data, 24).ok_or(ToolError::BillUnreadable)?;
    let yt_siz = read_be_u32(data, 28).ok_or(ToolError::BillUnreadable)?;
    let xto_siz = read_be_u32(data, 32).ok_or(ToolError::BillUnreadable)?;
    let yto_siz = read_be_u32(data, 36).ok_or(ToolError::BillUnreadable)?;
    let csiz = read_be_u16(data, 40).ok_or(ToolError::BillUnreadable)?;

    if xsiz <= x_osiz
        || ysiz <= y_osiz
        || xt_siz == 0
        || yt_siz == 0
        || xto_siz >= xsiz
        || yto_siz >= ysiz
        || xto_siz > x_osiz
        || yto_siz > y_osiz
        || csiz == 0
    {
        return Err(ToolError::BillUnreadable);
    }
    if csiz > MAX_JPX_COMPONENTS {
        return Err(ToolError::BillTooBig);
    }
    let nx_tiles = u64::from(xsiz - xto_siz).div_ceil(u64::from(xt_siz));
    let ny_tiles = u64::from(ysiz - yto_siz).div_ceil(u64::from(yt_siz));
    if nx_tiles.saturating_mul(ny_tiles) > MAX_JPX_TILES {
        return Err(ToolError::BillTooBig);
    }

    let width = u64::from(xsiz - x_osiz);
    let height = u64::from(ysiz - y_osiz);
    Ok((width, height))
}

/// Parses an embedded PDF JBIG2 stream (ITU-T T.88 Annex D.3, sequential
/// segment headers without file header) and validates every `PageInformation`
/// (type 48), `EndOfStripe` (type 50), and region segment (types 4, 6, 7, 20,
/// 22, 23, 36, 38, 39, 40, 42, 43) against dimension and pixel caps (B-469).
fn parse_jbig2_dimensions(data: &[u8]) -> Result<(u64, u64), ToolError> {
    let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
    let mut pos = 0usize;
    let mut seg_count = 0usize;
    let mut found_page_info = false;
    let mut has_unknown_height = false;
    let mut unknown_height_max_w = 0u64;
    let mut max_stripe_h = 0u64;
    let mut max_w = 0u64;
    let mut max_h = 0u64;

    while pos < data.len() {
        seg_count += 1;
        if seg_count > MAX_JBIG2_SEGMENTS {
            return Err(ToolError::BillTooBig);
        }

        let seg_num = read_be_u32(data, pos).ok_or(ToolError::BillUnreadable)?;
        let flags = *data.get(pos + 4).ok_or(ToolError::BillUnreadable)?;
        let seg_type = flags & 0x3F;
        let page_assoc_long = (flags & 0x40) != 0;
        let count_and_ret = *data.get(pos + 5).ok_or(ToolError::BillUnreadable)?;
        pos += 6;

        let short_count = (count_and_ret >> 5) & 0x07;
        if short_count == 5 || short_count == 6 {
            return Err(ToolError::BillUnreadable);
        }
        let ref_count: usize = if short_count < 7 {
            usize::from(short_count)
        } else {
            let b = data.get(pos..pos + 3).ok_or(ToolError::BillUnreadable)?;
            pos += 3;
            let count = u32::from_be_bytes([count_and_ret & 0x1F, b[0], b[1], b[2]]) as usize;
            let ret_bytes = count.saturating_add(1).div_ceil(8);
            pos = pos
                .checked_add(ret_bytes)
                .ok_or(ToolError::BillUnreadable)?;
            if pos > data.len() {
                return Err(ToolError::BillUnreadable);
            }
            count
        };

        let ref_num_size = if seg_num <= 256 {
            1usize
        } else if seg_num <= 65_536 {
            2usize
        } else {
            4usize
        };
        let ref_bytes = ref_count
            .checked_mul(ref_num_size)
            .ok_or(ToolError::BillUnreadable)?;
        pos = pos
            .checked_add(ref_bytes)
            .ok_or(ToolError::BillUnreadable)?;
        let page_assoc_bytes = if page_assoc_long { 4usize } else { 1usize };
        pos = pos
            .checked_add(page_assoc_bytes)
            .ok_or(ToolError::BillUnreadable)?;

        let data_len_raw = read_be_u32(data, pos).ok_or(ToolError::BillUnreadable)?;
        pos += 4;

        let seg_len = if data_len_raw == 0xFFFF_FFFF {
            scan_jbig2_unknown_region_len(&data[pos..])?
        } else {
            usize::try_from(data_len_raw).map_err(|_| ToolError::BillUnreadable)?
        };
        let seg_end = pos.checked_add(seg_len).ok_or(ToolError::BillUnreadable)?;
        let seg_data = data.get(pos..seg_end).ok_or(ToolError::BillUnreadable)?;
        pos = seg_end;

        match seg_type {
            // Page information segment (7.4.8)
            48 => {
                if seg_data.len() < 19 {
                    return Err(ToolError::BillUnreadable);
                }
                let w = u64::from(read_be_u32(seg_data, 0).ok_or(ToolError::BillUnreadable)?);
                let h_raw = read_be_u32(seg_data, 4).ok_or(ToolError::BillUnreadable)?;
                if w == 0 {
                    return Err(ToolError::BillUnreadable);
                }
                if w > max_dim {
                    return Err(ToolError::BillTooBig);
                }
                found_page_info = true;
                if h_raw == 0xFFFF_FFFF {
                    has_unknown_height = true;
                    unknown_height_max_w = unknown_height_max_w.max(w);
                } else {
                    let h = u64::from(h_raw);
                    if h == 0 {
                        return Err(ToolError::BillUnreadable);
                    }
                    if h > max_dim || w.saturating_mul(h) > MAX_PDF_IMAGE_PIXELS {
                        return Err(ToolError::BillTooBig);
                    }
                    max_w = max_w.max(w);
                    max_h = max_h.max(h);
                }
            }
            // End of stripe segment (7.4.10)
            50 => {
                if seg_data.len() != 4 {
                    return Err(ToolError::BillUnreadable);
                }
                let end_row = read_be_u32(seg_data, 0).ok_or(ToolError::BillUnreadable)?;
                let stripe_h = u64::from(end_row).saturating_add(1);
                if stripe_h > max_dim {
                    return Err(ToolError::BillTooBig);
                }
                max_stripe_h = max_stripe_h.max(stripe_h);
            }
            // Region segments (7.4.1): width (u32) + height (u32) at bytes 0..8
            4 | 6 | 7 | 20 | 22 | 23 | 36 | 38 | 39 | 40 | 42 | 43 => {
                if seg_data.len() < 17 {
                    return Err(ToolError::BillUnreadable);
                }
                let rw = u64::from(read_be_u32(seg_data, 0).ok_or(ToolError::BillUnreadable)?);
                let rh = u64::from(read_be_u32(seg_data, 4).ok_or(ToolError::BillUnreadable)?);
                if rw > max_dim || rh > max_dim || rw.saturating_mul(rh) > MAX_PDF_IMAGE_PIXELS {
                    return Err(ToolError::BillTooBig);
                }
            }
            // End of file segment (7.4.11)
            51 => break,
            _ => {}
        }
    }

    if !found_page_info {
        return Err(ToolError::BillUnreadable);
    }
    if has_unknown_height {
        if max_stripe_h == 0 {
            return Err(ToolError::BillUnreadable);
        }
        if unknown_height_max_w.saturating_mul(max_stripe_h) > MAX_PDF_IMAGE_PIXELS {
            return Err(ToolError::BillTooBig);
        }
        max_w = max_w.max(unknown_height_max_w);
        max_h = max_h.max(max_stripe_h);
    }
    if max_w == 0 || max_h == 0 {
        return Err(ToolError::BillUnreadable);
    }
    Ok((max_w, max_h))
}

/// Scans an immediate generic region segment whose header declared unknown
/// length (`0xFFFFFFFF`), matching `hayro-jbig2`'s end-marker scan (7.2.7).
fn scan_jbig2_unknown_region_len(data: &[u8]) -> Result<usize, ToolError> {
    let flags = *data.get(17).ok_or(ToolError::BillUnreadable)?;
    let uses_mmr = (flags & 1) != 0;
    let at_bytes = match (uses_mmr, (flags >> 1) & 0x03, (flags & 0x10) != 0) {
        (true, _, _) => 0usize,
        (false, 0, false) => 8usize,
        (false, 0, true) => 24usize,
        (false, _, _) => 2usize,
    };
    let mut scan = 18usize.saturating_add(at_bytes);
    let end_marker: [u8; 2] = if uses_mmr { [0x00, 0x00] } else { [0xFF, 0xAC] };
    while scan.checked_add(6).is_some_and(|end| end <= data.len()) {
        if data[scan..scan + 2] == end_marker {
            return Ok(scan + 6);
        }
        scan += 1;
    }
    Err(ToolError::BillUnreadable)
}

// ---------------------------------------------------------------------------
// Bounded stream filter chain & decoders (RFC 1950/1951 Flate, LZW, RLE, ASCII)
// ---------------------------------------------------------------------------

fn stage_params_for(stage_params: &[StageParams], idx: usize) -> StageParams {
    stage_params.get(idx).copied().unwrap_or_default()
}

/// Runs the PDF stream filter chain within `budget`.
///
/// - Never aborts early on `PdfFilter::Unknown` (matching `hayro-syntax`, which
///   drops unrecognized filter names and continues executing subsequent filters,
///   B-469).
/// - Validates per-stage `/DecodeParms`/`/DP` (including LZW `EarlyChange`,
///   Flate/LZW predictor row-buffer sizes, and CCITT `Columns`/`Rows`) and
///   embedded codec headers for `DCTDecode`, `JPXDecode`, and `JBIG2Decode`.
/// - When `fail_closed` is `true` (all Layer 2 streams/inline images, plus
///   Layer 1 `/XRef` and unencrypted `/ObjStm`), any undecodable or malformed
///   stage returns `Err(ToolError::BillUnreadable)` rather than `Ok(None)`.
fn check_filter_chain(
    raw: &[u8],
    filters: &[PdfFilter],
    stage_params: &[StageParams],
    img_ctx: StreamImageContext,
    capture_final: bool,
    fail_closed: bool,
    budget: &mut Budget,
) -> Result<DecodedStream, ToolError> {
    let mut owned: Option<Vec<u8>> = None;
    let mut last_len = raw.len();

    for (idx, &filter) in filters.iter().enumerate() {
        let params = stage_params_for(stage_params, idx);
        let has_later_known = filters[idx + 1..].iter().any(|&f| f != PdfFilter::Unknown);
        let need_bytes = capture_final || has_later_known;
        let limit = budget.remaining_decompressed();
        let input = owned.as_deref().unwrap_or(raw);

        match filter {
            PdfFilter::Unknown => {
                // `hayro-syntax` drops unrecognized filter names and runs all
                // subsequent recognized filters on the same byte stream (B-469).
                continue;
            }
            PdfFilter::Crypt => {
                if idx == 0 {
                    // Leading `/Crypt` is already handled by `raw_data()`.
                    continue;
                }
                if fail_closed {
                    return Err(ToolError::BillUnreadable);
                }
                return Ok(None);
            }
            PdfFilter::Flate => {
                check_predictor_params(params)?;
                let Some((produced, out)) = bounded_flate_decode(input, limit, need_bytes)? else {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                };
                budget.charge_decompressed(produced)?;
                last_len = produced;
                owned = out;
            }
            PdfFilter::Lzw => {
                check_predictor_params(params)?;
                let Some((produced, out)) =
                    bounded_lzw_decode(input, params.early_change, limit, need_bytes)?
                else {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                };
                budget.charge_decompressed(produced)?;
                last_len = produced;
                owned = out;
            }
            PdfFilter::RunLength => {
                let Some((produced, out)) = bounded_rle_decode(input, limit, need_bytes)? else {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                };
                budget.charge_decompressed(produced)?;
                last_len = produced;
                owned = out;
            }
            PdfFilter::AsciiHex => {
                let Some((produced, out)) = bounded_ascii_hex_decode(input, limit, need_bytes)?
                else {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                };
                last_len = produced;
                owned = out;
            }
            PdfFilter::Ascii85 => {
                let Some((produced, out)) = bounded_ascii85_decode(input, limit, need_bytes)?
                else {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                };
                budget.charge_decompressed(produced)?;
                last_len = produced;
                owned = out;
            }
            PdfFilter::Ccitt => {
                check_ccitt_stage_dims(params, img_ctx, budget)?;
                if has_later_known {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                }
                return Ok(None);
            }
            PdfFilter::Dct => {
                match parse_jpeg_dimensions(input) {
                    Ok((w, h)) => {
                        let _ = budget.check_extra_image_dims(w, h, img_ctx.pixels_charged)?;
                    }
                    Err(err @ ToolError::BillTooBig) => return Err(err),
                    Err(err) => {
                        if fail_closed {
                            return Err(err);
                        }
                        return Ok(None);
                    }
                }
                if has_later_known {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                }
                return Ok(None);
            }
            PdfFilter::Jpx => {
                match parse_jpx_dimensions(input) {
                    Ok((w, h)) => {
                        let _ = budget.check_extra_image_dims(w, h, img_ctx.pixels_charged)?;
                    }
                    Err(err @ ToolError::BillTooBig) => return Err(err),
                    Err(err) => {
                        if fail_closed {
                            return Err(err);
                        }
                        return Ok(None);
                    }
                }
                if has_later_known {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                }
                return Ok(None);
            }
            PdfFilter::Jbig2 => {
                match parse_jbig2_dimensions(input) {
                    Ok((w, h)) => {
                        let _ = budget.check_extra_image_dims(w, h, img_ctx.pixels_charged)?;
                    }
                    Err(err @ ToolError::BillTooBig) => return Err(err),
                    Err(err) => {
                        if fail_closed {
                            return Err(err);
                        }
                        return Ok(None);
                    }
                }
                if has_later_known {
                    if fail_closed {
                        return Err(ToolError::BillUnreadable);
                    }
                    return Ok(None);
                }
                return Ok(None);
            }
        }
    }

    Ok(Some((last_len, if capture_final { owned } else { None })))
}

// --- Pure-Rust bounded RFC 1950 (zlib) + RFC 1951 (DEFLATE) decoder ---------

struct LsbBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    bits: u8,
}

impl<'a> LsbBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            buf: 0,
            bits: 0,
        }
    }

    fn fill(&mut self, needed: u8) -> bool {
        while self.bits < needed {
            if self.pos >= self.data.len() {
                return false;
            }
            self.buf |= u32::from(self.data[self.pos]) << self.bits;
            self.pos += 1;
            self.bits += 8;
        }
        true
    }

    fn read_bits(&mut self, n: u8) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        if !self.fill(n) {
            return None;
        }
        let mask = (1u32 << n) - 1;
        let val = self.buf & mask;
        self.buf >>= n;
        self.bits -= n;
        Some(val)
    }

    fn align_to_byte(&mut self) {
        let discard = self.bits & 7;
        self.buf >>= discard;
        self.bits -= discard;
    }

    fn read_byte_aligned(&mut self) -> Option<u8> {
        if self.bits >= 8 {
            let b = (self.buf & 0xff) as u8;
            self.buf >>= 8;
            self.bits -= 8;
            return Some(b);
        }
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn read_u16_le_aligned(&mut self) -> Option<u16> {
        let lo = u16::from(self.read_byte_aligned()?);
        let hi = u16::from(self.read_byte_aligned()?);
        Some(lo | (hi << 8))
    }
}

struct HuffmanTable {
    counts: [u16; 16],
    first_code: [u32; 16],
    first_sym: [usize; 16],
    symbols: Vec<u16>,
    max_bits: u8,
}

impl HuffmanTable {
    fn from_lengths(lengths: &[u8]) -> Option<Self> {
        let mut counts = [0u16; 16];
        let mut max_bits = 0u8;
        for &len in lengths {
            if len > 15 {
                return None;
            }
            if len > 0 {
                counts[usize::from(len)] = counts[usize::from(len)].checked_add(1)?;
                max_bits = max_bits.max(len);
            }
        }
        if max_bits == 0 {
            return None;
        }
        let mut first_code = [0u32; 16];
        let mut first_sym = [0usize; 16];
        let mut code = 0u32;
        let mut total = 0usize;
        for bits in 1..=15usize {
            code <<= 1;
            first_code[bits] = code;
            first_sym[bits] = total;
            let c = u32::from(counts[bits]);
            code = code.checked_add(c)?;
            if code > (1u32 << bits) {
                return None;
            }
            total += usize::from(counts[bits]);
        }
        let mut offsets = first_sym;
        let mut symbols = vec![0u16; total];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                let slot = &mut offsets[usize::from(len)];
                symbols[*slot] = u16::try_from(sym).ok()?;
                *slot += 1;
            }
        }
        Some(Self {
            counts,
            first_code,
            first_sym,
            symbols,
            max_bits,
        })
    }

    fn decode(&self, reader: &mut LsbBitReader<'_>) -> Option<u16> {
        let mut code = 0u32;
        for bits in 1..=usize::from(self.max_bits) {
            let bit = reader.read_bits(1)?;
            code = (code << 1) | bit;
            let count = u32::from(self.counts[bits]);
            let first = self.first_code[bits];
            if count > 0 && code >= first && code - first < count {
                let idx = self.first_sym[bits] + (code - first) as usize;
                return self.symbols.get(idx).copied();
            }
        }
        None
    }
}

const LEN_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn fixed_huffman_tables() -> Option<(HuffmanTable, HuffmanTable)> {
    let mut lit_lengths = [0u8; 288];
    lit_lengths[0..=143].fill(8);
    lit_lengths[144..=255].fill(9);
    lit_lengths[256..=279].fill(7);
    lit_lengths[280..=287].fill(8);
    let dist_lengths = [5u8; 32];
    Some((
        HuffmanTable::from_lengths(&lit_lengths)?,
        HuffmanTable::from_lengths(&dist_lengths)?,
    ))
}

fn read_dynamic_huffman_tables(
    reader: &mut LsbBitReader<'_>,
) -> Option<(HuffmanTable, HuffmanTable)> {
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return None;
    }
    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        cl_lengths[CL_ORDER[i]] = reader.read_bits(3)? as u8;
    }
    let cl_table = HuffmanTable::from_lengths(&cl_lengths)?;
    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let sym = cl_table.decode(reader)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let prev = *lengths.last()?;
                let repeat = reader.read_bits(2)? as usize + 3;
                if lengths.len() + repeat > total {
                    return None;
                }
                lengths.resize(lengths.len() + repeat, prev);
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                if lengths.len() + repeat > total {
                    return None;
                }
                lengths.resize(lengths.len() + repeat, 0);
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                if lengths.len() + repeat > total {
                    return None;
                }
                lengths.resize(lengths.len() + repeat, 0);
            }
            _ => return None,
        }
    }
    let lit_table = HuffmanTable::from_lengths(&lengths[..hlit])?;
    // Per RFC 1951, a block with no back-references may omit distance codes
    // (all zero lengths); build a dummy table when all distance lengths are 0.
    let dist_slice = &lengths[hlit..];
    let dist_table = if dist_slice.iter().all(|&l| l == 0) {
        HuffmanTable::from_lengths(&[1])?
    } else {
        HuffmanTable::from_lengths(dist_slice)?
    };
    Some((lit_table, dist_table))
}

/// Result of a bounded stream decode: `Some((produced_bytes, optional_captured_bytes))`
/// when decoding completed within bounds, or `None` when the stream is malformed.
type DecodedStream = Option<(usize, Option<Vec<u8>>)>;

/// Boundedly inflates a zlib (RFC 1950) or raw deflate (RFC 1951) stream.
///
/// In counting mode (`capture == false`), uses O(1) memory and aborts with
/// `Err(ToolError::BillTooBig)` the instant output exceeds `max_out`.
fn bounded_flate_decode(
    data: &[u8],
    max_out: usize,
    capture: bool,
) -> Result<DecodedStream, ToolError> {
    if data.is_empty() {
        return Ok(Some((0, capture.then(Vec::new))));
    }
    // Strip optional zlib header (RFC 1950), falling back to raw deflate if
    // the zlib attempt fails to parse (matching `hayro-syntax`).
    let mut tried_offset_2 = false;
    let mut decoded = None;
    if data.len() >= 2 {
        let cmf = data[0];
        let flg = data[1];
        let header_ok = (cmf & 0x0f) == 8 && (((u16::from(cmf) << 8) | u16::from(flg)) % 31 == 0);
        if header_ok {
            let offset = if (flg & 0x20) != 0 { 6 } else { 2 };
            if offset == 2 {
                tried_offset_2 = true;
            }
            if offset <= data.len() {
                decoded = bounded_deflate_raw(&data[offset..], max_out, capture)?;
            }
        }
    }
    if decoded.is_none() {
        decoded = bounded_deflate_raw(data, max_out, capture)?;
    }
    // Match `lopdf`'s `decompress_zlib` fallback, which skips 2 bytes and
    // attempts raw deflate when standard zlib header validation fails (B-469).
    // Check `&data[2..]` even when offset 0 (e.g. `[0x03, 0x00, <bomb>]`) or
    // offset 6 (`FDICT`) succeeded, and reject multi-stage / ObjStm streams
    // where `hayro` and `lopdf` would decode diverging payloads.
    if !tried_offset_2 && data.len() > 2 {
        let offset2 = bounded_deflate_raw(&data[2..], max_out, false)?;
        if capture && matches!(offset2, Some((n, _)) if n > 0) {
            return Err(ToolError::BillUnreadable);
        }
    }
    Ok(decoded)
}

fn bounded_deflate_raw(
    deflate_slice: &[u8],
    max_out: usize,
    capture: bool,
) -> Result<DecodedStream, ToolError> {
    let mut reader = LsbBitReader::new(deflate_slice);
    let mut out_len = 0usize;
    let mut out = capture.then(Vec::new);

    loop {
        let Some(bfinal) = reader.read_bits(1) else {
            return Ok(None);
        };
        let Some(btype) = reader.read_bits(2) else {
            return Ok(None);
        };

        match btype {
            0 => {
                reader.align_to_byte();
                let Some(len) = reader.read_u16_le_aligned() else {
                    return Ok(None);
                };
                let Some(nlen) = reader.read_u16_le_aligned() else {
                    return Ok(None);
                };
                if len != !nlen {
                    return Ok(None);
                }
                let block_len = usize::from(len);
                out_len = out_len.saturating_add(block_len);
                if out_len > max_out {
                    return Err(ToolError::BillTooBig);
                }
                for _ in 0..block_len {
                    let Some(b) = reader.read_byte_aligned() else {
                        return Ok(None);
                    };
                    if let Some(buf) = out.as_mut() {
                        buf.push(b);
                    }
                }
            }
            1 | 2 => {
                let tables = if btype == 1 {
                    fixed_huffman_tables()
                } else {
                    read_dynamic_huffman_tables(&mut reader)
                };
                let Some((lit_table, dist_table)) = tables else {
                    return Ok(None);
                };
                loop {
                    let Some(sym) = lit_table.decode(&mut reader) else {
                        return Ok(None);
                    };
                    if sym < 256 {
                        out_len = out_len.saturating_add(1);
                        if out_len > max_out {
                            return Err(ToolError::BillTooBig);
                        }
                        if let Some(buf) = out.as_mut() {
                            buf.push(sym as u8);
                        }
                    } else if sym == 256 {
                        break;
                    } else if sym <= 285 {
                        let len_idx = usize::from(sym - 257);
                        let Some(extra_len) = reader.read_bits(LEN_EXTRA[len_idx]) else {
                            return Ok(None);
                        };
                        let match_len = LEN_BASE[len_idx] + extra_len as usize;
                        let Some(dist_sym) = dist_table.decode(&mut reader) else {
                            return Ok(None);
                        };
                        let dist_idx = usize::from(dist_sym);
                        if dist_idx >= DIST_BASE.len() {
                            return Ok(None);
                        }
                        let Some(extra_dist) = reader.read_bits(DIST_EXTRA[dist_idx]) else {
                            return Ok(None);
                        };
                        let distance = DIST_BASE[dist_idx] + extra_dist as usize;
                        if distance == 0 || distance > out_len {
                            return Ok(None);
                        }
                        out_len = out_len.saturating_add(match_len);
                        if out_len > max_out {
                            return Err(ToolError::BillTooBig);
                        }
                        if let Some(buf) = out.as_mut() {
                            for _ in 0..match_len {
                                let b = buf[buf.len() - distance];
                                buf.push(b);
                            }
                        }
                    } else {
                        return Ok(None);
                    }
                }
            }
            _ => return Ok(None),
        }

        if bfinal == 1 {
            break;
        }
    }

    Ok(Some((out_len, out)))
}

// --- Pure-Rust bounded PDF LZW decoder (MSB-first, 9..12-bit codes) ----------

struct MsbBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    bits: u8,
}

impl<'a> MsbBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            buf: 0,
            bits: 0,
        }
    }

    fn read_bits(&mut self, n: u8) -> Option<u16> {
        while self.bits < n {
            let b = *self.data.get(self.pos)?;
            self.pos += 1;
            self.buf = (self.buf << 8) | u32::from(b);
            self.bits += 8;
        }
        let shift = self.bits - n;
        let mask = (1u32 << n) - 1;
        let val = ((self.buf >> shift) & mask) as u16;
        self.bits -= n;
        Some(val)
    }
}

/// Boundedly decodes a PDF `LZWDecode` stream, aborting immediately with
/// `Err(ToolError::BillTooBig)` if decompressed output exceeds `max_out`.
fn bounded_lzw_decode(
    data: &[u8],
    early_change: bool,
    max_out: usize,
    capture: bool,
) -> Result<DecodedStream, ToolError> {
    const CLEAR: u16 = 256;
    const EOD: u16 = 257;
    const MAX_TABLE: usize = 4096;

    let mut reader = MsbBitReader::new(data);
    let mut code_size = 9u8;
    let mut next_code = 258usize;
    let mut entry_len = [0usize; MAX_TABLE];
    let mut entry_first = [0u8; MAX_TABLE];
    let mut entry_parent = [0u16; MAX_TABLE];
    let mut entry_last = [0u8; MAX_TABLE];
    for i in 0..256usize {
        entry_len[i] = 1;
        entry_first[i] = i as u8;
        entry_last[i] = i as u8;
    }

    let mut prev_code: Option<usize> = None;
    let mut out_len = 0usize;
    let mut out = capture.then(Vec::new);
    let mut scratch = Vec::new();
    let early = usize::from(early_change);

    while let Some(code) = reader.read_bits(code_size) {
        if code == EOD {
            break;
        }
        if code == CLEAR {
            code_size = 9;
            next_code = 258;
            prev_code = None;
            continue;
        }
        let code_idx = usize::from(code);
        let (seq_len, first_byte) = if code_idx < next_code {
            (entry_len[code_idx], entry_first[code_idx])
        } else if code_idx == next_code
            && let Some(prev) = prev_code
        {
            (entry_len[prev].saturating_add(1), entry_first[prev])
        } else {
            return Ok(None);
        };

        out_len = out_len.saturating_add(seq_len);
        if out_len > max_out {
            return Err(ToolError::BillTooBig);
        }

        if let Some(prev) = prev_code
            && next_code < MAX_TABLE
        {
            entry_len[next_code] = entry_len[prev].saturating_add(1);
            entry_first[next_code] = entry_first[prev];
            entry_parent[next_code] = prev as u16;
            entry_last[next_code] = first_byte;
            next_code += 1;
            let threshold = next_code + early;
            if threshold == 512 && code_size == 9 {
                code_size = 10;
            } else if threshold == 1024 && code_size == 10 {
                code_size = 11;
            } else if threshold == 2048 && code_size == 11 {
                code_size = 12;
            }
        }

        if let Some(buf) = out.as_mut() {
            scratch.clear();
            let mut cur = code_idx;
            while cur >= 256 {
                scratch.push(entry_last[cur]);
                cur = usize::from(entry_parent[cur]);
            }
            scratch.push(cur as u8);
            buf.extend(scratch.iter().rev().copied());
        }

        prev_code = Some(code_idx);
    }

    Ok(Some((out_len, out)))
}

// --- Pure-Rust bounded RunLength, ASCIIHex, and ASCII85 decoders ------------

fn bounded_rle_decode(
    data: &[u8],
    max_out: usize,
    capture: bool,
) -> Result<DecodedStream, ToolError> {
    let mut i = 0usize;
    let mut out_len = 0usize;
    let mut out = capture.then(Vec::new);
    while i < data.len() {
        let len_byte = data[i];
        i += 1;
        if len_byte == 128 {
            break;
        }
        if len_byte < 128 {
            let count = usize::from(len_byte) + 1;
            if i + count > data.len() {
                return Ok(None);
            }
            out_len = out_len.saturating_add(count);
            if out_len > max_out {
                return Err(ToolError::BillTooBig);
            }
            if let Some(buf) = out.as_mut() {
                buf.extend_from_slice(&data[i..i + count]);
            }
            i += count;
        } else {
            let count = 257 - usize::from(len_byte);
            let Some(&byte) = data.get(i) else {
                return Ok(None);
            };
            i += 1;
            out_len = out_len.saturating_add(count);
            if out_len > max_out {
                return Err(ToolError::BillTooBig);
            }
            if let Some(buf) = out.as_mut() {
                buf.resize(buf.len() + count, byte);
            }
        }
    }
    Ok(Some((out_len, out)))
}

fn bounded_ascii_hex_decode(
    data: &[u8],
    max_out: usize,
    capture: bool,
) -> Result<DecodedStream, ToolError> {
    let mut out_len = 0usize;
    let mut out = capture.then(Vec::new);
    let mut high: Option<u8> = None;
    for &b in data {
        if b == b'>' {
            break;
        }
        if is_pdf_ws(b) {
            continue;
        }
        let Some(nibble) = hex_nibble(b) else {
            return Ok(None);
        };
        if let Some(h) = high.take() {
            if out_len >= max_out {
                return Err(ToolError::BillTooBig);
            }
            out_len += 1;
            if let Some(buf) = out.as_mut() {
                buf.push((h << 4) | nibble);
            }
        } else {
            high = Some(nibble);
        }
    }
    if let Some(h) = high {
        if out_len >= max_out {
            return Err(ToolError::BillTooBig);
        }
        out_len += 1;
        if let Some(buf) = out.as_mut() {
            buf.push(h << 4);
        }
    }
    Ok(Some((out_len, out)))
}

fn bounded_ascii85_decode(
    data: &[u8],
    max_out: usize,
    capture: bool,
) -> Result<DecodedStream, ToolError> {
    let mut out_len = 0usize;
    let mut out = capture.then(Vec::new);
    let mut group = [0u32; 5];
    let mut count = 0usize;
    let mut i = 0usize;
    while i < data.len() {
        let b = data[i];
        i += 1;
        if is_pdf_ws(b) {
            continue;
        }
        if b == b'~' {
            break;
        }
        if b == b'z' && count == 0 {
            out_len = out_len.saturating_add(4);
            if out_len > max_out {
                return Err(ToolError::BillTooBig);
            }
            if let Some(buf) = out.as_mut() {
                buf.extend_from_slice(&[0, 0, 0, 0]);
            }
            continue;
        }
        if !(b'!'..=b'u').contains(&b) {
            return Ok(None);
        }
        group[count] = u32::from(b - b'!');
        count += 1;
        if count == 5 {
            let mut acc = 0u64;
            for &d in &group {
                acc = acc * 85 + u64::from(d);
            }
            let Ok(word) = u32::try_from(acc) else {
                return Ok(None);
            };
            out_len = out_len.saturating_add(4);
            if out_len > max_out {
                return Err(ToolError::BillTooBig);
            }
            if let Some(buf) = out.as_mut() {
                buf.extend_from_slice(&word.to_be_bytes());
            }
            count = 0;
        }
    }
    if count == 1 {
        return Ok(None);
    }
    if count >= 2 {
        for slot in &mut group[count..] {
            *slot = 84;
        }
        let mut acc = 0u64;
        for &d in &group {
            acc = acc * 85 + u64::from(d);
        }
        let Ok(word) = u32::try_from(acc) else {
            return Ok(None);
        };
        let bytes = word.to_be_bytes();
        let emit = count - 1;
        out_len = out_len.saturating_add(emit);
        if out_len > max_out {
            return Err(ToolError::BillTooBig);
        }
        if let Some(buf) = out.as_mut() {
            buf.extend_from_slice(&bytes[..emit]);
        }
    }
    Ok(Some((out_len, out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes `count` zero bytes as a valid RFC 1950 zlib stream using fixed
    /// Huffman literal `0` + max-length (258) back-references (tiny output for
    /// huge expansions).
    fn zlib_zero_bomb(count: usize) -> Vec<u8> {
        let mut bits: Vec<u8> = Vec::new();
        let mut cur = 0u8;
        let mut used = 0u8;
        let mut push_lsb = |val: u32, n: u8| {
            for i in 0..n {
                if ((val >> i) & 1) != 0 {
                    cur |= 1 << used;
                }
                used += 1;
                if used == 8 {
                    bits.push(cur);
                    cur = 0;
                    used = 0;
                }
            }
        };
        // BFINAL = 1, BTYPE = 01 (fixed Huffman)
        push_lsb(1, 1);
        push_lsb(1, 2);
        let mut push_msb = |val: u32, n: u8| {
            for i in (0..n).rev() {
                push_lsb((val >> i) & 1, 1);
            }
        };
        // Literal 0x00 in fixed Huffman: 8-bit code 0b0011_0000 (0x30)
        push_msb(0x30, 8);
        let mut remaining = count.saturating_sub(1);
        // Symbol 285 (length 258, 0 extra bits): 8-bit code 0b1100_0101 (0xC5),
        // distance symbol 0 (distance 1, 0 extra bits): 5-bit code 0b00000.
        while remaining >= 258 {
            push_msb(0xC5, 8);
            push_msb(0, 5);
            remaining -= 258;
        }
        for _ in 0..remaining {
            push_msb(0x30, 8);
        }
        // End-of-block symbol 256: 7-bit code 0b0000000
        push_msb(0, 7);
        if used > 0 {
            bits.push(cur);
        }
        // Wrap in RFC 1950 zlib header (0x78 0x9C) + dummy Adler-32 trailer.
        let mut out = Vec::with_capacity(bits.len() + 6);
        out.extend_from_slice(&[0x78, 0x9C]);
        out.extend_from_slice(&bits);
        out.extend_from_slice(&[0, 0, 0, 1]);
        out
    }

    #[test]
    fn bounded_flate_counts_normal_and_rejects_bomb_without_allocating() {
        let small = zlib_zero_bomb(4_096);
        let (len, captured) = bounded_flate_decode(&small, 10_000, true)
            .expect("within limit")
            .expect("valid zlib");
        assert_eq!(len, 4_096);
        assert_eq!(captured.as_deref(), Some(&vec![0u8; 4_096][..]));

        let bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);
        assert!(bomb.len() < 200_000, "compact bomb: {} bytes", bomb.len());
        let err =
            bounded_flate_decode(&bomb, MAX_PDF_STREAM_DECOMPRESSED_BYTES, false).unwrap_err();
        assert!(matches!(err, ToolError::BillTooBig));
    }

    #[test]
    fn bounded_lzw_rejects_expanding_bomb() {
        // Build a compact MSB-first PDF LZW stream of repeated zeros:
        // CLEAR(256), 0, 258, 259, ..., repeatedly resetting at 4090.
        let mut buf = Vec::new();
        let mut acc = 0u32;
        let mut bits = 0u8;
        let mut push = |code: u16, width: u8| {
            acc = (acc << width) | u32::from(code);
            bits += width;
            while bits >= 8 {
                bits -= 8;
                buf.push(((acc >> bits) & 0xff) as u8);
            }
        };
        let mut width = 9u8;
        for _ in 0..30 {
            push(256, width); // CLEAR (written at the current code width)
            width = 9;
            push(0, width);
            let mut next = 258usize;
            for step in 0..2000usize {
                let code = u16::try_from(258 + step).unwrap();
                push(code, width);
                next += 1;
                if next + 1 == 512 {
                    width = 10;
                } else if next + 1 == 1024 {
                    width = 11;
                } else if next + 1 == 2048 {
                    width = 12;
                }
            }
        }
        push(257, width); // EOD
        if bits > 0 {
            buf.push(((acc << (8 - bits)) & 0xff) as u8);
        }
        let err =
            bounded_lzw_decode(&buf, true, MAX_PDF_STREAM_DECOMPRESSED_BYTES, false).unwrap_err();
        assert!(matches!(err, ToolError::BillTooBig));
    }

    #[test]
    fn bounded_rle_rejects_expanding_bomb() {
        let mut rle = Vec::new();
        // Each [129, 0] repeats 0x00 128 times.
        for _ in 0..(MAX_PDF_STREAM_DECOMPRESSED_BYTES / 128 + 10) {
            rle.extend_from_slice(&[129, 0]);
        }
        rle.push(128); // EOD
        let err = bounded_rle_decode(&rle, MAX_PDF_STREAM_DECOMPRESSED_BYTES, false).unwrap_err();
        assert!(matches!(err, ToolError::BillTooBig));
    }

    #[test]
    fn hex_escaped_filter_names_are_decoded() {
        let (name, end) = parse_pdf_name(b"/Fl#61teDecode ", 0).expect("parsed");
        assert_eq!(name, b"FlateDecode");
        assert_eq!(end, 14);
    }

    fn assemble_test_pdf(objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.5\n%\xE2\xE3\xCF\xD3\n");
        let mut offsets = Vec::with_capacity(objects.len());
        for (idx, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            let id = idx + 1;
            out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_offset = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    fn lzw_zero_bomb(early_change: bool) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut acc = 0u32;
        let mut bits = 0u8;
        let mut push = |code: u16, width: u8| {
            acc = (acc << width) | u32::from(code);
            bits += width;
            while bits >= 8 {
                bits -= 8;
                buf.push(((acc >> bits) & 0xff) as u8);
            }
        };
        let early = usize::from(early_change);
        let mut width = 9u8;
        for _ in 0..30 {
            push(256, width);
            width = 9;
            push(0, width);
            let mut next = 258usize;
            for step in 0..2000usize {
                let code = u16::try_from(258 + step).unwrap();
                push(code, width);
                next += 1;
                if next + early == 512 {
                    width = 10;
                } else if next + early == 1024 {
                    width = 11;
                } else if next + early == 2048 {
                    width = 12;
                }
            }
        }
        push(257, width);
        if bits > 0 {
            buf.push(((acc << (8 - bits)) & 0xff) as u8);
        }
        buf
    }

    fn synthetic_j2k_siz(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0xFF, 0x4F, 0xFF, 0x51]); // SOC + SIZ
        out.extend_from_slice(&41u16.to_be_bytes()); // Lsiz (38 + 3*1)
        out.extend_from_slice(&0u16.to_be_bytes()); // Rsiz
        out.extend_from_slice(&width.to_be_bytes()); // Xsiz
        out.extend_from_slice(&height.to_be_bytes()); // Ysiz
        out.extend_from_slice(&0u32.to_be_bytes()); // XOsiz
        out.extend_from_slice(&0u32.to_be_bytes()); // YOsiz
        out.extend_from_slice(&width.to_be_bytes()); // XTsiz
        out.extend_from_slice(&height.to_be_bytes()); // YTsiz
        out.extend_from_slice(&0u32.to_be_bytes()); // XTOsiz
        out.extend_from_slice(&0u32.to_be_bytes()); // YTOsiz
        out.extend_from_slice(&1u16.to_be_bytes()); // Csiz = 1
        out.extend_from_slice(&[7, 1, 1]); // 8-bit unsigned, 1x1 subsampling
        out
    }

    fn synthetic_jp2_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let len = u32::try_from(payload.len() + 8).unwrap();
        let mut out = Vec::with_capacity(len as usize);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(box_type);
        out.extend_from_slice(payload);
        out
    }

    fn synthetic_jbig2_page_info(width: u32, height: u32) -> Vec<u8> {
        let mut seg_data = Vec::with_capacity(19);
        seg_data.extend_from_slice(&width.to_be_bytes());
        seg_data.extend_from_slice(&height.to_be_bytes());
        seg_data.extend_from_slice(&0u32.to_be_bytes()); // X res
        seg_data.extend_from_slice(&0u32.to_be_bytes()); // Y res
        seg_data.push(0); // flags
        seg_data.extend_from_slice(&0u16.to_be_bytes()); // striping

        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_be_bytes()); // seg_num = 0
        out.push(48); // PageInformation, 1-byte page assoc
        out.push(0); // 0 referred segments
        out.push(1); // page 1
        out.extend_from_slice(&19u32.to_be_bytes()); // data length
        out.extend_from_slice(&seg_data);
        out
    }

    fn synthetic_jbig2_end_of_stripe(seg_num: u32, end_row: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&seg_num.to_be_bytes());
        out.push(50); // EndOfStripe
        out.push(0);
        out.push(1);
        out.extend_from_slice(&4u32.to_be_bytes());
        out.extend_from_slice(&end_row.to_be_bytes());
        out
    }

    fn synthetic_jbig2_region(seg_num: u32, seg_type: u8, width: u32, height: u32) -> Vec<u8> {
        let mut seg_data = Vec::with_capacity(17);
        seg_data.extend_from_slice(&width.to_be_bytes());
        seg_data.extend_from_slice(&height.to_be_bytes());
        seg_data.extend_from_slice(&0u32.to_be_bytes()); // X loc
        seg_data.extend_from_slice(&0u32.to_be_bytes()); // Y loc
        seg_data.push(0); // flags

        let mut out = Vec::new();
        out.extend_from_slice(&seg_num.to_be_bytes());
        out.push(seg_type);
        out.push(0);
        out.push(1);
        out.extend_from_slice(&17u32.to_be_bytes());
        out.extend_from_slice(&seg_data);
        out
    }

    fn synthetic_jpeg_sof0(width: u16, height: u16) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xC0];
        out.extend_from_slice(&11u16.to_be_bytes()); // Lf
        out.push(8); // precision
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.push(1); // Nf
        out.extend_from_slice(&[1, 0x11, 0]); // component 1
        out.extend_from_slice(&[0xFF, 0xD9]); // EOI
        out
    }

    #[test]
    fn b469_leading_unknown_filter_does_not_skip_subsequent_filters() {
        let bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);

        // `/Filter [/X /FlateDecode]` must NOT short-circuit on `/X`.
        let mut stream_obj = format!(
            "<< /Filter [/X /FlateDecode] /Length {} >>\nstream\n",
            bomb.len()
        )
        .into_bytes();
        stream_obj.extend_from_slice(&bomb);
        stream_obj.extend_from_slice(b"\nendstream");

        let pdf_bytes = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            stream_obj,
        ]);

        // Layer 1 catches it.
        assert!(matches!(
            validate_raw_pdf_safety(&pdf_bytes),
            Err(ToolError::BillTooBig)
        ));
        // Layer 2 also catches it directly.
        let parsed = Pdf::new(pdf_bytes).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed),
            Err(ToolError::BillTooBig)
        ));

        // `/Filter [/X /JBIG2Decode]` still validates the embedded JBIG2 header.
        let huge_jbig2 = synthetic_jbig2_page_info(60_000, 60_000);
        let mut budget = Budget::default();
        let err = check_filter_chain(
            &huge_jbig2,
            &[PdfFilter::Unknown, PdfFilter::Jbig2],
            &[],
            StreamImageContext::default(),
            false,
            true,
            &mut budget,
        )
        .unwrap_err();
        assert!(matches!(err, ToolError::BillTooBig));

        // Non-Name item inside `/Filter` array fails closed in Layer 2.
        let mut bad_filter_obj =
            b"<< /Filter [ 123 /FlateDecode ] /Length 0 >>\nstream\nendstream".to_vec();
        bad_filter_obj.truncate(bad_filter_obj.len());
        let bad_pdf_bytes = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            bad_filter_obj,
        ]);
        let parsed_bad = Pdf::new(bad_pdf_bytes).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_bad),
            Err(ToolError::BillUnreadable)
        ));
    }

    #[test]
    fn b469_inline_images_in_page_and_form_xobjects_are_validated() {
        let bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);

        // 1. Oversized inline image in page content stream.
        let mut page_inline_bomb =
            b"q 100 0 0 100 0 0 cm BI /W 60000 /H 60000 /CS /G /BPC 8 /F /Fl ID ".to_vec();
        page_inline_bomb.extend_from_slice(&bomb);
        page_inline_bomb.extend_from_slice(b"\nEI Q\n");
        let mut page_content =
            format!("<< /Length {} >>\nstream\n", page_inline_bomb.len()).into_bytes();
        page_content.extend_from_slice(&page_inline_bomb);
        page_content.extend_from_slice(b"\nendstream");

        let pdf_bytes = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            page_content,
        ]);
        let parsed = Pdf::new(pdf_bytes).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed),
            Err(ToolError::BillTooBig)
        ));

        // 2. Small-dimension inline image with Flate decompression bomb in page content stream.
        let mut page_flate_inline = b"q BI /W 2 /H 2 /CS /G /BPC 8 /F /Fl ID ".to_vec();
        page_flate_inline.extend_from_slice(&bomb);
        page_flate_inline.extend_from_slice(b"\nEI Q\n");
        let mut page_content_2 =
            format!("<< /Length {} >>\nstream\n", page_flate_inline.len()).into_bytes();
        page_content_2.extend_from_slice(&page_flate_inline);
        page_content_2.extend_from_slice(b"\nendstream");
        let pdf_bytes_2 = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            page_content_2,
        ]);
        let parsed_2 = Pdf::new(pdf_bytes_2).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_2),
            Err(ToolError::BillTooBig)
        ));

        // 3. Oversized inline image hidden inside a Form XObject (`/Subtype /Form`).
        let form_body = b"q BI /W 60000 /H 60000 /CS /G /BPC 8 ID \x00\x00\nEI Q\n";
        let mut form_obj = format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Length {} >>\nstream\n",
            form_body.len()
        )
        .into_bytes();
        form_obj.extend_from_slice(form_body);
        form_obj.extend_from_slice(b"\nendstream");

        let form_pdf_bytes = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /XObject << /Fm0 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
            b"<< /Length 10 >>\nstream\n/Fm0 Do\n\nendstream".to_vec(),
            form_obj,
        ]);
        let parsed_form = Pdf::new(form_pdf_bytes).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_form),
            Err(ToolError::BillTooBig)
        ));

        // 4. Valid small 2x2 grayscale inline image passes both Layer 1 and Layer 2.
        let valid_body =
            b"q 20 0 0 20 10 10 cm BI /W 2 /H 2 /CS /G /BPC 8 ID \x00\xFF\xFF\x00\nEI Q\n";
        let mut valid_content =
            format!("<< /Length {} >>\nstream\n", valid_body.len()).into_bytes();
        valid_content.extend_from_slice(valid_body);
        valid_content.extend_from_slice(b"\nendstream");
        let valid_pdf = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            valid_content,
        ]);
        validate_raw_pdf_safety(&valid_pdf).expect("Layer 1 accepts valid inline image");
        let parsed_valid = Pdf::new(valid_pdf).expect("parseable PDF");
        validate_parsed_pdf_safety(&parsed_valid).expect("Layer 2 accepts valid inline image");
    }

    #[test]
    fn b469_literal_string_unbalanced_dict_does_not_bypass_layer1() {
        let bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);

        // 1. `1 0 obj (<<) endobj` immediately before a compressed `/XRef` Flate bomb.
        let mut raw_xref_pdf = b"%PDF-1.5\n1 0 obj (<<) endobj\n2 0 obj << /Type /XRef /Size 3 /W [1 2 1] /Filter /FlateDecode /Length ".to_vec();
        raw_xref_pdf.extend_from_slice(bomb.len().to_string().as_bytes());
        raw_xref_pdf.extend_from_slice(b" >>\nstream\n");
        raw_xref_pdf.extend_from_slice(&bomb);
        raw_xref_pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&raw_xref_pdf),
            Err(ToolError::BillTooBig)
        ));

        // 2. `1 0 obj (<<) endobj` immediately before a compressed `/ObjStm` Flate bomb.
        let mut raw_objstm_pdf = b"%PDF-1.5\n1 0 obj (<<) endobj\n2 0 obj << /Type /ObjStm /N 1 /First 4 /Filter /FlateDecode /Length ".to_vec();
        raw_objstm_pdf.extend_from_slice(bomb.len().to_string().as_bytes());
        raw_objstm_pdf.extend_from_slice(b" >>\nstream\n");
        raw_objstm_pdf.extend_from_slice(&bomb);
        raw_objstm_pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&raw_objstm_pdf),
            Err(ToolError::BillTooBig)
        ));

        // 3. Unclosed `(` wrapping an `/XRef` bomb + closing `)` after it.
        let mut wrapped_paren_pdf =
            b"%PDF-1.5\n1 0 obj (\n2 0 obj << /Type /XRef /Filter /FlateDecode /Length ".to_vec();
        wrapped_paren_pdf.extend_from_slice(bomb.len().to_string().as_bytes());
        wrapped_paren_pdf.extend_from_slice(b" >>\nstream\n");
        wrapped_paren_pdf.extend_from_slice(&bomb);
        wrapped_paren_pdf.extend_from_slice(b"\nendstream\nendobj\n) endobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&wrapped_paren_pdf),
            Err(ToolError::BillTooBig)
        ));

        // 4. Unclosed `<<` before an `/XRef` bomb + closing `>>` after it.
        let mut unclosed_dict_pdf =
            b"%PDF-1.5\n1 0 obj <<\n2 0 obj << /Type /XRef /Filter /FlateDecode /Length ".to_vec();
        unclosed_dict_pdf.extend_from_slice(bomb.len().to_string().as_bytes());
        unclosed_dict_pdf.extend_from_slice(b" >>\nstream\n");
        unclosed_dict_pdf.extend_from_slice(&bomb);
        unclosed_dict_pdf.extend_from_slice(b"\nendstream\nendobj\n>> endobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&unclosed_dict_pdf),
            Err(ToolError::BillTooBig)
        ));

        // 5. Forged `10 0 obj 1 endobj` inside literal string and stream body
        //    must NOT poison `indirects[10]`.
        let forged = b"%PDF-1.5\n\
            10 0 obj 50000 endobj\n\
            1 0 obj (10 0 obj 1 endobj) endobj\n\
            2 0 obj << /Length 19 >>\nstream\n10 0 obj 2 endobj\n\nendstream\nendobj\n";
        let indirects = scan_indirect_objects(forged).expect("scan succeeds");
        assert!(matches!(
            indirects.get(&10),
            Some(SimpleIndirect::Int(50_000))
        ));
    }

    #[test]
    fn b469_codec_headers_array_decode_parms_and_fail_closed() {
        // 1. JBIG2 PageInfo oversized (60000x60000) and striped EndOfStripe oversized.
        assert!(matches!(
            parse_jbig2_dimensions(&synthetic_jbig2_page_info(60_000, 60_000)),
            Err(ToolError::BillTooBig)
        ));
        let mut striped = synthetic_jbig2_page_info(1_000, 0xFFFF_FFFF);
        striped.extend_from_slice(&synthetic_jbig2_end_of_stripe(1, 59_999));
        assert!(matches!(
            parse_jbig2_dimensions(&striped),
            Err(ToolError::BillTooBig)
        ));
        // Small PageInfo (10x10) with huge ImmediateGenericRegion (50000x50000).
        let mut huge_region = synthetic_jbig2_page_info(10, 10);
        huge_region.extend_from_slice(&synthetic_jbig2_region(1, 38, 50_000, 50_000));
        assert!(matches!(
            parse_jbig2_dimensions(&huge_region),
            Err(ToolError::BillTooBig)
        ));
        assert_eq!(
            parse_jbig2_dimensions(&synthetic_jbig2_page_info(16, 24)).unwrap(),
            (16, 24)
        );

        // 2. JPX raw J2K SIZ oversized & JP2 container with tiny ihdr + huge jp2c SIZ.
        assert_eq!(
            parse_jpx_dimensions(&synthetic_j2k_siz(16, 24)).unwrap(),
            (16, 24)
        );
        let mut budget = Budget::default();
        assert!(matches!(
            check_filter_chain(
                &synthetic_j2k_siz(60_000, 60_000),
                &[PdfFilter::Jpx],
                &[],
                StreamImageContext {
                    width: Some(1),
                    height: Some(1),
                    pixels_charged: 1
                },
                false,
                true,
                &mut budget,
            ),
            Err(ToolError::BillTooBig)
        ));

        let mut jp2_bomb = Vec::new();
        jp2_bomb.extend_from_slice(b"\x00\x00\x00\x0C\x6A\x50\x20\x20\x0D\x0A\x87\x0A");
        jp2_bomb.extend_from_slice(&synthetic_jp2_box(b"ftyp", b"jp2 \x00\x00\x00\x00jp2 "));
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u16.to_be_bytes());
        ihdr.extend_from_slice(&[7, 7, 0, 0]);
        let jp2h = synthetic_jp2_box(b"jp2h", &synthetic_jp2_box(b"ihdr", &ihdr));
        jp2_bomb.extend_from_slice(&jp2h);
        jp2_bomb.extend_from_slice(&synthetic_jp2_box(
            b"jp2c",
            &synthetic_j2k_siz(60_000, 60_000),
        ));
        assert!(matches!(
            parse_jpx_dimensions(&jp2_bomb),
            Err(ToolError::BillTooBig)
        ));

        // 3. DCT (JPEG) SOF0 oversized despite `/Width 1 /Height 1`.
        let huge_jpg = synthetic_jpeg_sof0(30_000, 30_000);
        let mut budget = Budget::default();
        assert!(matches!(
            check_filter_chain(
                &huge_jpg,
                &[PdfFilter::Dct],
                &[],
                StreamImageContext {
                    width: Some(1),
                    height: Some(1),
                    pixels_charged: 1
                },
                false,
                true,
                &mut budget,
            ),
            Err(ToolError::BillTooBig)
        ));
        assert_eq!(
            parse_jpeg_dimensions(&synthetic_jpeg_sof0(64, 48)).unwrap(),
            (64, 48)
        );

        // 4. CCITT with omitted `/Rows` + array `/DP [ << /Columns 10000 >> ]`
        //    combined with `/Width 1 /Height 10000` (100 MP > 32 MP).
        let ccitt_obj = b"<< /Type /XObject /Subtype /Image /Width 1 /Height 10000 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter [/CCITTFaxDecode] /DP [ << /Columns 10000 >> ] /Length 2 >>\nstream\n\x00\x00\nendstream".to_vec();
        let ccitt_pdf = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /XObject << /Im0 4 0 R >> >> >>".to_vec(),
            ccitt_obj,
        ]);
        assert!(matches!(
            validate_raw_pdf_safety(&ccitt_pdf),
            Err(ToolError::BillTooBig)
        ));
        let parsed_ccitt = Pdf::new(ccitt_pdf).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_ccitt),
            Err(ToolError::BillTooBig)
        ));

        // 5. LZW bomb with `/EarlyChange 0` inside array `/DP [ << /EarlyChange 0 >> ]`.
        let lzw_bomb = lzw_zero_bomb(false);
        let mut lzw_obj = format!(
            "<< /Filter [/LZWDecode] /DP [ << /EarlyChange 0 >> ] /Length {} >>\nstream\n",
            lzw_bomb.len()
        )
        .into_bytes();
        lzw_obj.extend_from_slice(&lzw_bomb);
        lzw_obj.extend_from_slice(b"\nendstream");
        let lzw_pdf = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            lzw_obj,
        ]);
        assert!(matches!(
            validate_raw_pdf_safety(&lzw_pdf),
            Err(ToolError::BillTooBig)
        ));
        let parsed_lzw = Pdf::new(lzw_pdf).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_lzw),
            Err(ToolError::BillTooBig)
        ));

        // 6. Fail-closed on corrupt Flate stream in Layer 2.
        let corrupt_flate_pdf = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            b"<< /Filter /FlateDecode /Length 6 >>\nstream\n\x78\x9C\xFF\xFF\xFF\xFF\nendstream"
                .to_vec(),
        ]);
        let parsed_corrupt = Pdf::new(corrupt_flate_pdf).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_corrupt),
            Err(ToolError::BillUnreadable)
        ));
    }

    #[test]
    fn b469_inline_image_scan_cannot_be_bypassed_by_extra_dict_tags() {
        let bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);
        let mut inline_payload = b"q BI /W 60000 /H 60000 /CS /G /BPC 8 /F /Fl ID ".to_vec();
        inline_payload.extend_from_slice(&bomb);
        inline_payload.extend_from_slice(b"\nEI Q\n");

        // 1. Form XObject tagged with `/Type /Metadata`.
        let mut form_meta = format!(
            "<< /Type /Metadata /Subtype /Form /BBox [0 0 100 100] /Length {} >>\nstream\n",
            inline_payload.len()
        )
        .into_bytes();
        form_meta.extend_from_slice(&inline_payload);
        form_meta.extend_from_slice(b"\nendstream");

        let pdf_meta = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /XObject << /Fm0 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
            b"<< /Length 8 >>\nstream\n/Fm0 Do\n\nendstream".to_vec(),
            form_meta,
        ]);
        let parsed_meta = Pdf::new(pdf_meta).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_meta),
            Err(ToolError::BillTooBig)
        ));

        // 2. Form XObject tagged with `/IM true`.
        let mut form_im = format!(
            "<< /Subtype /Form /IM true /BBox [0 0 100 100] /Length {} >>\nstream\n",
            inline_payload.len()
        )
        .into_bytes();
        form_im.extend_from_slice(&inline_payload);
        form_im.extend_from_slice(b"\nendstream");

        let pdf_im = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /XObject << /Fm0 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
            b"<< /Length 8 >>\nstream\n/Fm0 Do\n\nendstream".to_vec(),
            form_im,
        ]);
        let parsed_im = Pdf::new(pdf_im).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_im),
            Err(ToolError::BillTooBig)
        ));

        // 3. Appearance/CharProc stream disguised with `/Subtype /Image` and `/Type /Metadata`.
        let mut disguised = format!(
            "<< /Type /Metadata /Subtype /Image /Width 1 /Height 1 /Length {} >>\nstream\n",
            inline_payload.len()
        )
        .into_bytes();
        disguised.extend_from_slice(&inline_payload);
        disguised.extend_from_slice(b"\nendstream");

        let pdf_disguised = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            b"<< /Length 0 >>\nstream\n\nendstream".to_vec(),
            disguised,
        ]);
        let parsed_disguised = Pdf::new(pdf_disguised).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_disguised),
            Err(ToolError::BillTooBig)
        ));
    }

    #[test]
    fn b469_layer1_bypasses_comment_string_outer_stream_and_filter_dp_shadowing() {
        let flate_bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);
        let lzw_bomb = lzw_zero_bomb(false);

        // Vector 2a: balanced literal string closed via `% )` before `stream\n`
        // hiding an `/XRef` Flate bomb.
        let mut v2a = b"%PDF-1.5\n1 0 obj (\n".to_vec();
        let xref_offset_2a = v2a.len();
        v2a.extend_from_slice(
            format!(
                "2 0 obj << /Type /XRef /Size 3 /W [1 2 1] /Filter /FlateDecode /Length {} >>\n% )\nstream\n",
                flate_bomb.len()
            )
            .as_bytes(),
        );
        v2a.extend_from_slice(&flate_bomb);
        v2a.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{xref_offset_2a}\n%%EOF\n").as_bytes(),
        );
        assert!(matches!(
            validate_raw_pdf_safety(&v2a),
            Err(ToolError::BillTooBig)
        ));

        // Vector 2a (dict-wrapped): balanced string inside an outer dict hiding `/XRef`.
        let mut v2a_dict = b"%PDF-1.5\n1 0 obj << /Note (\n".to_vec();
        let xref_offset_2a_dict = v2a_dict.len();
        v2a_dict.extend_from_slice(
            format!(
                "2 0 obj << /Type /XRef /Size 3 /W [1 2 1] /Filter /FlateDecode /Length {} >>\n% ) >>\nstream\n",
                flate_bomb.len()
            )
            .as_bytes(),
        );
        v2a_dict.extend_from_slice(&flate_bomb);
        v2a_dict.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{xref_offset_2a_dict}\n%%EOF\n").as_bytes(),
        );
        assert!(matches!(
            validate_raw_pdf_safety(&v2a_dict),
            Err(ToolError::BillTooBig)
        ));

        // Vector 2b: uncompressed outer dummy stream swallowing an inner `/XRef` bomb.
        let mut inner_xref = format!(
            "1 0 obj << /Type /XRef /Size 3 /W [1 2 1] /Filter /FlateDecode /Length {} >>\nstream\n",
            flate_bomb.len()
        )
        .into_bytes();
        inner_xref.extend_from_slice(&flate_bomb);
        inner_xref.extend_from_slice(b"\nendstream\nendobj\n");

        let mut v2b = format!(
            "%PDF-1.5\n99 0 obj << /Length {} >>\nstream\n",
            inner_xref.len()
        )
        .into_bytes();
        let xref_offset_2b = v2b.len();
        v2b.extend_from_slice(&inner_xref);
        v2b.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{xref_offset_2b}\n%%EOF\n").as_bytes(),
        );
        assert!(matches!(
            validate_raw_pdf_safety(&v2b),
            Err(ToolError::BillTooBig)
        ));

        // Vector 2c (1): `/F /Bogus /Filter [/FlateDecode]` on `/XRef` in Layer 1.
        let mut v2c_filter = format!(
            "%PDF-1.5\n2 0 obj << /Type /XRef /Size 3 /W [1 2 1] /F /Bogus /Filter [/FlateDecode] /Length {} >>\nstream\n",
            flate_bomb.len()
        )
        .into_bytes();
        v2c_filter.extend_from_slice(&flate_bomb);
        v2c_filter.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&v2c_filter),
            Err(ToolError::BillUnreadable)
        ));

        // Vector 2c (2): `/DP << >> /DecodeParms [ << /EarlyChange 0 >> ]` with
        // `/Filter [/LZWDecode]` on `/XRef` in Layer 1.
        let mut v2c_dp = format!(
            "%PDF-1.5\n2 0 obj << /Type /XRef /Size 3 /W [1 2 1] /Filter [/LZWDecode] /DP << >> /DecodeParms [ << /EarlyChange 0 >> ] /Length {} >>\nstream\n",
            lzw_bomb.len()
        )
        .into_bytes();
        v2c_dp.extend_from_slice(&lzw_bomb);
        v2c_dp.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&v2c_dp),
            Err(ToolError::BillTooBig)
        ));
    }

    #[test]
    fn b469_predictor_oom_and_panic_params_rejected_in_both_layers() {
        let small_zlib = zlib_zero_bomb(16);

        let invalid_cases: &[(&str, &str)] = &[
            (
                "predictor 0 OOM",
                "/Predictor 0 /Columns 500000000 /Colors 5 /BitsPerComponent 8",
            ),
            ("columns 0 div-by-zero panic", "/Predictor 2 /Columns 0"),
            ("colors 0 div-by-zero panic", "/Predictor 2 /Colors 0"),
            (
                "colors * bpc > 255 u8 overflow panic",
                "/Predictor 2 /Colors 32 /BitsPerComponent 8",
            ),
        ];

        for (label, parms) in invalid_cases {
            let mut stream_obj = format!(
                "<< /Filter /FlateDecode /DecodeParms << {parms} >> /Length {} >>\nstream\n",
                small_zlib.len()
            )
            .into_bytes();
            stream_obj.extend_from_slice(&small_zlib);
            stream_obj.extend_from_slice(b"\nendstream");

            let pdf_bytes = assemble_test_pdf(&[
                b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
                b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
                stream_obj,
            ]);

            assert!(
                matches!(
                    validate_raw_pdf_safety(&pdf_bytes),
                    Err(ToolError::BillUnreadable)
                ),
                "Layer 1 must reject {label}"
            );
            let parsed = Pdf::new(pdf_bytes).expect("parseable PDF");
            assert!(
                matches!(
                    validate_parsed_pdf_safety(&parsed),
                    Err(ToolError::BillUnreadable)
                ),
                "Layer 2 must reject {label}"
            );
        }

        // Valid PNG predictor with huge row buffer still triggers BillTooBig.
        let mut huge_row_obj = format!(
            "<< /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 500000000 /Colors 5 /BitsPerComponent 8 >> /Length {} >>\nstream\n",
            small_zlib.len()
        )
        .into_bytes();
        huge_row_obj.extend_from_slice(&small_zlib);
        huge_row_obj.extend_from_slice(b"\nendstream");
        let huge_row_pdf = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            huge_row_obj,
        ]);
        assert!(matches!(
            validate_raw_pdf_safety(&huge_row_pdf),
            Err(ToolError::BillTooBig)
        ));
        let parsed_huge_row = Pdf::new(huge_row_pdf).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_huge_row),
            Err(ToolError::BillTooBig)
        ));

        // Valid PNG predictor with normal parameters passes both layers.
        let mut valid_pred_obj = format!(
            "<< /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 4 /Colors 1 /BitsPerComponent 8 >> /Length {} >>\nstream\n",
            small_zlib.len()
        )
        .into_bytes();
        valid_pred_obj.extend_from_slice(&small_zlib);
        valid_pred_obj.extend_from_slice(b"\nendstream");
        let valid_pred_pdf = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            valid_pred_obj,
        ]);
        validate_raw_pdf_safety(&valid_pred_pdf).expect("Layer 1 accepts valid predictor");
        let parsed_valid = Pdf::new(valid_pred_pdf).expect("parseable PDF");
        validate_parsed_pdf_safety(&parsed_valid).expect("Layer 2 accepts valid predictor");
    }

    #[test]
    fn b469_external_f_filter_bypass_and_lopdf_raw_deflate_fallback_rejected() {
        let zlib_bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);
        // Strip the 2-byte RFC 1950 zlib header and 4-byte Adler-32 trailer,
        // then prepend `[0x06, 0x00]` (invalid zlib header that `lopdf` skips
        // before running raw deflate).
        let raw_deflate = &zlib_bomb[2..zlib_bomb.len() - 4];
        let mut lopdf_bomb = Vec::with_capacity(2 + raw_deflate.len());
        lopdf_bomb.extend_from_slice(&[0x06, 0x00]);
        lopdf_bomb.extend_from_slice(raw_deflate);

        // Direct bounded_flate_decode catches the 2-byte-skip raw deflate bomb.
        assert!(matches!(
            bounded_flate_decode(&lopdf_bomb, MAX_PDF_STREAM_DECOMPRESSED_BYTES, false),
            Err(ToolError::BillTooBig)
        ));

        // Page `/Contents` stream combining `/F (x)` with `/Filter /FlateDecode`
        // and the `[0x06, 0x00]` + raw-deflate bomb must be rejected by BOTH
        // Layer 1 and Layer 2 before `pdf-extract`/`lopdf` can run.
        let mut ext_stream = format!(
            "<< /F (x) /Filter /FlateDecode /Length {} >>\nstream\n",
            lopdf_bomb.len()
        )
        .into_bytes();
        ext_stream.extend_from_slice(&lopdf_bomb);
        ext_stream.extend_from_slice(b"\nendstream");

        let pdf_bytes = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            ext_stream,
        ]);
        assert!(matches!(
            validate_raw_pdf_safety(&pdf_bytes),
            Err(ToolError::BillUnreadable)
        ));
        let parsed = Pdf::new(pdf_bytes).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed),
            Err(ToolError::BillUnreadable)
        ));

        // Non-stream dictionary with `/F (example.pdf)` (e.g. a Filespec)
        // remains valid in both Layer 1 and Layer 2.
        let valid_filespec_pdf = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            b"<< /Length 0 >>\nstream\n\nendstream".to_vec(),
            b"<< /Type /Filespec /F (example.pdf) >>".to_vec(),
        ]);
        validate_raw_pdf_safety(&valid_filespec_pdf).expect("Layer 1 accepts non-stream Filespec");
        let parsed_filespec = Pdf::new(valid_filespec_pdf).expect("parseable PDF");
        validate_parsed_pdf_safety(&parsed_filespec).expect("Layer 2 accepts non-stream Filespec");
    }

    #[test]
    fn b469_layer1_no_quadratic_dos_on_repeated_unclosed_delimiters() {
        use std::time::{Duration, Instant};

        // 2 MiB of unclosed `(`
        let mut parens = b"%PDF-1.7\n".to_vec();
        parens.resize(parens.len() + 2_000_000, b'(');
        let t0 = Instant::now();
        let res_parens = validate_raw_pdf_safety(&parens);
        let elapsed_parens = t0.elapsed();
        assert!(matches!(res_parens, Err(ToolError::BillUnreadable)));
        assert!(
            elapsed_parens < Duration::from_secs(1),
            "repeated `(` took {elapsed_parens:?} (must be < 1s)"
        );

        // 2 MiB of unclosed `<` (`<<` repeated)
        let mut angles = b"%PDF-1.7\n".to_vec();
        angles.resize(angles.len() + 2_000_000, b'<');
        let t1 = Instant::now();
        let res_angles = validate_raw_pdf_safety(&angles);
        let elapsed_angles = t1.elapsed();
        assert!(matches!(res_angles, Err(ToolError::BillUnreadable)));
        assert!(
            elapsed_angles < Duration::from_secs(1),
            "repeated `<` took {elapsed_angles:?} (must be < 1s)"
        );

        // 2 MiB of `%` followed by `<<` on one comment line
        let mut percents = b"%PDF-1.7\n".to_vec();
        percents.resize(percents.len() + 2_000_000, b'%');
        percents.extend_from_slice(b"<<");
        let t2 = Instant::now();
        let _ = validate_raw_pdf_safety(&percents);
        let elapsed_percents = t2.elapsed();
        assert!(
            elapsed_percents < Duration::from_secs(1),
            "repeated `%` + `<<` took {elapsed_percents:?} (must be < 1s)"
        );
    }

    #[test]
    fn b469_flate_offset0_prefix_and_multistage_divergence_rejected() {
        // 1. Single-stage `/Filter /FlateDecode` with `[0x03, 0x00]` (valid
        // 0-byte raw deflate at offset 0) followed by a raw deflate bomb at
        // offset 2 (`lopdf` fallback) must be rejected with `BillTooBig`.
        let zlib_bomb = zlib_zero_bomb(MAX_PDF_STREAM_DECOMPRESSED_BYTES + 4_096);
        let raw_bomb = &zlib_bomb[2..zlib_bomb.len() - 4];
        let mut prefix03_bomb = Vec::with_capacity(2 + raw_bomb.len());
        prefix03_bomb.extend_from_slice(&[0x03, 0x00]);
        prefix03_bomb.extend_from_slice(raw_bomb);

        let mut single_stream = format!(
            "<< /Filter /FlateDecode /Length {} >>\nstream\n",
            prefix03_bomb.len()
        )
        .into_bytes();
        single_stream.extend_from_slice(&prefix03_bomb);
        single_stream.extend_from_slice(b"\nendstream");

        let pdf_single = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            single_stream,
        ]);
        assert!(matches!(
            validate_raw_pdf_safety(&pdf_single),
            Err(ToolError::BillTooBig)
        ));
        let parsed_single = Pdf::new(pdf_single).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_single),
            Err(ToolError::BillTooBig)
        ));

        // 2. Multi-stage `/Filter [/FlateDecode /FlateDecode]` with
        // `[0x03, 0x00]` + raw deflate of a small non-empty payload must be
        // rejected because `hayro` (offset 0) and `lopdf` (offset 2) decode
        // different payloads at stage 1.
        let small_zlib = zlib_zero_bomb(32);
        let raw_small = &small_zlib[2..small_zlib.len() - 4];
        let mut prefix03_small = Vec::with_capacity(2 + raw_small.len());
        prefix03_small.extend_from_slice(&[0x03, 0x00]);
        prefix03_small.extend_from_slice(raw_small);

        let mut multi_stream = format!(
            "<< /Filter [/FlateDecode /FlateDecode] /Length {} >>\nstream\n",
            prefix03_small.len()
        )
        .into_bytes();
        multi_stream.extend_from_slice(&prefix03_small);
        multi_stream.extend_from_slice(b"\nendstream");

        let pdf_multi = assemble_test_pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
            multi_stream,
        ]);
        assert!(matches!(
            validate_raw_pdf_safety(&pdf_multi),
            Err(ToolError::BillUnreadable | ToolError::BillTooBig)
        ));
        let parsed_multi = Pdf::new(pdf_multi).expect("parseable PDF");
        assert!(matches!(
            validate_parsed_pdf_safety(&parsed_multi),
            Err(ToolError::BillUnreadable | ToolError::BillTooBig)
        ));
    }

    #[test]
    fn b469_layer1_unresolved_indirect_decode_parms_rejected_on_obj_stm() {
        let small_zlib = zlib_zero_bomb(16);

        // 1. `/ObjStm` with `/Filter [/FlateDecode] /DecodeParms [ 99 0 R ]`
        // where `99 0 obj` is missing/hidden.
        let mut case1 = format!(
            "%PDF-1.5\n1 0 obj << /Type /ObjStm /N 1 /First 5 /Filter [/FlateDecode] /DecodeParms [ 99 0 R ] /Length {} >>\nstream\n",
            small_zlib.len()
        )
        .into_bytes();
        case1.extend_from_slice(&small_zlib);
        case1.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&case1),
            Err(ToolError::BillUnreadable)
        ));

        // 2. `/ObjStm` with `/Filter /FlateDecode /DecodeParms 99 0 R` where
        // `99 0 obj << /Predictor 12 /Columns 100 0 R >> endobj` appears BEFORE
        // `100 0 obj 2000000000 endobj` (forward reference inside indirect dict).
        let mut case2 = format!(
            "%PDF-1.5\n99 0 obj << /Predictor 12 /Columns 100 0 R >> endobj\n100 0 obj 2000000000 endobj\n1 0 obj << /Type /ObjStm /N 1 /First 5 /Filter /FlateDecode /DecodeParms 99 0 R /Length {} >>\nstream\n",
            small_zlib.len()
        )
        .into_bytes();
        case2.extend_from_slice(&small_zlib);
        case2.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&case2),
            Err(ToolError::BillUnreadable)
        ));

        // 3. `/ObjStm` with direct `/Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 100 0 R >>`
        // where `100 0 obj` is missing/hidden.
        let mut case3 = format!(
            "%PDF-1.5\n1 0 obj << /Type /ObjStm /N 1 /First 5 /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 100 0 R >> /Length {} >>\nstream\n",
            small_zlib.len()
        )
        .into_bytes();
        case3.extend_from_slice(&small_zlib);
        case3.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        assert!(matches!(
            validate_raw_pdf_safety(&case3),
            Err(ToolError::BillUnreadable)
        ));
    }
}
