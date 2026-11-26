// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure-Rust PDF structural and decompression-bomb pre-validator for
//! `read_bill` (B-461, D-1004, T-72).
//!
//! Runs in two layers before any PDF text extraction (`pdf-extract`) or page
//! rasterization (`hayro::render`):
//!
//! 1. [`validate_raw_pdf_safety`]: inspects raw PDF bytes *before* `Pdf::new`
//!    or `lopdf::Document::load_mem` parses xref/object streams, capping object
//!    count, stream count, page count, embedded `/XObject /Image` dimensions,
//!    and boundedly inflating `FlateDecode`, `LZWDecode` and `RunLengthDecode`
//!    streams (including chained ASCII/Flate/LZW filters and `/ObjStm` streams).
//! 2. [`validate_parsed_pdf_safety`]: walks the lazily parsed
//!    [`hayro::hayro_syntax::Pdf`] objects (without calling `stream.decoded()`)
//!    to catch any indirect `/Filter`, `/Width`, or `/Height` references or
//!    object-stream-hidden image XObjects before rendering or text extraction.

use std::collections::HashMap;

use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::object::{Array, Dict, Name, Number, Object, Stream};

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
/// `/XObject /Image` (32 MP, ~128 MiB RGBA, B-461, T-72).
pub(crate) const MAX_PDF_IMAGE_PIXELS: u64 = 32_000_000;

/// Maximum cumulative pixel count across all embedded PDF images
/// (128 MP, B-461, T-72).
pub(crate) const MAX_PDF_TOTAL_IMAGE_PIXELS: u64 = 128_000_000;

/// Maximum filter stages chained on a single PDF stream (B-461, T-72).
const MAX_PDF_FILTER_STAGES: usize = 4;

/// Maximum bits per component accepted for an embedded PDF image.
const MAX_PDF_IMAGE_BPC: u64 = 16;

/// Filter kinds checked by the bounded stream decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PdfFilter {
    Flate,
    Lzw,
    RunLength,
    AsciiHex,
    Ascii85,
    Ccitt,
    Other,
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
            _ => Self::Other,
        }
    }
}

/// Simple indirect object values pre-indexed in pass 1 of raw PDF scanning so
/// indirect `/Length`, `/Width`, `/Height`, and `/Filter` references are
/// resolved even before `Pdf::new`.
enum SimpleIndirect {
    Int(u64),
    Filters(Vec<PdfFilter>),
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

    fn check_image_dims(&mut self, width: u64, height: u64) -> Result<(), ToolError> {
        let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
        if width > max_dim || height > max_dim {
            return Err(ToolError::BillTooBig);
        }
        let pixels = width.saturating_mul(height);
        if pixels > MAX_PDF_IMAGE_PIXELS {
            return Err(ToolError::BillTooBig);
        }
        self.total_image_pixels = self.total_image_pixels.saturating_add(pixels);
        if self.total_image_pixels > MAX_PDF_TOTAL_IMAGE_PIXELS {
            return Err(ToolError::BillTooBig);
        }
        Ok(())
    }
}

