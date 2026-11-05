//! gdi32 + msimg32 (gradient fill) declarations.
#![allow(non_snake_case, dead_code)]

use super::user32::RECT;
use super::*;

pub const PS_SOLID: i32 = 0;

pub const BI_RGB: u32 = 0;
pub const DIB_RGB_COLORS: u32 = 0;
pub const SRCCOPY: u32 = 0x00CC_0020;

pub const TRANSPARENT: i32 = 1;
pub const OPAQUE: i32 = 2;

pub const GRADIENT_FILL_RECT_H: u32 = 0;
pub const GRADIENT_FILL_RECT_V: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TRIVERTEX {
    pub x: LONG,
    pub y: LONG,
    pub Red: u16,
    pub Green: u16,
    pub Blue: u16,
    pub Alpha: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GRADIENT_RECT {
    pub UpperLeft: u32,
    pub LowerRight: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BITMAPINFOHEADER {
    pub biSize: u32,
    pub biWidth: LONG,
    pub biHeight: LONG,
    pub biPlanes: u16,
    pub biBitCount: u16,
    pub biCompression: u32,
    pub biSizeImage: u32,
    pub biXPelsPerMeter: LONG,
    pub biYPelsPerMeter: LONG,
    pub biClrUsed: u32,
    pub biClrImportant: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BITMAPINFO {
    pub bmiHeader: BITMAPINFOHEADER,
    pub bmiColors: [u32; 1],
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateSolidBrush(color: COLORREF) -> HBRUSH;
    pub fn CreatePen(style: i32, width: i32, color: COLORREF) -> HANDLE;
    pub fn RoundRect(hdc: HDC, l: i32, t: i32, r: i32, b: i32, w: i32, h: i32) -> BOOL;
    pub fn Polyline(hdc: HDC, pts: *const super::user32::POINT, count: i32) -> BOOL;
    pub fn DeleteObject(obj: HGDIOBJ) -> BOOL;
    pub fn SelectObject(hdc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
    pub fn SetTextColor(hdc: HDC, color: COLORREF) -> COLORREF;
    pub fn SetBkMode(hdc: HDC, mode: i32) -> i32;
    pub fn SetBkColor(hdc: HDC, color: COLORREF) -> COLORREF;
    pub fn GetStockObject(index: i32) -> HGDIOBJ;
    pub fn CreateFontW(
        h: i32,
        w: i32,
        esc: i32,
        orient: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        outprec: u32,
        clipprec: u32,
        quality: u32,
        pitchfamily: u32,
        face: *const u16,
    ) -> HFONT;
    pub fn TextOutW(hdc: HDC, x: i32, y: i32, text: *const u16, len: i32) -> BOOL;
    pub fn FillRect(hdc: HDC, rect: *const RECT, brush: HBRUSH) -> i32;
    pub fn MoveToEx(hdc: HDC, x: i32, y: i32, lppt: *mut super::user32::POINT) -> BOOL;
    pub fn LineTo(hdc: HDC, x: i32, y: i32) -> BOOL;
    pub fn GetTextExtentPoint32W(hdc: HDC, lpString: *const u16, c: i32, lpSize: *mut super::SIZE) -> BOOL;
    pub fn StretchDIBits(
        hdc: HDC,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        bits: *const c_void_raw,
        bmi: *const BITMAPINFO,
        usage: u32,
        rop: u32,
    ) -> i32;
    pub fn SetStretchBltMode(hdc: HDC, mode: i32) -> i32;
    pub fn DrawTextW(
        hdc: HDC,
        text: *const u16,
        len: i32,
        rect: *mut RECT,
        format: u32,
    ) -> i32;
    pub fn CreateCompatibleDC(hdc: HDC) -> HDC;
    pub fn CreateCompatibleBitmap(hdc: HDC, cx: i32, cy: i32) -> HBITMAP;
    pub fn DeleteDC(hdc: HDC) -> BOOL;
    pub fn BitBlt(
        hdc: HDC,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        hdcSrc: HDC,
        x1: i32,
        y1: i32,
        rop: u32,
    ) -> BOOL;
    pub fn Ellipse(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32) -> BOOL;
    pub fn CreateDIBSection(
        hdc: HDC,
        pbmi: *const BITMAPINFO,
        usage: u32,
        ppvBits: *mut *mut c_void_raw,
        hSection: HANDLE,
        offset: u32,
    ) -> HBITMAP;
    pub fn GetDIBits(
        hdc: HDC,
        hbm: HBITMAP,
        start: u32,
        cLines: u32,
        lpvBits: *mut c_void_raw,
        lpbmi: *mut BITMAPINFO,
        usage: u32,
    ) -> i32;
}

#[link(name = "msimg32")]
extern "system" {
    pub fn GradientFill(
        hdc: HDC,
        vertices: *mut TRIVERTEX,
        nvertices: usize,
        meshes: *mut c_void_raw,
        nmeshes: usize,
        mode: u32,
    ) -> BOOL;
}

pub const FW_BOLD: i32 = 700;
pub const FW_NORMAL: i32 = 400;
pub const DEFAULT_CHARSET: u32 = 1;
pub const OUT_DEFAULT_PRECIS: u32 = 0;
pub const CLIP_DEFAULT_PRECIS: u32 = 0;
pub const CLEARTYPE_QUALITY: u32 = 5;
pub const FF_DONTCARE: u32 = 0;

pub const DT_LEFT: u32 = 0x0;
pub const DT_SINGLELINE: u32 = 0x20;
pub const DT_VCENTER: u32 = 0x4;
pub const DT_CALCRECT: u32 = 0x400;
pub const DT_NOCLIP: u32 = 0x100;

pub const COLORONCOLOR: i32 = 3;
pub const HALFTONE: i32 = 4;

/// COLORREF from 8-bit RGB.
pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// Parse "DodgerBlue"-style names is overkill; accept "#RRGGBB" or a small named table.
pub fn parse_color(s: &str, fallback: COLORREF) -> COLORREF {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                return rgb(
                    ((v >> 16) & 0xFF) as u8,
                    ((v >> 8) & 0xFF) as u8,
                    (v & 0xFF) as u8,
                );
            }
        }
    }
    match s.to_ascii_lowercase().as_str() {
        "dodgerblue" => rgb(30, 144, 255),
        "deepskyblue" => rgb(0, 191, 255),
        "white" => rgb(255, 255, 255),
        "black" => rgb(0, 0, 0),
        "red" => rgb(255, 0, 0),
        "lime" | "green" => rgb(0, 255, 0),
        "yellow" => rgb(255, 255, 0),
        _ => fallback,
    }
}