/// Layer 1: Pure-Rust structural and stream-bomb pre-validator run directly on
/// raw PDF bytes *before* invoking `hayro::hayro_syntax::Pdf::new` or
/// `pdf_extract`.
pub(crate) fn validate_raw_pdf_safety(bytes: &[u8]) -> Result<(), ToolError> {
    let header_window = &bytes[..bytes.len().min(1024)];
    if !contains_subslice(header_window, b"%PDF-") {
        return Err(ToolError::BillUnreadable);
    }

    // Pass 1: count indirect object definitions and capture simple scalar
    // indirect objects (`N G obj <int|/Filter|[...]> endobj`).
    let indirects = scan_indirect_objects(bytes)?;

    // Pass 2: scan all dictionaries and `stream ... endstream` blocks.
    let mut budget = Budget::default();
    let mut stream_count = 0usize;
    let mut page_count = 0usize;
    let mut pos = 0usize;

    while pos < bytes.len() {
        if bytes[pos] == b'%' {
            pos = skip_line(bytes, pos);
            continue;
        }
        if bytes[pos] == b'<'
            && bytes.get(pos + 1) == Some(&b'<')
            && let Some((dict_end, dict_bytes)) = extract_balanced_dict(bytes, pos)
        {
            let info = inspect_raw_dict(dict_bytes, &indirects);
            if info.is_page {
                page_count += 1;
                if page_count > MAX_PDF_PAGES {
                    return Err(ToolError::BillTooBig);
                }
            }
            check_raw_dict_image(&info, &mut budget)?;

            let after_ws = skip_pdf_ws_and_comments(bytes, dict_end);
            if bytes[after_ws..].starts_with(b"stream")
                && matches!(bytes.get(after_ws + 6), Some(b'\n' | b'\r'))
            {
                stream_count += 1;
                if stream_count > MAX_PDF_STREAMS {
                    return Err(ToolError::BillTooBig);
                }
                let data_start = skip_stream_eol(bytes, after_ws + 6);
                let (stream_slice, next_pos) = locate_stream_slice(bytes, data_start, info.length);
                if !info.filters.is_empty() {
                    if info.filters.len() > MAX_PDF_FILTER_STAGES {
                        return Err(ToolError::BillTooBig);
                    }
                    let decoded = check_filter_chain(
                        stream_slice,
                        &info.filters,
                        info.early_change,
                        info.is_obj_stm,
                        &mut budget,
                    )?;
                    if let Some(obj_stm_bytes) = decoded {
                        scan_obj_stm_dicts(&obj_stm_bytes, &indirects, &mut budget)?;
                    }
                }
                pos = next_pos;
                continue;
            }
            pos = dict_end;
            continue;
        }
        pos += 1;
    }

    Ok(())
}

/// Layer 2: Validates a lazily parsed [`Pdf`] before text extraction or page
/// rendering, catching indirect `/Filter`, `/Width`, or `/Height` references
/// and object-stream-hidden image XObjects without calling `stream.decoded()`.
pub(crate) fn validate_parsed_pdf_safety(pdf: &Pdf) -> Result<(), ToolError> {
    if pdf.len() > MAX_PDF_OBJECTS {
        return Err(ToolError::BillTooBig);
    }
    if pdf.pages().len() > MAX_PDF_PAGES {
        return Err(ToolError::BillTooBig);
    }

    let mut budget = Budget::default();
    let mut stream_count = 0usize;

    for obj in pdf.objects() {
        match obj {
            Object::Stream(stream) => {
                stream_count += 1;
                if stream_count > MAX_PDF_STREAMS {
                    return Err(ToolError::BillTooBig);
                }
                validate_hayro_stream(&stream, &mut budget)?;
            }
            Object::Dict(dict) => {
                validate_hayro_image_dict(&dict, &mut budget)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn validate_hayro_stream(stream: &Stream<'_>, budget: &mut Budget) -> Result<(), ToolError> {
    let dict = stream.dict();
    validate_hayro_image_dict(dict, budget)?;

    let filters = hayro_dict_filters(dict)?;
    if filters.is_empty() {
        return Ok(());
    }

    if filters.contains(&PdfFilter::Ccitt) {
        check_hayro_ccitt_params(dict, budget)?;
    }

    let needs_check = filters
        .iter()
        .any(|f| matches!(f, PdfFilter::Flate | PdfFilter::Lzw | PdfFilter::RunLength));
    if !needs_check {
        return Ok(());
    }

    let early_change = dict
        .get::<Dict<'_>>(b"DecodeParms")
        .and_then(|d| d.get::<i32>(b"EarlyChange"))
        .is_none_or(|v| v != 0);

    let raw = stream.raw_data();
    let _ = check_filter_chain(&raw, &filters, early_change, false, budget)?;
    Ok(())
}

/// Resolves `/Filter` (or abbreviated `/F`) from a `hayro-syntax` stream
/// dictionary, handling both single names and arrays (including indirect
/// references resolved transparently by `Dict::get`).
fn hayro_dict_filters(dict: &Dict<'_>) -> Result<Vec<PdfFilter>, ToolError> {
    if let Some(name) = dict
        .get::<Name<'_>>(b"Filter")
        .or_else(|| dict.get::<Name<'_>>(b"F"))
    {
        return Ok(vec![PdfFilter::from_pdf_name(name.as_ref())]);
    }
    if let Some(arr) = dict
        .get::<Array<'_>>(b"Filter")
        .or_else(|| dict.get::<Array<'_>>(b"F"))
    {
        let mut filters = Vec::new();
        for name in arr.iter::<Name<'_>>() {
            filters.push(PdfFilter::from_pdf_name(name.as_ref()));
            if filters.len() > MAX_PDF_FILTER_STAGES {
                return Err(ToolError::BillTooBig);
            }
        }
        return Ok(filters);
    }
    Ok(Vec::new())
}

fn validate_hayro_image_dict(dict: &Dict<'_>, budget: &mut Budget) -> Result<(), ToolError> {
    let is_image = dict
        .get::<Name<'_>>(b"Subtype")
        .is_some_and(|n| n.as_ref() == b"Image")
        || dict.get::<bool>(b"ImageMask").unwrap_or(false);
    if !is_image {
        return Ok(());
    }
    if let Some(bpc) = dict.get::<Number>(b"BitsPerComponent") {
        let v = bpc.as_i64();
        if v < 0 || (v as u64) > MAX_PDF_IMAGE_BPC {
            return Err(ToolError::BillUnreadable);
        }
    }
    let width = dict
        .get::<Number>(b"Width")
        .or_else(|| dict.get::<Number>(b"W"))
        .map(|n| n.as_i64());
    let height = dict
        .get::<Number>(b"Height")
        .or_else(|| dict.get::<Number>(b"H"))
        .map(|n| n.as_i64());
    if let (Some(w), Some(h)) = (width, height) {
        if w < 0 || h < 0 {
            return Err(ToolError::BillUnreadable);
        }
        budget.check_image_dims(w as u64, h as u64)?;
    }
    Ok(())
}

fn check_hayro_ccitt_params(dict: &Dict<'_>, budget: &mut Budget) -> Result<(), ToolError> {
    if let Some(parms) = dict.get::<Dict<'_>>(b"DecodeParms") {
        let cols = parms.get::<Number>(b"Columns").map_or(0, |n| n.as_i64());
        let rows = parms.get::<Number>(b"Rows").map_or(0, |n| n.as_i64());
        if cols < 0 || rows < 0 {
            return Err(ToolError::BillUnreadable);
        }
        let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
        if (cols as u64) > max_dim || (rows as u64) > max_dim {
            return Err(ToolError::BillTooBig);
        }
        if cols > 0 && rows > 0 {
            budget.check_image_dims(cols as u64, rows as u64)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Raw PDF scanning helpers (Pass 1 & Pass 2)
// ---------------------------------------------------------------------------

fn scan_indirect_objects(bytes: &[u8]) -> Result<HashMap<u32, SimpleIndirect>, ToolError> {
    let mut map = HashMap::new();
    let mut object_count = 0usize;
    let mut pos = 0usize;

    while pos < bytes.len() {
        if bytes[pos] == b'%' {
            pos = skip_line(bytes, pos);
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
            if let Some((val, next)) = parse_simple_indirect_value(bytes, body_start) {
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

fn parse_simple_indirect_value(bytes: &[u8], start: usize) -> Option<(SimpleIndirect, usize)> {
    if start >= bytes.len() {
        return None;
    }
    let (val, end) = if bytes[start].is_ascii_digit() {
        let (n, end) = parse_u64_ascii(bytes, start)?;
        (SimpleIndirect::Int(n), end)
    } else if bytes[start] == b'/' {
        let (name, end) = parse_pdf_name(bytes, start)?;
        (
            SimpleIndirect::Filters(vec![PdfFilter::from_pdf_name(&name)]),
            end,
        )
    } else if bytes[start] == b'[' {
        let (filters, end) = parse_filter_array(bytes, start)?;
        (SimpleIndirect::Filters(filters), end)
    } else {
        return None;
    };
    let after = skip_pdf_ws_and_comments(bytes, end);
    if bytes[after..].starts_with(b"endobj") {
        Some((val, after + 6))
    } else {
        None
    }
}

/// Extracted metadata from a raw PDF dictionary `<< ... >>`.
struct RawDictInfo {
    is_page: bool,
    is_image: bool,
    is_obj_stm: bool,
    width: Option<u64>,
    height: Option<u64>,
    bpc: Option<u64>,
    ccitt_columns: Option<u64>,
    ccitt_rows: Option<u64>,
    length: Option<usize>,
    filters: Vec<PdfFilter>,
    early_change: bool,
}

fn inspect_raw_dict(dict: &[u8], indirects: &HashMap<u32, SimpleIndirect>) -> RawDictInfo {
    let mut info = RawDictInfo {
        is_page: false,
        is_image: false,
        is_obj_stm: false,
        width: None,
        height: None,
        bpc: None,
        ccitt_columns: None,
        ccitt_rows: None,
        length: None,
        filters: Vec::new(),
        early_change: true,
    };

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
                        b"ImageMask" => {
                            if inner[val_pos..].starts_with(b"true") {
                                info.is_image = true;
                                i = val_pos + 4;
                                continue;
                            }
                        }
                        b"Width" | b"W" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.width = Some(n);
                                i = end;
                                continue;
                            }
                        }
                        b"Height" | b"H" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.height = Some(n);
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
                        b"Filter" | b"F" => {
                            if let Some((filters, end)) =
                                parse_filters_or_ref(inner, val_pos, indirects)
                            {
                                info.filters = filters;
                                i = end;
                                continue;
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Inside a nested dictionary such as `/DecodeParms << ... >>`.
                    match key.as_slice() {
                        b"EarlyChange" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.early_change = n != 0;
                                i = end;
                                continue;
                            }
                        }
                        b"Columns" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.ccitt_columns = Some(n);
                                i = end;
                                continue;
                            }
                        }
                        b"Rows" => {
                            if let Some((n, end)) = parse_int_or_ref(inner, val_pos, indirects) {
                                info.ccitt_rows = Some(n);
                                i = end;
                                continue;
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

    info
}

fn check_raw_dict_image(info: &RawDictInfo, budget: &mut Budget) -> Result<(), ToolError> {
    if info.is_image {
        if let Some(bpc) = info.bpc
            && bpc > MAX_PDF_IMAGE_BPC
        {
            return Err(ToolError::BillUnreadable);
        }
        if let (Some(w), Some(h)) = (info.width, info.height) {
            budget.check_image_dims(w, h)?;
        }
    }
    if info.filters.contains(&PdfFilter::Ccitt) {
        let max_dim = u64::from(MAX_BILL_DECODE_DIMENSION);
        if info.ccitt_columns.is_some_and(|c| c > max_dim)
            || info.ccitt_rows.is_some_and(|r| r > max_dim)
        {
            return Err(ToolError::BillTooBig);
        }
        if let (Some(c), Some(r)) = (info.ccitt_columns, info.ccitt_rows) {
            budget.check_image_dims(c, r)?;
        }
    }
    Ok(())
}

fn scan_obj_stm_dicts(
    bytes: &[u8],
    indirects: &HashMap<u32, SimpleIndirect>,
    budget: &mut Budget,
) -> Result<(), ToolError> {
    let mut pos = 0usize;
    while pos < bytes.len() {
        if bytes[pos] == b'<'
            && bytes.get(pos + 1) == Some(&b'<')
            && let Some((dict_end, dict_bytes)) = extract_balanced_dict(bytes, pos)
        {
            let info = inspect_raw_dict(dict_bytes, indirects);
            check_raw_dict_image(&info, budget)?;
            pos = dict_end;
            continue;
        }
        pos += 1;
    }
    Ok(())
}

fn extract_balanced_dict(bytes: &[u8], start: usize) -> Option<(usize, &[u8])> {
    if !bytes[start..].starts_with(b"<<") {
        return None;
    }
    let mut i = start;
    let mut depth = 0usize;
    // Cap scan window for a single dictionary to 1 MiB to avoid quadratic scans
    // on malformed unclosed `<<`.
    let limit = bytes.len().min(start.saturating_add(1024 * 1024));
    while i < limit {
        match bytes[i] {
            b'%' => i = skip_line(bytes, i),
            b'(' => i = skip_pdf_literal_string(bytes, i),
            b'<' if bytes.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 2;
            }
            b'>' if bytes.get(i + 1) == Some(&b'>') => {
                depth = depth.saturating_sub(1);
                i += 2;
                if depth == 0 {
                    return Some((i, &bytes[start..i]));
                }
            }
            _ => i += 1,
        }
    }
    None
}

fn locate_stream_slice(
    bytes: &[u8],
    data_start: usize,
    declared_len: Option<usize>,
) -> (&[u8], usize) {
    if let Some(len) = declared_len
        && let Some(end) = data_start.checked_add(len)
        && end <= bytes.len()
    {
        let after = skip_pdf_ws(bytes, end);
        if bytes[after..].starts_with(b"endstream") {
            return (&bytes[data_start..end], after + 9);
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
        return (&bytes[data_start..end], data_start + rel_end + 9);
    }
    (&bytes[data_start..], bytes.len())
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
                Some(SimpleIndirect::Int(v)) => Some((*v, ws2 + 1)),
                _ => None,
            };
        }
    }
    Some((first, after_first))
}

fn parse_filters_or_ref(
    bytes: &[u8],
    pos: usize,
    indirects: &HashMap<u32, SimpleIndirect>,
) -> Option<(Vec<PdfFilter>, usize)> {
    if pos >= bytes.len() {
        return None;
    }
    match bytes[pos] {
        b'/' => {
            let (name, end) = parse_pdf_name(bytes, pos)?;
            Some((vec![PdfFilter::from_pdf_name(&name)], end))
        }
        b'[' => parse_filter_array(bytes, pos),
        b if b.is_ascii_digit() => {
            let (obj_num, after_first) = parse_u64_ascii(bytes, pos)?;
            let ws1 = skip_pdf_ws(bytes, after_first);
            let (_gen, after_gen) = parse_u64_ascii(bytes, ws1)?;
            let ws2 = skip_pdf_ws(bytes, after_gen);
            if bytes.get(ws2) == Some(&b'R') {
                let id = u32::try_from(obj_num).ok()?;
                if let Some(SimpleIndirect::Filters(list)) = indirects.get(&id) {
                    return Some((list.clone(), ws2 + 1));
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_filter_array(bytes: &[u8], start: usize) -> Option<(Vec<PdfFilter>, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut filters = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() {
        i = skip_pdf_ws_and_comments(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b']' {
            return Some((filters, i + 1));
        }
        if bytes[i] == b'/' {
            let (name, end) = parse_pdf_name(bytes, i)?;
            filters.push(PdfFilter::from_pdf_name(&name));
            if filters.len() > MAX_PDF_FILTER_STAGES + 1 {
                return Some((filters, end));
            }
            i = end;
        } else {
            return None;
        }
    }
    None
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

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    find_subslice(haystack, needle).is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------
// Bounded stream filter chain & decoders (RFC 1950/1951 Flate, LZW, RLE, ASCII)
// ---------------------------------------------------------------------------

/// Runs the PDF stream filter chain within `budget`.
///
/// Returns `Ok(Some(bytes))` only when `capture_final` is `true` and decoding
/// succeeded within bounds; returns `Err(ToolError::BillTooBig)` immediately
/// the moment any stage exceeds the per-stream or cumulative PDF budget.
/// Malformed compressed streams stop early and return `Ok(None)` so `hayro` /
/// `pdf-extract` can report `BillUnreadable` normally.
fn check_filter_chain(
    raw: &[u8],
    filters: &[PdfFilter],
    early_change: bool,
    capture_final: bool,
    budget: &mut Budget,
) -> Result<Option<Vec<u8>>, ToolError> {
    let mut owned: Option<Vec<u8>> = None;

    for (idx, &filter) in filters.iter().enumerate() {
        let is_last = idx + 1 == filters.len();
        let need_bytes = capture_final || !is_last;
        let limit = budget.remaining_decompressed();
        let input = owned.as_deref().unwrap_or(raw);

        match filter {
            PdfFilter::Flate => {
                let Some((produced, out)) = bounded_flate_decode(input, limit, need_bytes)? else {
                    return Ok(None);
                };
                budget.charge_decompressed(produced)?;
                owned = out;
            }
            PdfFilter::Lzw => {
                let Some((produced, out)) =
                    bounded_lzw_decode(input, early_change, limit, need_bytes)?
                else {
                    return Ok(None);
                };
                budget.charge_decompressed(produced)?;
                owned = out;
            }
            PdfFilter::RunLength => {
                let Some((produced, out)) = bounded_rle_decode(input, limit, need_bytes)? else {
                    return Ok(None);
                };
                budget.charge_decompressed(produced)?;
                owned = out;
            }
            PdfFilter::AsciiHex => {
                if need_bytes {
                    let Some(out) = bounded_ascii_hex_decode(input, limit)? else {
                        return Ok(None);
                    };
                    owned = Some(out);
                }
            }
            PdfFilter::Ascii85 => {
                if need_bytes {
                    let Some(out) = bounded_ascii85_decode(input, limit)? else {
                        return Ok(None);
                    };
                    owned = Some(out);
                }
            }
            PdfFilter::Ccitt | PdfFilter::Other => {
                // Image codec filters (DCT/JPX/JBIG2/CCITT) are bounded by the
                // image dimension/pixel checks rather than byte-stream inflation.
                return Ok(None);
            }
        }
    }

    Ok(if capture_final { owned } else { None })
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
    // Strip optional zlib header (RFC 1950), falling back to raw deflate.
    let deflate_slice = if data.len() >= 2 {
        let cmf = data[0];
        let flg = data[1];
        let header_ok = (cmf & 0x0f) == 8 && (((u16::from(cmf) << 8) | u16::from(flg)) % 31 == 0);
        if header_ok {
            let offset = if (flg & 0x20) != 0 { 6 } else { 2 };
            if offset <= data.len() {
                &data[offset..]
            } else {
                return Ok(None);
            }
        } else {
            data
        }
    } else {
        return Ok(None);
    };

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

fn bounded_ascii_hex_decode(data: &[u8], max_out: usize) -> Result<Option<Vec<u8>>, ToolError> {
    let mut out = Vec::new();
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
            if out.len() >= max_out {
                return Err(ToolError::BillTooBig);
            }
            out.push((h << 4) | nibble);
        } else {
            high = Some(nibble);
        }
    }
    if let Some(h) = high {
        if out.len() >= max_out {
            return Err(ToolError::BillTooBig);
        }
        out.push(h << 4);
    }
    Ok(Some(out))
}

fn bounded_ascii85_decode(data: &[u8], max_out: usize) -> Result<Option<Vec<u8>>, ToolError> {
    let mut out = Vec::new();
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
            if out.len().saturating_add(4) > max_out {
                return Err(ToolError::BillTooBig);
            }
            out.extend_from_slice(&[0, 0, 0, 0]);
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
            if out.len().saturating_add(4) > max_out {
                return Err(ToolError::BillTooBig);
            }
            out.extend_from_slice(&word.to_be_bytes());
            count = 0;
        }
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
        if out.len().saturating_add(emit) > max_out {
            return Err(ToolError::BillTooBig);
        }
        out.extend_from_slice(&bytes[..emit]);
    }
    Ok(Some(out))
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
}
