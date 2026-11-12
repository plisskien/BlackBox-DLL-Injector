//! Pure Win32 Obsidian GUI Remaster.
//! Authentic dark obsidian & white porcelain accent aesthetic.
//! Zero third-party crates, zero runtime dependencies, high-DPI dynamic scaling.

use crate::config::{AdvancedOptions, Module, ScrambleOptions, Settings, SettingsScope};
use crate::inject::InjectOptions;
use crate::util::{from_wsz, hiword, last_error, loword, wstr};
use crate::win32::dwmapi;
use crate::win32::gdi32::{self, BITMAPINFO, DIB_RGB_COLORS, SRCCOPY};
use crate::win32::kernel32;
use crate::win32::user32::*;
use crate::win32::uxtheme;
use crate::win32::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const WM_APP_LOG: u32 = WM_APP + 1;
pub const WM_APP_INJECT_DONE: u32 = WM_APP + 2;
pub const WM_APP_ROW_MENU: u32 = WM_APP + 3;
// Move the row in wp by lp places (Ctrl+Up / Ctrl+Down, or the row menu)
pub const WM_APP_MOVE_ROW: u32 = WM_APP + 4;
// Edit the per-file delay of the row in wp
pub const WM_APP_EDIT_DELAY: u32 = WM_APP + 5;
// Drag & drop finished: move row wp to row lp
pub const WM_APP_MOVE_ROW_TO: u32 = WM_APP + 6;
// Edit all per-file options of the row in wp
pub const WM_APP_FILE_OPTIONS: u32 = WM_APP + 7;
// Remove selected row(s) from the list (Delete key)
pub const WM_APP_DELETE_ROW: u32 = WM_APP + 9;

// Columns of the DLL list. The order column is the injection position, the method column
// the per-file method, the delay column the per-file wait, and the dots in the last cell
// open the row menu.
const LIST_ORDER_COL: i32 = 0;
const LIST_NAME_COL: i32 = 1;
const LIST_ARCH_COL: i32 = 2;
const LIST_PROCESS_COL: i32 = 3;
const LIST_METHOD_COL: i32 = 4;
const LIST_DELAY_COL: i32 = 5;
const LIST_PATH_COL: i32 = 6;
const LIST_ACTIONS_COL: i32 = 7;

// Row menu commands
const MENU_ENABLED: u32 = 501;
const MENU_REMOVE: u32 = 502;
const MENU_OPEN_LOCATION: u32 = 504;
const MENU_MOVE_UP: u32 = 505;
const MENU_MOVE_DOWN: u32 = 506;
const MENU_SET_DELAY: u32 = 507;
const MENU_DEFAULT_DELAY: u32 = 508;
const MENU_FILE_OPTIONS: u32 = 509;
const MENU_UNLOAD: u32 = 510;
const MENU_UNLOAD_WIPE: u32 = 511;
const MENU_DLL_DETAILS: u32 = 512;
const MENU_SET_METHOD: u32 = 513;

// Largest per-file delay accepted, in milliseconds (one hour)
const MAX_MODULE_DELAY_MS: u32 = 3_600_000;

// Project links shown in About (clickable).
const THREAD_URL: &str = crate::update::THREAD_URL;
const GITHUB_URL: &str = crate::update::GITHUB_URL;
const DEV_URL: &str = crate::update::DEV_URL;

// Embedded application icon (window/taskbar icon and the target-card logo)
const LOGO_ICO: &[u8] = include_bytes!("../assets/blackbox.ico");

// Base Window Dimensions at 100% scale
const BASE_CLIENT_W: i32 = 520;
const BASE_CLIENT_H: i32 = 553;

const BASE_SETTINGS_W: i32 = 490;
const BASE_SETTINGS_H: i32 = 822;

const BASE_PICKER_W: i32 = 480;
const BASE_PICKER_H: i32 = 540;

const BASE_ABOUT_W: i32 = 460;
const BASE_ABOUT_H: i32 = 320;
const BASE_MSG_W: i32 = 390;

const LVM_FIRST_ID: u32 = 0x1000;
const LVM_SETBKCOLOR_ID: u32 = LVM_FIRST_ID + 1;
const LVM_SETTEXTCOLOR_ID: u32 = LVM_FIRST_ID + 36;
const LVM_SETTEXTBKCOLOR_ID: u32 = LVM_FIRST_ID + 38;
const LVM_GETHEADER_ID: u32 = LVM_FIRST_ID + 31;
const LVM_SETCOLUMNWIDTH_ID: u32 = LVM_FIRST_ID + 30;
const LVM_GETCOLUMNWIDTH_ID: u32 = LVM_FIRST_ID + 29;
const LVM_GETITEMW_ID: u32 = LVM_FIRST_ID + 75;
const ODT_COMBOBOX: u32 = 3;
#[allow(dead_code)]
const CBS_OWNERDRAWFIXED_C: u32 = 0x0010;
const CBS_HASSTRINGS_C: u32 = 0x0200;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_SETITEMHEIGHT: u32 = 0x0153;
const LVM_SETIMAGELIST: u32 = LVM_FIRST_ID + 3;
const LVSIL_SMALL: WPARAM = 1;
const LVIF_IMAGE_C: u32 = 0x0002;
const ILC_COLOR32: u32 = 0x0020;
const ILC_MASK: u32 = 0x0001;
#[allow(dead_code)]
const LVN_ITEMCHANGED_C: u32 = 0u32.wrapping_sub(100) - 1;
const NM_CUSTOMDRAW_C: u32 = 0u32.wrapping_sub(12);
const CDDS_PREPAINT_C: u32 = 0x00000001;
const CDDS_ITEM_C: u32 = 0x00010000;
const CDDS_ITEMPREPAINT_C: u32 = CDDS_ITEM_C | CDDS_PREPAINT_C;
const CDRF_NOTIFYITEMDRAW_C: u32 = 0x00000020;
const CDRF_SKIPDEFAULT_C: u32 = 0x00000004;
const HDI_TEXT_C: u32 = 0x0002;
const HDM_GETITEMW_C: u32 = 0x1200 + 11;
const DT_NOPREFIX_FLAG: u32 = 0x00000800;
const CB_SETDROPPEDWIDTH: u32 = 0x0160;
const WM_SETREDRAW: u32 = 0x000B;
#[allow(dead_code)]
const WM_GETICON: u32 = 0x007F;
const WM_TIMER: u32 = 0x0113;
const DPI_AWARENESS_PER_MONITOR_V2: isize = -4;
const ILD_TRANSPARENT_C: u32 = 0x0001;
const I_IMAGENONE_C: i32 = -2;
const VK_ESCAPE_C: usize = 0x1B;
const VK_RETURN_C: usize = 0x0D;
const VK_DELETE_C: usize = 0x2E;
const BM_CLICK_C: u32 = 0x00F5;
const BS_OWNERDRAW_C: isize = 0x0B;
const GWL_STYLE_C: i32 = -16;
const LVM_GETCOUNTPERPAGE_C: u32 = 0x1028;
const SM_CXVSCROLL_C: i32 = 2;

// Corner radii in logical px (scaled with `s`). Every surface of one kind shares a value.
const R_CARD: i32 = 10;
const R_BUTTON: i32 = 8;
const R_INPUT: i32 = 6;
const R_CHECK: i32 = 4;

#[link(name = "comctl32")]
extern "system" {
    fn ImageList_Create(cx: i32, cy: i32, flags: u32, cInitial: i32, cGrow: i32) -> HANDLE;
    fn ImageList_Destroy(himl: HANDLE) -> BOOL;
    fn ImageList_ReplaceIcon(himl: HANDLE, i: i32, hicon: HICON) -> i32;
    fn ImageList_Draw(himl: HANDLE, i: i32, hdcDst: HDC, x: i32, y: i32, fStyle: u32) -> BOOL;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> HANDLE;
    fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> HANDLE;
    fn CombineRgn(dst: HANDLE, src1: HANDLE, src2: HANDLE, mode: i32) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowRgn(hWnd: HWND, hRgn: HANDLE, bRedraw: BOOL) -> i32;
    fn GetPropW(hwnd: HWND, lpString: *const u16) -> HANDLE;
    fn SetPropW(hwnd: HWND, lpString: *const u16, hData: HANDLE) -> BOOL;
    fn RemovePropW(hwnd: HWND, lpString: *const u16) -> HANDLE;
    fn DestroyIcon(hIcon: HICON) -> BOOL;
    fn DrawIconEx(
        hdc: HDC,
        xLeft: i32,
        yTop: i32,
        hIcon: HICON,
        cxWidth: i32,
        cyHeight: i32,
        istepIfAniCur: u32,
        hbrFlickerFreeDraw: HBRUSH,
        diFlags: u32,
    ) -> BOOL;
    fn SetTimer(hWnd: HWND, nIDEvent: usize, uElapse: u32, lpTimerFunc: Option<unsafe extern "system" fn(HWND, u32, usize, u32)>) -> usize;
    fn KillTimer(hWnd: HWND, uIDEvent: usize) -> BOOL;
    fn GetWindowDC(hWnd: HWND) -> HDC;
    fn SetProcessDpiAwarenessContext(value: isize) -> BOOL;
    fn GetDpiForSystem() -> u32;
    fn IsDialogMessageW(hDlg: HWND, lpMsg: *mut MSG) -> BOOL;
    fn GetNextDlgTabItem(hDlg: HWND, hCtl: HWND, bPrevious: BOOL) -> HWND;
}

#[link(name = "gdi32")]
extern "system" {
    fn Rectangle(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32) -> BOOL;
}

#[repr(C)]
struct SHFILEINFOW {
    hIcon: HICON,
    iIcon: i32,
    dwAttributes: u32,
    szDisplayName: [u16; 260],
    szTypeName: [u16; 80],
}

#[link(name = "shell32")]
extern "system" {
    fn SHGetFileInfoW(
        pszPath: *const u16,
        dwFileAttributes: u32,
        psfi: *mut SHFILEINFOW,
        cbFileInfo: u32,
        uFlags: u32,
    ) -> usize;
}

#[link(name = "kernel32")]
extern "system" {
    fn QueryFullProcessImageNameW(
        hProcess: HANDLE,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> BOOL;
    fn SearchPathW(
        lpPath: *const u16,
        lpFileName: *const u16,
        lpExtension: *const u16,
        nBufferLength: u32,
        lpBuffer: *mut u16,
        lpFilePart: *mut *mut u16,
    ) -> u32;
}

#[repr(C)]
struct NMCUSTOMDRAW {
    hdr: NMHDR,
    dwDrawStage: u32,
    hdc: HDC,
    rc: RECT,
    dwItemSpec: usize,
    uItemState: u32,
    lItemlParam: LPARAM,
}

#[repr(C)]
struct HDITEMW {
    mask: u32,
    cxy: i32,
    pszText: *mut u16,
    hbm: HBITMAP,
    cchTextMax: i32,
    fmt: i32,
    lParam: LPARAM,
    iImage: i32,
    iOrder: i32,
    type_: u32,
    pvFilter: *mut c_void_raw,
    state: u32,
}
impl Default for HDITEMW {
    fn default() -> Self { unsafe { std::mem::zeroed() } }
}

// DrawText flags
const DT_LEFT_FLAG: u32 = 0;
const DT_CENTER_FLAG: u32 = 1;
const DT_VCENTER_FLAG: u32 = 4;
const DT_SINGLELINE_FLAG: u32 = 0x20;
const DT_CALCRECT_FLAG: u32 = 0x0000_0400;
const DT_WORDBREAK_FLAG: u32 = 0x0000_0010;

const SWP_NOACTIVATE_FLAG: u32 = 0x0010;

// Windows Control Class Names
const WC_BUTTON_S: &str = "BUTTON";
const WC_EDIT_S: &str = "EDIT";
const WC_LISTVIEW_S: &str = "SysListView32";
const WC_COMBOBOX_S: &str = "COMBOBOX";

const CBS_DROPDOWNLIST_C: u32 = 0x0003;
#[allow(dead_code)]
const BS_CHECKBOX_C: u32 = 0x0002;
const ES_NUMBER_C: u32 = 0x2000;
const ES_AUTOHSCROLL_C: u32 = 0x0080;
const LVS_REPORT_C: u32 = 0x0001;
const LVS_SINGLESEL_C: u32 = 0x0004;
const LVS_SHOWSELALWAYS_C: u32 = 0x0008;
const LVS_OWNERDRAWFIXED_C: u32 = 0x0400;
const LVS_EX_FULLROWSELECT_E: u32 = 0x00000020;
const LVS_EX_DOUBLEBUFFER_E: u32 = 0x00010000;
const LVIF_TEXT_C: u32 = 0x0001;
#[allow(dead_code)]
const LVIF_STATE_C: u32 = 0x0008;
const LVCF_TEXT_C: u32 = 0x0004;
const LVCF_WIDTH_C: u32 = 0x0002;
const LVCF_SUBITEM_C: u32 = 0x0008;
const LVNI_SELECTED_C: u32 = 0x0002;

// Control IDs - Main Window
const IDC_PROC_EDIT: i32 = 101;
const IDC_SELECT_BTN: i32 = 102;
const IDC_LIST: i32 = 110;
const IDC_ADD: i32 = 111;
const IDC_CLEAR: i32 = 112;
const IDC_MAIN_METHOD: i32 = 113;
const IDC_SETTINGS: i32 = 120;
const IDC_ABOUT: i32 = 121;
const IDC_INJECT: i32 = 122;
const IDC_DEBUG_POPUPS: i32 = 123;

// Control IDs - Process Picker Dialog
const IDC_PROC_FILTER: i32 = 201;
const IDC_PROC_WINDOWED: i32 = 202;
const IDC_PROC_LIST: i32 = 203;

// Control IDs - Settings Dialog
const IDC_SET_AUTO: i32 = 302;
const IDC_SET_CLOSE: i32 = 303;
const IDC_SET_STEALTH: i32 = 304;
const IDC_SET_ERASE: i32 = 305;
const IDC_SET_HIDE: i32 = 306;
const IDC_SET_HDBG: i32 = 307;
const IDC_DELAY: i32 = 308;
const IDC_DELAY_BETWEEN: i32 = 309;
const IDC_SCRAMBLE_BTN: i32 = 310;
const IDC_ADV_BTN: i32 = 311;
const IDC_SCALE_COMBO: i32 = 312;
const IDC_SET_PERPROC: i32 = 315;
const IDC_SET_WAIT_WINDOW: i32 = 316;
const IDC_SET_RETRY: i32 = 317;
const IDC_SET_CHILDREN: i32 = 318;
const IDC_SET_CHILD_FILTER: i32 = 319;
const IDC_SET_TITLE_FILTER: i32 = 320;
const IDC_SET_LOGONLY: i32 = 321;

// Control IDs - Per-file options dialog
const IDC_FOPT_METHOD: i32 = 501;
const IDC_FOPT_WAIT: i32 = 502;
const IDC_FOPT_AFTER: i32 = 503;
const IDC_FOPT_IGNORE: i32 = 504;
const IDC_FOPT_REQUIRE: i32 = 505;
const IDC_FOPT_REQUIRE_WINDOW: i32 = 506;
const IDC_FOPT_PROCESS: i32 = 507;
const IDC_FOPT_DELAY: i32 = 510;
const IDC_LOG_PANEL: i32 = 130;
#[allow(dead_code)]
const IDC_LOG_COPY: i32 = 131;
#[allow(dead_code)]
const IDC_LOG_CLEAR: i32 = 132;

// Log panel
static LOG_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const LOG_LINE_H: i32 = 20;
const MAX_LOG_LINES: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
}

fn log_level_for(text: &str) -> LogLevel {
    if text.starts_with("[+]") {
        LogLevel::Success
    } else if text.starts_with("[!]") {
        LogLevel::Error
    } else if text.starts_with("[-]") {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

// Dark Obsidian Palette
const C_BG: COLORREF = 0x001E1E1E;              // #1e1e1e - Deep Dark Canvas
const C_PANEL: COLORREF = 0x002A2827;           // #27282a - Charcoal Card Surface
const C_PANEL_ALT: COLORREF = 0x00272523;       // #232527 - Alternating Row Surface
const C_PANEL_BORDER: COLORREF = 0x003C3C3C;    // #3c3c3c - Crisp Panel Stroke
#[allow(dead_code)]
const C_SELECTION: COLORREF = 0x007A4F2F;       // #2f4f7a - Log text selection
const C_INPUT_BG: COLORREF = 0x00232120;        // #202123 - Form Input Background
const C_INPUT_BORDER: COLORREF = 0x0044403D;    // #3d4044 - Input Border Stroke
const C_ROW_SEL: COLORREF = 0x003D3835;         // #35383d - Selected row

// Porcelain Accent (Primary Action CTA)
const C_WHITE_CTA_BG: COLORREF = 0x00FDFBFB;    // #fbfbfd - Porcelain White CTA
#[allow(dead_code)]
const C_WHITE_CTA_HOVER: COLORREF = 0x00ECD9D9; // #d9d9ec - Lavender tinted white hover
#[allow(dead_code)]
const C_WHITE_CTA_ACTIVE: COLORREF = 0x00DBC5C5;// #c5c5db - Active state
const C_WHITE_CTA_TEXT: COLORREF = 0x001E1E1E;  // #1e1e1e - Dark Ink Text on CTA

// Secondary Slate Buttons
#[allow(dead_code)]
const C_SLATE_BG: COLORREF = 0x00403E3D;        // #3d3e40 - Slate Button Surface
#[allow(dead_code)]
const C_SLATE_HOVER: COLORREF = 0x00494949;     // #494949 - Slate Hover
#[allow(dead_code)]
const C_SLATE_ACTIVE: COLORREF = 0x00333333;    // #333333 - Slate Active
const C_SLATE_BORDER: COLORREF = 0x00565656;    // #565656 - Slate 1px Border
const C_SLATE_TEXT: COLORREF = 0x00FDFBFB;      // #fbfbfd - Crisp White Text

// Typography Colors
const C_TEXT_HEAD: COLORREF = 0x00A0A0A0;       // #a0a0a0 - Card Section Header Muted
const C_TEXT_BODY: COLORREF = 0x00FDFBFB;       // #fbfbfd - Standard White Text
const C_TEXT_MUTED: COLORREF = 0x0078716E;      // #6e7178 - Subtext / Path Gray

pub struct Shared {
    pub hwnd: usize,
    pub target_pid: u32,
    pub process_name: String,
    pub delay: u32,
    pub delay_between: u32,
    pub auto_inject: bool,
    pub close_on_inject: bool,
    pub auto_wait_window: bool,
    pub auto_retry: bool,
    pub inject_children: bool,
    pub auto_child_filter: String,
    pub auto_window_title: String,
    /// Enabled files in injection order, each with its own delay.
    pub modules: Vec<Module>,
    pub opts: InjectOptions,
}

struct InjectDone {
    ok: bool,
    crashed: bool,
    target_gone: bool,
    user_error: bool,
    message: String,
    backend_error: String,
    report_body: Option<String>,
    report_path: Option<String>,
}

struct App {
    shared: Arc<Mutex<Shared>>,
    settings: Settings,
    hfont: HFONT,
    hfont_bold: HFONT,
    hfont_small: HFONT,
    hfont_italic: HFONT,
    hfont_btn: HFONT,
    hfont_title: HFONT,
    bg_brush: HBRUSH,
    panel_brush: HBRUSH,
    input_brush: HBRUSH,
    injecting: bool,
    proc_title: String,
    proc_sub: String,
    proc_icon: Option<HICON>,
    arch_cache: HashMap<String, &'static str>,
    picker_cache: PickerCache,
    /// Process name whose per-process profile is currently loaded
    active_profile: String,
    /// Colored activity log shown in the panel at the bottom
    log: Vec<(LogLevel, String)>,
    /// Log scroll position; 0 keeps the newest line visible
    #[allow(dead_code)]
    log_scroll: i32,
    /// Wrapped-line metrics from the last paint (used by the scrollbar input)
    #[allow(dead_code)]
    log_total: usize,
    #[allow(dead_code)]
    log_visible: usize,
    /// Wrapped lines as drawn, used by selection hit-testing
    log_wrapped: Vec<(String, LogLevel)>,
    #[allow(dead_code)]
    log_sel_anchor: Option<(usize, usize)>,
    #[allow(dead_code)]
    log_sel_end: Option<(usize, usize)>,
    #[allow(dead_code)]
    log_selecting: bool,
    #[allow(dead_code)]
    log_drag: bool,
    #[allow(dead_code)]
    log_drag_offset: i32,
}

// Icon image list and per-process lookups kept across picker openings, so a reopened picker
// only fetches icons and architectures for processes it hasn't seen before.
struct PickerCache {
    himl: HANDLE,
    himl_scale: u32,
    icons: HashMap<String, i32>,
    arch: HashMap<u32, Option<crate::process::Arch>>,
}

impl PickerCache {
    fn new() -> Self {
        PickerCache { himl: std::ptr::null_mut(), himl_scale: 0, icons: HashMap::new(), arch: HashMap::new() }
    }
}

unsafe fn get_module_handle() -> HINSTANCE {
    kernel32::GetModuleHandleW(std::ptr::null())
}

#[inline]
fn new_msg() -> MSG {
    MSG { hwnd: NULL_HANDLE, message: 0, wParam: 0, lParam: 0, time: 0, pt: POINT { x: 0, y: 0 } }
}

// One step of a message loop. Tab and Shift+Tab move focus between controls, Esc cancels
// (command 2), and Enter activates the focused button or accepts (command 1). Commands go
// through WM_COMMAND so each window's own handler decides what they mean.
unsafe fn dispatch_dialog_message(hwnd: HWND, msg: &MSG) {
    // While a dropdown list is open it takes the arrows, Enter and Esc first
    if msg.message == WM_KEYDOWN && dropdown_key(msg.wParam) {
        return;
    }
    if msg.message == WM_KEYDOWN {
        if msg.wParam == VK_ESCAPE_C {
            SendMessageW(hwnd, WM_COMMAND, 2, 0);
            return;
        }
        if msg.wParam == VK_RETURN_C {
            let focus = GetFocus();
            if !focus.is_null() && GetWindowLongPtrW(focus, GWL_STYLE_C) & 0x0F == BS_OWNERDRAW_C {
                SendMessageW(focus, BM_CLICK_C, 0, 0);
            } else {
                SendMessageW(hwnd, WM_COMMAND, 1, 0);
            }
            return;
        }
        if msg.wParam == VK_DELETE_C {
            let focus = GetFocus();
            let lv = GetDlgItem(hwnd, IDC_LIST);
            if !focus.is_null() && focus == lv {
                SendMessageW(hwnd, WM_APP_DELETE_ROW, 0, 0);
                return;
            }
        }
    }
    if IsDialogMessageW(hwnd, msg as *const MSG as *mut MSG) == 0 {
        let _ = TranslateMessage(msg);
        DispatchMessageW(msg);
    }
}

// Starts keyboard focus on the first tab stop of a modal window.
unsafe fn focus_first_control(hwnd: HWND) {
    SetFocus(GetNextDlgTabItem(hwnd, NULL_HANDLE, 0));
}

// Window-class background brush, created once. Registering each dialog class used to create
// a brush on every open that was never freed.
static CLASS_BG: std::sync::atomic::AtomicPtr<std::ffi::c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

unsafe fn class_bg_brush() -> HBRUSH {
    use std::sync::atomic::Ordering;
    let cur = CLASS_BG.load(Ordering::Relaxed);
    if !cur.is_null() {
        return cur;
    }
    let brush = gdi32::CreateSolidBrush(C_BG);
    CLASS_BG.store(brush, Ordering::Relaxed);
    brush
}

// Settings-window control colours come from the App's shared brushes instead of a new brush per message
unsafe fn app_brush(hwnd: HWND, pick: fn(&App) -> HBRUSH) -> LRESULT {
    let app_ptr = GetPropW(hwnd, wstr("AppPtr").as_ptr()) as *mut App;
    if app_ptr.is_null() {
        return 0;
    }
    pick(&*app_ptr) as LRESULT
}

// Background behind a button or checkbox. Controls inside a card sit on C_PANEL;
// footers and page-level buttons sit on C_BG.
fn surface_color(id: i32) -> COLORREF {
    let in_card = id == IDC_SELECT_BTN
        || (IDC_SET_AUTO..=IDC_SET_CHILDREN).contains(&id)
        || id == IDC_SET_PERPROC
        || id == IDC_SET_LOGONLY
        || id == IDC_SCRAMBLE_BTN
        || id == IDC_ADV_BTN
        || (400..600).contains(&id);
    if in_card { C_PANEL } else { C_BG }
}

// Width the vertical scrollbar takes up when rows overflow the list, otherwise 0.
unsafe fn list_scrollbar_w(lv: HWND) -> i32 {
    let count = SendMessageW(lv, 0x1000 + 4 /* LVM_GETITEMCOUNT */, 0, 0) as i32;
    let per_page = SendMessageW(lv, LVM_GETCOUNTPERPAGE_C, 0, 0) as i32;
    if count > per_page { GetSystemMetrics(SM_CXVSCROLL_C) } else { 0 }
}

// Scale stored on a dialog window (see the "Scale" property set by each dialog).
unsafe fn dlg_scale(hwnd: HWND) -> u32 {
    let v = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
    if v > 0 { v } else { 100 }
}


#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(lpofn: *mut OpenFileNameW64) -> BOOL;
}

// ===== Scaling Engine =====
// The process is per-monitor DPI aware (see `run`), so layout is in physical pixels.
// Scale = zoom (100 = standard; Auto = 100) applied on top of the system DPI, then the design
// factor. The layout numbers and font sizes below are written for a larger design, so the
// factor brings the default to Windows' standard size: 9pt text and normal-sized controls.
const UI_DESIGN_SCALE: u32 = 80;

pub fn resolve_scale(ui_scale: u32) -> u32 {
    let zoom = if ui_scale == 0 { 100 } else { ui_scale.clamp(75, 150) };
    let dpi = unsafe { GetDpiForSystem() }.max(96);
    zoom * dpi * UI_DESIGN_SCALE / (96 * 100)
}

/// Shrinks a dialog scale until the dialog fits the screen, so it can always be used
/// (e.g. after the UI scale was set too large on a small display).
fn fit_dialog_scale(mut scale: u32, base_w: i32, base_h: i32) -> u32 {
    let sw = unsafe { GetSystemMetrics(0) };
    let sh = unsafe { GetSystemMetrics(1) };
    while scale > 60 && (s(base_w, scale) > sw - 40 || s(base_h, scale) > sh - 90) {
        scale -= 5;
    }
    scale
}

pub fn s(val: i32, scale: u32) -> i32 {
    ((val as i64 * scale as i64 + 50) / 100) as i32
}

// Custom Checkbox Vector Drawing (Porcelain White when checked, dark slate when unchecked)
// Coverage (0..1) of a point by a stroke of half-width half_w running from a to b. The stroke ends
// are round, and the edge fades over one pixel, which is what makes it anti-aliased.
fn stroke_coverage(px: f32, py: f32, a: (f32, f32), b: (f32, f32), half_w: f32) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (vx * vx + vy * vy).max(f32::EPSILON);
    let t = (((px - a.0) * vx + (py - a.1) * vy) / len2).clamp(0.0, 1.0);
    let dx = px - (a.0 + t * vx);
    let dy = py - (a.1 + t * vy);
    (half_w + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0)
}

// Blends an anti-aliased stroke over a buffer that draw_aa_rounded_rect has filled, so the stroke
// lands on the real fill and the surface behind it.
fn draw_aa_stroke(pixels: &mut [u32], w: usize, h: usize, a: (f32, f32), b: (f32, f32), half_w: f32, ink: u32) {
    let x0 = (a.0.min(b.0) - half_w - 1.0).floor().max(0.0) as usize;
    let x1 = ((a.0.max(b.0) + half_w + 1.0).ceil().max(0.0) as usize).min(w);
    let y0 = (a.1.min(b.1) - half_w - 1.0).floor().max(0.0) as usize;
    let y1 = ((a.1.max(b.1) + half_w + 1.0).ceil().max(0.0) as usize).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let cov = stroke_coverage(x as f32 + 0.5, y as f32 + 0.5, a, b, half_w);
            if cov > 0.0 {
                let i = y * w + x;
                // colorref_to_dib swaps red and blue in both directions, so it also takes a DIB pixel back to a COLORREF
                let under = colorref_to_dib(pixels[i]);
                pixels[i] = colorref_to_dib(blend_colors(under, ink, cov));
            }
        }
    }
}

unsafe fn blit_dib(hdc: HDC, x: i32, y: i32, w: usize, h: usize, pixels: &[u32]) {
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = 40;
    bmi.bmiHeader.biWidth = w as i32;
    bmi.bmiHeader.biHeight = -(h as i32);
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;

    gdi32::StretchDIBits(
        hdc,
        x,
        y,
        w as i32,
        h as i32,
        0,
        0,
        w as i32,
        h as i32,
        pixels.as_ptr() as *const c_void_raw,
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}

// The box and its tick are built in a pixel buffer, so the corners and the tick are anti-aliased.
// GDI's RoundRect and Polyline are not, which is why the tick looked jagged. surface is the colour
// the box sits on, so its corners blend into that.
unsafe fn draw_custom_checkbox(hdc: HDC, x: i32, y: i32, size: i32, checked: bool, hovered: bool, scale: u32, surface: COLORREF) {
    let border_color = if hovered { 0x00807C78 } else { 0x00524D4A };
    let bg_color = if checked { C_WHITE_CTA_BG } else { C_INPUT_BG };
    let border = if checked { C_WHITE_CTA_BG } else { border_color };

    let w = size.max(1) as usize;
    let mut pixels = vec![0u32; w * w];
    draw_aa_rounded_rect(&mut pixels, w, w, s(R_CHECK, scale) as f32, surface, bg_color, border);

    if checked {
        // Tick is drawn in the box's own proportions: short left arm, longer right arm
        let f = size as f32;
        let half_w = (f * 0.045).max(0.9);
        draw_aa_stroke(&mut pixels, w, w, (f * 0.25, f * 0.50), (f * 0.45, f * 0.72), half_w, C_WHITE_CTA_TEXT);
        draw_aa_stroke(&mut pixels, w, w, (f * 0.45, f * 0.72), (f * 0.78, f * 0.26), half_w, C_WHITE_CTA_TEXT);
    }

    blit_dib(hdc, x, y, w, w, &pixels);
}

// Row whose "..." cell the mouse is over, or -1. Only the module list has that column.
static DOTS_HOVER_ROW: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

// Drag & drop state: the row being dragged, the row under the pointer, and whether the
// movement passed the drag threshold.
static DRAG_FROM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static DRAG_TO: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static DRAGGING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static DRAG_START: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
const LVM_SUBITEMHITTEST_ID: u32 = LVM_FIRST_ID + 57;

#[repr(C)]
#[derive(Default)]
struct LvHitTestInfo {
    pt: POINT,
    flags: u32,
    iItem: i32,
    iSubItem: i32,
    iGroup: i32,
}

// Keeps DOTS_HOVER_ROW in step with the mouse, and repaints the list when it changes
unsafe fn track_dots_hover(hwnd: HWND, msg: u32, lp: LPARAM) {
    use std::sync::atomic::Ordering::Relaxed;
    let mut row = -1;
    if msg == WM_MOUSEMOVE {
        let mut hit = LvHitTestInfo {
            pt: POINT { x: (lp & 0xFFFF) as i16 as i32, y: ((lp >> 16) & 0xFFFF) as i16 as i32 },
            ..Default::default()
        };
        SendMessageW(hwnd, LVM_SUBITEMHITTEST_ID, 0, &mut hit as *mut _ as LPARAM);
        if hit.iItem >= 0 && hit.iSubItem == LIST_ACTIONS_COL {
            row = hit.iItem;
        }
        // Asks for WM_MOUSELEAVE, so the hover clears when the pointer leaves the list
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        TrackMouseEvent(&mut tme);
    }
    if DOTS_HOVER_ROW.swap(row, Relaxed) != row {
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

// Three dots, with no box around them. They are grey at rest and brighter while the mouse is over them.
// A click on the dots opens the row menu.
unsafe fn draw_row_dots(hdc: HDC, x: i32, y: i32, w: i32, h: i32, bg: COLORREF, hot: bool) {
    if w <= 0 || h <= 0 { return; }
    let (wu, hu) = (w as usize, h as usize);
    let mut pixels = vec![colorref_to_dib(bg); wu * hu];
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let gap = (h as f32 * 0.2).max(4.0);
    let r = (h as f32 * 0.06).max(1.2);
    let ink = if hot { C_TEXT_BODY } else { C_TEXT_MUTED };
    for i in -1..=1 {
        let dx = cx + i as f32 * gap;
        draw_aa_stroke(&mut pixels, wu, hu, (dx, cy), (dx, cy), r, ink);
    }
    blit_dib(hdc, x, y, wu, hu, &pixels);
}

// ===== Clean Owner-Draw Button Subclass =====
unsafe extern "system" fn dark_header_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let orig = GetPropW(hwnd, wstr("OrigHdrProc").as_ptr()) as isize;
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut client_rc = RECT::default();
            GetClientRect(hwnd, &mut client_rc);

            let mem_dc = gdi32::CreateCompatibleDC(hdc);
            let mem_bm = gdi32::CreateCompatibleBitmap(hdc, client_rc.right, client_rc.bottom);
            let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

            let bg_brush = gdi32::CreateSolidBrush(0x00272523);
            let border_brush = gdi32::CreateSolidBrush(0x003C3C3C);
            gdi32::FillRect(mem_dc, &client_rc, bg_brush);

            let hfont = SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT;
            let old_font = if !hfont.is_null() { gdi32::SelectObject(mem_dc, hfont) } else { std::ptr::null_mut() };

            gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);
            gdi32::SetTextColor(mem_dc, 0x00A0A0A0);

            let lv = GetParent(hwnd);
            let count = SendMessageW(hwnd, 0x1200 /* HDM_GETITEMCOUNT */, 0, 0) as i32;
            for i in 0..count {
                let mut rc_item = RECT::default();
                if SendMessageW(hwnd, 0x1200 + 7 /* HDM_GETITEMRECT */, i as WPARAM, &mut rc_item as *mut _ as LPARAM) != 0 {
                    let div_rc = RECT {
                        left: rc_item.right - 1,
                        top: rc_item.top + 3,
                        right: rc_item.right,
                        bottom: rc_item.bottom - 3,
                    };
                    gdi32::FillRect(mem_dc, &div_rc, border_brush);

                    let mut text_buf = [0u16; 128];
                    let mut text_len = 0;

                    if !lv.is_null() {
                        let mut lvc = LVCOLUMNW {
                            mask: LVCF_TEXT_C,
                            pszText: text_buf.as_mut_ptr(),
                            cchTextMax: 128,
                            ..Default::default()
                        };
                        if SendMessageW(lv, 0x1000 + 95 /* LVM_GETCOLUMNW */, i as WPARAM, &mut lvc as *mut _ as LPARAM) != 0 {
                            text_len = text_buf.iter().position(|&c| c == 0).unwrap_or(0);
                        }
                    }

                    if text_len == 0 {
                        let mut item = HDITEMW::default();
                        item.mask = 0x0002;
                        item.pszText = text_buf.as_mut_ptr();
                        item.cchTextMax = 128;
                        if SendMessageW(hwnd, 0x1200 + 11 /* HDM_GETITEMW */, i as WPARAM, &mut item as *mut _ as LPARAM) != 0 {
                            text_len = text_buf.iter().position(|&c| c == 0).unwrap_or(0);
                        }
                    }

                    if text_len > 0 {
                        let mut txt_rc = RECT {
                            left: rc_item.left + 10,
                            top: rc_item.top,
                            right: rc_item.right - 8,
                            bottom: rc_item.bottom,
                        };
                        gdi32::DrawTextW(mem_dc, text_buf.as_ptr(), text_len as i32, &mut txt_rc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
                    }
                }
            }

            let btm_rc = RECT { left: 0, top: client_rc.bottom - 1, right: client_rc.right, bottom: client_rc.bottom };
            gdi32::FillRect(mem_dc, &btm_rc, border_brush);

            if !old_font.is_null() { gdi32::SelectObject(mem_dc, old_font); }

            gdi32::BitBlt(hdc, 0, 0, client_rc.right, client_rc.bottom, mem_dc, 0, 0, SRCCOPY);
            gdi32::SelectObject(mem_dc, old_bm);
            gdi32::DeleteObject(mem_bm);
            gdi32::DeleteDC(mem_dc);
            gdi32::DeleteObject(bg_brush);
            gdi32::DeleteObject(border_brush);

            EndPaint(hwnd, &ps);
            0
        }
        WM_NCDESTROY => {
            RemovePropW(hwnd, wstr("OrigHdrProc").as_ptr());
            CallWindowProcW(orig, hwnd, msg, wp, lp)
        }
        _ => CallWindowProcW(orig, hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn dark_listview_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let orig = GetPropW(hwnd, wstr("OrigLVProc").as_ptr()) as isize;
    if msg == WM_MOUSEMOVE || msg == WM_MOUSELEAVE {
        track_dots_hover(hwnd, msg, lp);
    }
    if msg == WM_SETCURSOR && DOTS_HOVER_ROW.load(std::sync::atomic::Ordering::Relaxed) >= 0 {
        // A hand over the dots shows they can be clicked
        SetCursor(LoadCursorW(NULL_HANDLE, 32649usize as *const u16));
        return 1;
    }
    // Drag & drop reordering: press a row, drag it over another row, release to move it
    {
        use std::sync::atomic::Ordering::Relaxed;
        if msg == WM_LBUTTONDOWN {
            let mut hit = LvHitTestInfo {
                pt: POINT { x: (lp & 0xFFFF) as i16 as i32, y: ((lp >> 16) & 0xFFFF) as i16 as i32 },
                ..Default::default()
            };
            SendMessageW(hwnd, LVM_SUBITEMHITTEST_ID, 0, &mut hit as *mut _ as LPARAM);
            DRAG_FROM.store(hit.iItem, Relaxed);
            DRAG_TO.store(hit.iItem, Relaxed);
            DRAG_START.store(lp as isize, Relaxed);
            DRAGGING.store(false, Relaxed);
        } else if msg == WM_MOUSEMOVE {
            let from = DRAG_FROM.load(Relaxed);
            if from >= 0 && (GetKeyState(0x01) as u16 & 0x8000) != 0 {
                let start = DRAG_START.load(Relaxed) as u32;
                let dx = ((lp & 0xFFFF) as i16 as i32) - ((start & 0xFFFF) as i16 as i32);
                let dy = (((lp >> 16) & 0xFFFF) as i16 as i32) - (((start >> 16) & 0xFFFF) as i16 as i32);
                if !DRAGGING.load(Relaxed) && dx * dx + dy * dy > 25 {
                    DRAGGING.store(true, Relaxed);
                }
                if DRAGGING.load(Relaxed) {
                    let mut hit = LvHitTestInfo {
                        pt: POINT { x: (lp & 0xFFFF) as i16 as i32, y: ((lp >> 16) & 0xFFFF) as i16 as i32 },
                        ..Default::default()
                    };
                    SendMessageW(hwnd, LVM_SUBITEMHITTEST_ID, 0, &mut hit as *mut _ as LPARAM);
                    if DRAG_TO.load(Relaxed) != hit.iItem {
                        DRAG_TO.store(hit.iItem, Relaxed);
                        InvalidateRect(hwnd, std::ptr::null(), 0);
                    }
                }
            }
        } else if msg == WM_LBUTTONUP {
            let from = DRAG_FROM.load(Relaxed);
            let to = DRAG_TO.load(Relaxed);
            let dragging = DRAGGING.load(Relaxed);
            DRAG_FROM.store(-1, Relaxed);
            DRAG_TO.store(-1, Relaxed);
            DRAGGING.store(false, Relaxed);
            if dragging {
                InvalidateRect(hwnd, std::ptr::null(), 0);
                if from >= 0 && to >= 0 && from != to {
                    let _ = PostMessageW(GetParent(hwnd), WM_APP_MOVE_ROW_TO, from as WPARAM, to as LPARAM);
                }
                return 0;
            }
        }
    }

    // Ctrl+Up / Ctrl+Down move the selected file, which changes when it is injected
    if msg == WM_KEYDOWN && (wp == VK_UP_C || wp == VK_DOWN_C) {
        let ctrl = (GetKeyState(0x11 /* VK_CONTROL */) as u16 & 0x8000) != 0;
        if ctrl {
            let idx = SendMessageW(hwnd, LVM_GETNEXTITEM, (-1isize) as WPARAM, LVNI_SELECTED_C as LPARAM) as i32;
            if idx >= 0 {
                let delta: i32 = if wp == VK_UP_C { -1 } else { 1 };
                let _ = PostMessageW(GetParent(hwnd), WM_APP_MOVE_ROW, idx as WPARAM, delta as LPARAM);
            }
            return 0;
        }
    }
    // Delete key removes the selected module(s) from the list
    if msg == WM_KEYDOWN && wp == VK_DELETE_C {
        let parent = GetParent(hwnd);
        if !parent.is_null() {
            let _ = PostMessageW(parent, WM_APP_DELETE_ROW, 0, 0);
        }
        return 0;
    }
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let res = CallWindowProcW(orig, hwnd, msg, wp, lp);
            let hdc = GetDC(hwnd);
            if !hdc.is_null() {
                let mut client_rc = RECT::default();
                GetClientRect(hwnd, &mut client_rc);
                // The column header is a child window across the top of the list. Only blank
                // the body below it, or an empty list paints over the header text.
                let header = SendMessageW(hwnd, LVM_GETHEADER_ID, 0, 0) as HWND;
                let mut header_rc = RECT::default();
                if !header.is_null() {
                    GetWindowRect(header, &mut header_rc);
                }
                let header_h = header_rc.bottom - header_rc.top;
                let bg_brush = gdi32::CreateSolidBrush(C_PANEL);

                // Strip to the right of the last column (columns don't always fill the list)
                let col_count = SendMessageW(header, 0x1200 /* HDM_GETITEMCOUNT */, 0, 0) as i32;
                let mut cols_w = 0;
                for c in 0..col_count {
                    cols_w += SendMessageW(hwnd, 0x1000 + 29 /* LVM_GETCOLUMNWIDTH */, c as WPARAM, 0) as i32;
                }
                if cols_w < client_rc.right {
                    let strip_rc = RECT { left: cols_w, top: header_h, right: client_rc.right, bottom: client_rc.bottom };
                    gdi32::FillRect(hdc, &strip_rc, bg_brush);
                }

                // Area under the last row, or the whole body when the list is empty
                let mut body_top = header_h;
                let count = SendMessageW(hwnd, 0x1000 + 4 /* LVM_GETITEMCOUNT */, 0, 0) as i32;
                if count > 0 {
                    let mut item_rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                    if SendMessageW(hwnd, 0x1000 + 14 /* LVM_GETITEMRECT */, (count - 1) as WPARAM, &mut item_rc as *mut _ as LPARAM) != 0 {
                        body_top = body_top.max(item_rc.bottom);
                    }
                    // Gap between header and first row (about 2px). It takes the colour of the first
                    // row, so a selected first row isn't cut off from the header by a band of the
                    // list background.
                    let mut first_rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                    if SendMessageW(hwnd, 0x1000 + 14 /* LVM_GETITEMRECT */, 0, &mut first_rc as *mut _ as LPARAM) != 0 && first_rc.top > header_h {
                        let first_selected = SendMessageW(hwnd, 0x1000 + 44 /* LVM_GETITEMSTATE */, 0, 0x0002 /* LVIS_SELECTED */) & 0x0002 != 0;
                        let gap_brush = if first_selected { gdi32::CreateSolidBrush(C_ROW_SEL) } else { bg_brush };
                        let gap_rc = RECT { left: 0, top: header_h, right: client_rc.right, bottom: first_rc.top };
                        gdi32::FillRect(hdc, &gap_rc, gap_brush);
                        if first_selected {
                            let _ = gdi32::DeleteObject(gap_brush);
                        }
                    }
                }
                if body_top < client_rc.bottom {
                    let empty_rc = RECT { left: 0, top: body_top, right: client_rc.right, bottom: client_rc.bottom };
                    gdi32::FillRect(hdc, &empty_rc, bg_brush);
                }

                // Empty-state hint, centred under the column header
                if count == 0 {
                    let hint = match GetDlgCtrlID(hwnd) {
                        IDC_LIST => "Drag DLL files here or click + Add",
                        IDC_PROC_LIST => "No matching processes",
                        _ => "",
                    };
                    if !hint.is_empty() {
                        let font = SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT;
                        let old_font = gdi32::SelectObject(hdc, font);
                        gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
                        gdi32::SetTextColor(hdc, C_TEXT_MUTED);
                        let mut trc = RECT { left: client_rc.left, top: header_h, right: client_rc.right, bottom: client_rc.bottom };
                        let wh = wstr(hint);
                        gdi32::DrawTextW(hdc, wh.as_ptr(), -1, &mut trc, DT_CENTER_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
                        gdi32::SelectObject(hdc, old_font);
                    }
                }

                // Drag & drop drop indicator: a bright line on the row the file will land on
                {
                    use std::sync::atomic::Ordering::Relaxed;
                    if DRAGGING.load(Relaxed) && DRAG_TO.load(Relaxed) >= 0 {
                        let mut item_rc = RECT::default();
                        if SendMessageW(hwnd, 0x1000 + 14 /* LVM_GETITEMRECT */, DRAG_TO.load(Relaxed) as WPARAM, &mut item_rc as *mut _ as LPARAM) != 0 {
                            let line = RECT {
                                left: 0,
                                top: item_rc.top - s(1, 160),
                                right: client_rc.right,
                                bottom: item_rc.top + s(2, 160),
                            };
                            let brush = gdi32::CreateSolidBrush(C_TEXT_BODY);
                            gdi32::FillRect(hdc, &line, brush);
                            let _ = gdi32::DeleteObject(brush);
                        }
                    }
                }

                gdi32::DeleteObject(bg_brush);
                ReleaseDC(hwnd, hdc);
            }
            res
        }
        WM_NCDESTROY => {
            RemovePropW(hwnd, wstr("OrigLVProc").as_ptr());
            CallWindowProcW(orig, hwnd, msg, wp, lp)
        }
        _ => CallWindowProcW(orig, hwnd, msg, wp, lp),
    }
}

struct BtnState {
    old_proc: isize,
    hovered: bool,
    is_primary: bool,
    is_checkbox: bool,
    checked: bool,
}

static mut BTN_STATES: Option<HashMap<usize, BtnState>> = None;

#[allow(static_mut_refs)]
unsafe fn get_btn_states() -> &'static mut HashMap<usize, BtnState> {
    if BTN_STATES.is_none() {
        BTN_STATES = Some(HashMap::new());
    }
    BTN_STATES.as_mut().unwrap()
}

// Flips a custom checkbox and repaints it. False when the button is not a checkbox.
unsafe fn toggle_checkbox_state(hwnd: HWND, key: usize) -> bool {
    let states = get_btn_states();
    if let Some(st) = states.get_mut(&key) {
        if st.is_checkbox {
            st.checked = !st.checked;
            InvalidateRect(hwnd, std::ptr::null(), 0);
            return true;
        }
    }
    false
}

unsafe extern "system" fn btn_subclass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let key = hwnd as usize;
    let old_proc = {
        let states = get_btn_states();
        states.get(&key).map(|s| s.old_proc).unwrap_or(0)
    };

    match msg {
        WM_MOUSEMOVE => {
            let states = get_btn_states();
            if let Some(st) = states.get_mut(&key) {
                if !st.hovered {
                    st.hovered = true;
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut tme);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
        }
        WM_MOUSELEAVE => {
            let states = get_btn_states();
            if let Some(st) = states.get_mut(&key) {
                st.hovered = false;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
        }
        // Checkboxes toggle here; normal buttons fall through so the button proc reports BN_CLICKED
        WM_LBUTTONUP => {
            if toggle_checkbox_state(hwnd, key) {
                let cid = GetDlgCtrlID(hwnd);
                SendMessageW(GetParent(hwnd), WM_COMMAND, cid as WPARAM, hwnd as LPARAM);
                let _ = ReleaseCapture();
                let cid = GetDlgCtrlID(hwnd);
                SendMessageW(GetParent(hwnd), WM_COMMAND, cid as WPARAM, hwnd as LPARAM);
                return 0;
            }
        }
        // Handled here and not passed on: the button's default proc replays it as WM_LBUTTONUP,
        // which would toggle a second time and leave the checkbox as it was.
        BM_CLICK_C => {
            if toggle_checkbox_state(hwnd, key) {
                let cid = GetDlgCtrlID(hwnd);
                SendMessageW(GetParent(hwnd), WM_COMMAND, cid as WPARAM, hwnd as LPARAM);
                return 1;
            }
        }
        WM_NCDESTROY => {
            let states = get_btn_states();
            states.remove(&key);
        }
        _ => {}
    }

    if old_proc != 0 {
        CallWindowProcW(old_proc, hwnd, msg, wp, lp)
    } else {
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

unsafe fn subclass_button(btn: HWND, is_primary: bool) {
    let key = btn as usize;
    let old = SetWindowLongPtrW(btn, GWLP_WNDPROC, btn_subclass_proc as LONG_PTR);
    let states = get_btn_states();
    states.insert(key, BtnState {
        old_proc: old,
        hovered: false,
        is_primary,
        is_checkbox: false,
        checked: false,
    });
}

unsafe fn subclass_checkbox(btn: HWND, initial_checked: bool) {
    let key = btn as usize;
    let old = SetWindowLongPtrW(btn, GWLP_WNDPROC, btn_subclass_proc as LONG_PTR);
    let states = get_btn_states();
    states.insert(key, BtnState {
        old_proc: old,
        hovered: false,
        is_primary: false,
        is_checkbox: true,
        checked: initial_checked,
    });
}

unsafe fn get_checkbox_state(btn: HWND) -> bool {
    let key = btn as usize;
    let states = get_btn_states();
    states.get(&key).map(|s| s.checked).unwrap_or(false)
}

fn blend_colors(c1: u32, c2: u32, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let r1 = (c1 & 0xFF) as f32;
    let g1 = ((c1 >> 8) & 0xFF) as f32;
    let b1 = ((c1 >> 16) & 0xFF) as f32;

    let r2 = (c2 & 0xFF) as f32;
    let g2 = ((c2 >> 8) & 0xFF) as f32;
    let b2 = ((c2 >> 16) & 0xFF) as f32;

    let r = (r1 * (1.0 - a) + r2 * a).round() as u32;
    let g = (g1 * (1.0 - a) + g2 * a).round() as u32;
    let b = (b1 * (1.0 - a) + b2 * a).round() as u32;

    r | (g << 8) | (b << 16)
}

// GDI's RoundRect takes the width and height of the corner ellipse, which is twice the corner
// radius. Every call site passes a radius, so this doubles it. Without that every corner came out
// half as round as the palette sizes say.
unsafe fn round_rect(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32, radius_w: i32, radius_h: i32) -> BOOL {
    gdi32::RoundRect(hdc, left, top, right, bottom, radius_w * 2, radius_h * 2)
}

// COLORREF (0x00BBGGRR) to the 0x00RRGGBB layout a 32bpp DIB expects. Without this the
// red and blue channels come out swapped in every anti-aliased button and combobox.
fn colorref_to_dib(c: u32) -> u32 {
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}

fn draw_aa_rounded_rect(
    pixels: &mut [u32],
    w: usize,
    h: usize,
    radius: f32,
    bg_color: u32,
    fill_color: u32,
    border_color: u32,
) {
    let r = radius.min((w as f32) / 2.0).min((h as f32) / 2.0);
    let border_width = 1.0f32;

    for y in 0..h {
        let py = y as f32 + 0.5;
        for x in 0..w {
            let px = x as f32 + 0.5;

            let cx = if px < r { r } else if px > w as f32 - r { w as f32 - r } else { px };
            let cy = if py < r { r } else if py > h as f32 - r { h as f32 - r } else { py };

            let dx = px - cx;
            let dy = py - cy;
            let dist_to_corner = (dx * dx + dy * dy).sqrt();

            let dist_from_outer = if px < r || px > w as f32 - r || py < r || py > h as f32 - r {
                r - dist_to_corner
            } else {
                let d_left = px;
                let d_right = w as f32 - px;
                let d_top = py;
                let d_bot = h as f32 - py;
                d_left.min(d_right).min(d_top).min(d_bot)
            };

            let outer_alpha = (dist_from_outer + 0.5).clamp(0.0, 1.0);

            if outer_alpha <= 0.0 {
                pixels[y * w + x] = colorref_to_dib(bg_color);
            } else {
                let dist_from_inner = dist_from_outer - border_width;
                let inner_alpha = (dist_from_inner + 0.5).clamp(0.0, 1.0);

                let btn_pixel = blend_colors(border_color, fill_color, inner_alpha);
                let final_pixel = blend_colors(bg_color, btn_pixel, outer_alpha);
                pixels[y * w + x] = colorref_to_dib(final_pixel);
            }
        }
    }
}

struct ComboboxState {
    old_proc: LONG_PTR,
    hovered: bool,
    scale: u32,
    bg_parent: COLORREF,
    font: HFONT,
}

static mut CB_STATES: Option<HashMap<usize, ComboboxState>> = None;

#[allow(static_mut_refs)]
unsafe fn get_cb_states() -> &'static mut HashMap<usize, ComboboxState> {
    if CB_STATES.is_none() {
        CB_STATES = Some(HashMap::new());
    }
    CB_STATES.as_mut().unwrap()
}

unsafe fn paint_combobox_dc(hwnd: HWND, hdc: HDC, scale: u32, bg_parent: COLORREF, hovered: bool) {
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let w = (rc.right - rc.left) as usize;
    let h = (rc.bottom - rc.top) as usize;
    if w == 0 || h == 0 { return; }

    let is_open = dropdown_open_for(hwnd);
    let active_hover = hovered || is_open;

    let fill_clr = if is_open {
        0x0032302E
    } else if active_hover {
        0x002B2928
    } else {
        C_INPUT_BG // 0x00232120
    };
    let no_border = !GetPropW(hwnd, wstr("NoBorder").as_ptr()).is_null();
    let border_clr = if no_border {
        fill_clr
    } else if is_open {
        0x00787570
    } else if active_hover {
        C_SLATE_BORDER // 0x00565656
    } else {
        C_PANEL_BORDER // 0x003C3C3C
    };

    let radius = s(6, scale) as f32;
    let mut pixels = vec![0u32; w * h];
    draw_aa_rounded_rect(&mut pixels, w, h, radius, bg_parent, fill_clr, border_clr);

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = 40;
    bmi.bmiHeader.biWidth = w as i32;
    bmi.bmiHeader.biHeight = -(h as i32);
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;

    // Double buffer memory DC to ensure completely flicker-free, tear-free rendering
    let mem_dc = gdi32::CreateCompatibleDC(hdc);
    let mem_bm = gdi32::CreateCompatibleBitmap(hdc, w as i32, h as i32);
    let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

    gdi32::StretchDIBits(
        mem_dc,
        0, 0, w as i32, h as i32,
        0, 0, w as i32, h as i32,
        pixels.as_ptr() as *const c_void_raw,
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );

    // Draw Text into mem_dc
    let cur = SendMessageW(hwnd, CB_GETCURSEL, 0, 0) as i32;
    let mut buf = [0u16; 256];
    let mut len = 0;
    if cur >= 0 {
        let r = SendMessageW(hwnd, CB_GETLBTEXT, cur as WPARAM, buf.as_mut_ptr() as LPARAM);
        if r >= 0 {
            len = buf.iter().position(|&c| c == 0).unwrap_or(r as usize);
        }
    }

    let font = {
        let states = get_cb_states();
        states.get(&(hwnd as usize)).map(|s| s.font).unwrap_or(NULL_HANDLE)
    };
    let hfont = if !font.is_null() { font } else { SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT };
    let old_font = if !hfont.is_null() { gdi32::SelectObject(mem_dc, hfont) } else { NULL_HANDLE };
    let old_bk = gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);
    gdi32::SetTextColor(mem_dc, C_TEXT_BODY);

    let mut txt_rc = RECT {
        left: rc.left + s(10, scale),
        top: rc.top,
        right: rc.right - s(24, scale),
        bottom: rc.bottom,
    };
    gdi32::DrawTextW(mem_dc, buf.as_ptr(), len as i32, &mut txt_rc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG | 0x00008000 /* DT_END_ELLIPSIS */);

    // Draw Integrated Sleek Chevron Arrow into mem_dc
    let arrow_x = rc.right - s(14, scale);
    let arrow_y = (rc.top + rc.bottom) / 2;
    let arrow_color = if active_hover { 0x00E0E0E0 } else { 0x00A0A0A0 };
    let arrow_pen = gdi32::CreatePen(gdi32::PS_SOLID, s(2, scale).max(1), arrow_color);
    let old_pen = gdi32::SelectObject(mem_dc, arrow_pen);

    let d_x = s(4, scale).max(3);
    let d_y = s(2, scale).max(2);
    if is_open {
        // Up arrow ^
        gdi32::MoveToEx(mem_dc, arrow_x - d_x, arrow_y + d_y, std::ptr::null_mut());
        gdi32::LineTo(mem_dc, arrow_x, arrow_y - d_y);
        gdi32::LineTo(mem_dc, arrow_x + d_x + 1, arrow_y + d_y + 1);
    } else {
        // Down arrow v
        gdi32::MoveToEx(mem_dc, arrow_x - d_x, arrow_y - d_y, std::ptr::null_mut());
        gdi32::LineTo(mem_dc, arrow_x, arrow_y + d_y);
        gdi32::LineTo(mem_dc, arrow_x + d_x + 1, arrow_y - d_y - 1);
    }

    gdi32::SelectObject(mem_dc, old_pen);
    let _ = gdi32::DeleteObject(arrow_pen);

    gdi32::SetBkMode(mem_dc, old_bk);
    if !old_font.is_null() {
        gdi32::SelectObject(mem_dc, old_font);
    }

    // Atomic blit to destination DC
    gdi32::BitBlt(hdc, 0, 0, w as i32, h as i32, mem_dc, 0, 0, SRCCOPY);

    gdi32::SelectObject(mem_dc, old_bm);
    let _ = gdi32::DeleteObject(mem_bm);
    let _ = gdi32::DeleteDC(mem_dc);
}

unsafe extern "system" fn combobox_subclass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let key = hwnd as usize;
    let (old_proc, hovered, scale, bg_parent) = {
        let states = get_cb_states();
        if let Some(s) = states.get(&key) {
            (s.old_proc, s.hovered, s.scale, s.bg_parent)
        } else {
            (0, false, 100, C_BG)
        }
    };
    if old_proc == 0 {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }

    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_combobox_dc(hwnd, hdc, scale, bg_parent, hovered);
            EndPaint(hwnd, &ps);
            return 0;
        }
        0x0318 /* WM_PRINTCLIENT */ => {
            let hdc = wp as HDC;
            paint_combobox_dc(hwnd, hdc, scale, bg_parent, hovered);
            return 0;
        }
        0x0085 /* WM_NCPAINT */ => return 0,
        WM_MOUSEMOVE => {
            let states = get_cb_states();
            if let Some(st) = states.get_mut(&key) {
                if !st.hovered {
                    st.hovered = true;
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut tme);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            return 0; // Prevent native ComboBox procedure from receiving mousemove and drawing native button
        }
        WM_MOUSELEAVE => {
            let states = get_cb_states();
            if let Some(st) = states.get_mut(&key) {
                st.hovered = false;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            return 0;
        }
        WM_SETCURSOR => {
            let cur = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
            SetCursor(cur);
            return 1;
        }
        WM_LBUTTONDOWN => {
            if GetFocus() != hwnd {
                SetFocus(hwnd);
            }
            if dropdown_open_for(hwnd) {
                close_dropdown();
            } else {
                open_dropdown(hwnd, scale);
            }
            return 0;
        }
        WM_LBUTTONUP => {
            return 0;
        }
        0x014F /* CB_SHOWDROPDOWN */ => {
            // Alt+Down and F4 reach here too; they open the same popup instead of the native list
            if wp != 0 {
                open_dropdown(hwnd, scale);
            } else if dropdown_open_for(hwnd) {
                close_dropdown();
            }
            return 0;
        }
        0x0007 /* WM_SETFOCUS */
        | 0x0008 /* WM_KILLFOCUS */
        | 0x014E /* CB_SETCURSEL */
        | 0x0215 /* WM_CAPTURECHANGED */
        | 0x001F /* WM_CANCELMODE */
        | 0x0128 /* WM_UPDATEUISTATE */ => {
            let res = CallWindowProcW(old_proc, hwnd, msg, wp, lp);
            // Immediately overwrite any native drop button or separator stripe synchronously before screen presentation
            let hdc = GetDC(hwnd);
            if !hdc.is_null() {
                paint_combobox_dc(hwnd, hdc, scale, bg_parent, hovered);
                ReleaseDC(hwnd, hdc);
            }
            ValidateRect(hwnd, std::ptr::null());
            return res;
        }
        WM_NCDESTROY => {
            let states = get_cb_states();
            states.remove(&key);
        }
        _ => {}
    }

    CallWindowProcW(old_proc, hwnd, msg, wp, lp)
}

// ===== Dropdown list =====
// The native combobox list drew an outline first, filled in row by row, and made the closed box
// repaint as it opened. This popup paints the whole list in one pass and is shown already drawn.
// It captures the mouse while open: clicking outside, Esc, or Enter closes it.
const WS_EX_TOOLWINDOW_C: u32 = 0x0000_0080;
const SWP_SHOWWINDOW_C: u32 = 0x0040;
const HWND_TOPMOST_C: HWND = -1isize as HWND;
const SM_CYSCREEN_C: i32 = 1;
const VK_UP_C: usize = 0x26;
const VK_DOWN_C: usize = 0x28;
const CBN_SELCHANGE_C: u32 = 1;
const NULL_PEN_C: i32 = 8;
const DT_END_ELLIPSIS_C: u32 = 0x0000_8000;
const WM_RBUTTONDOWN_C: u32 = 0x0204;
const WM_CAPTURECHANGED_C: u32 = 0x0215;

type AtomicHandle = std::sync::atomic::AtomicPtr<std::ffi::c_void>;

// The popup window (created on first use) and the combobox whose list it shows (null if none)
static DROP_WND: AtomicHandle = AtomicHandle::new(std::ptr::null_mut());
static DROP_OWNER: AtomicHandle = AtomicHandle::new(std::ptr::null_mut());

struct DropdownState {
    owner: HWND,
    items: Vec<String>,
    current: i32,
    hover: i32,
    item_h: i32,
    pad: i32,
    scale: u32,
    font: HFONT,
}

fn drop_owner() -> HWND {
    DROP_OWNER.load(std::sync::atomic::Ordering::Relaxed)
}

unsafe fn dropdown_open_for(owner: HWND) -> bool {
    drop_owner() == owner
}

unsafe fn dropdown_state() -> Option<&'static mut DropdownState> {
    let popup = DROP_WND.load(std::sync::atomic::Ordering::Relaxed);
    if popup.is_null() {
        return None;
    }
    let st = GetWindowLongPtrW(popup, GWLP_USERDATA) as *mut DropdownState;
    if st.is_null() { None } else { Some(&mut *st) }
}

unsafe fn dropdown_window() -> HWND {
    let existing = DROP_WND.load(std::sync::atomic::Ordering::Relaxed);
    if !existing.is_null() {
        return existing;
    }
    let hinst = get_module_handle();
    let cls = wstr("AppComboPopup");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(dropdown_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let state = Box::new(DropdownState {
        owner: NULL_HANDLE,
        items: Vec::new(),
        current: -1,
        hover: -1,
        item_h: 0,
        pad: 0,
        scale: 100,
        font: NULL_HANDLE,
    });
    let popup = CreateWindowExW(
        WS_EX_TOOLWINDOW_C | WS_EX_TOPMOST as u32, cls.as_ptr(), wstr("").as_ptr(), WS_POPUP,
        0, 0, 1, 1, NULL_HANDLE, NULL_HANDLE, hinst, std::ptr::null_mut(),
    );
    SetWindowLongPtrW(popup, GWLP_USERDATA, Box::into_raw(state) as LONG_PTR);
    DROP_WND.store(popup, std::sync::atomic::Ordering::Relaxed);
    popup
}

unsafe fn open_dropdown(owner: HWND, scale: u32) {
    close_dropdown();
    let popup = dropdown_window();
    let st = match dropdown_state() {
        Some(st) => st,
        None => return,
    };

    let count = SendMessageW(owner, CB_GETCOUNT, 0, 0) as i32;
    st.items = (0..count.max(0))
        .map(|i| {
            let mut buf = [0u16; 256];
            let r = SendMessageW(owner, CB_GETLBTEXT, i as WPARAM, buf.as_mut_ptr() as LPARAM);
            let len = if r >= 0 { buf.iter().position(|&c| c == 0).unwrap_or(0) } else { 0 };
            String::from_utf16_lossy(&buf[..len])
        })
        .collect();
    st.owner = owner;
    st.current = SendMessageW(owner, CB_GETCURSEL, 0, 0) as i32;
    st.hover = st.current;
    st.scale = scale;
    st.font = SendMessageW(owner, WM_GETFONT, 0, 0) as HFONT;
    st.item_h = s(30, scale);
    st.pad = s(4, scale);

    let mut rc = RECT::default();
    GetWindowRect(owner, &mut rc);
    let width = rc.right - rc.left;
    let height = st.pad * 2 + st.item_h * count.max(1);
    let gap = s(4, scale);
    let mut top = rc.bottom + gap;
    if top + height > GetSystemMetrics(SM_CYSCREEN_C) {
        top = rc.top - gap - height; // not enough room below, so open upwards
    }

    DROP_OWNER.store(owner, std::sync::atomic::Ordering::Relaxed);
    round_window(popup, width, height, s(R_INPUT, scale));
    SetWindowPos(popup, HWND_TOPMOST_C, rc.left, top, width, height, SWP_NOACTIVATE_FLAG | SWP_SHOWWINDOW_C);
    // Paint now, so the first frame on screen is already the complete list
    UpdateWindow(popup);
    SetCapture(popup);
    InvalidateRect(owner, std::ptr::null(), 0);
}

unsafe fn close_dropdown() {
    let owner = DROP_OWNER.swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Relaxed);
    if owner.is_null() {
        return;
    }
    // Releasing capture sends WM_CAPTURECHANGED back here, which finds the owner already cleared
    ReleaseCapture();
    ShowWindow(DROP_WND.load(std::sync::atomic::Ordering::Relaxed), SW_HIDE);
    InvalidateRect(owner, std::ptr::null(), 0);
}

// Picks a row (or closes, for -1). Sends the same CBN_SELCHANGE the native combobox would,
// so the parent window's handler runs unchanged.
unsafe fn commit_dropdown(index: i32) {
    let st = match dropdown_state() {
        Some(st) => st,
        None => return,
    };
    let owner = st.owner;
    let changed = index >= 0 && index != st.current;
    close_dropdown();
    if changed {
        SendMessageW(owner, CB_SETCURSEL, index as WPARAM, 0);
        let id = GetDlgCtrlID(owner) as u32 & 0xFFFF;
        SendMessageW(GetParent(owner), WM_COMMAND, ((CBN_SELCHANGE_C << 16) | id) as WPARAM, owner as LPARAM);
    }
}

// Keys while the list is open. Arrows move the highlight, Enter picks it, Esc closes it.
// Returns false when the key isn't ours, so normal dialog handling carries on.
unsafe fn dropdown_key(vk: usize) -> bool {
    if drop_owner().is_null() {
        return false;
    }
    let st = match dropdown_state() {
        Some(st) => st,
        None => return false,
    };
    let count = st.items.len() as i32;
    match vk {
        VK_ESCAPE_C => close_dropdown(),
        VK_RETURN_C => commit_dropdown(st.hover),
        VK_UP_C | VK_DOWN_C => {
            if count > 0 {
                let step = if vk == VK_UP_C { -1 } else { 1 };
                st.hover = (st.hover + step).clamp(0, count - 1);
                InvalidateRect(DROP_WND.load(std::sync::atomic::Ordering::Relaxed), std::ptr::null(), 0);
            }
        }
        _ => return false,
    }
    true
}

// Whether a mouse position (client coordinates in lParam) is inside the popup, and the row under it (-1 if none)
unsafe fn dropdown_hit(hwnd: HWND, lp: LPARAM, st: &DropdownState) -> (bool, i32) {
    let x = (lp & 0xFFFF) as i16 as i32;
    let y = ((lp >> 16) & 0xFFFF) as i16 as i32;
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    if x < 0 || y < 0 || x >= rc.right || y >= rc.bottom {
        return (false, -1);
    }
    if y < st.pad {
        return (true, -1);
    }
    let row = (y - st.pad) / st.item_h;
    if row < st.items.len() as i32 { (true, row) } else { (true, -1) }
}

unsafe fn paint_dropdown(hwnd: HWND, hdc: HDC, st: &DropdownState) {
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right, rc.bottom);
    let r = s(R_INPUT, st.scale);

    let mem_dc = gdi32::CreateCompatibleDC(hdc);
    let mem_bm = gdi32::CreateCompatibleBitmap(hdc, w, h);
    let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

    // Page colour behind the rounded corners, then the panel
    let page = gdi32::CreateSolidBrush(C_BG);
    gdi32::FillRect(mem_dc, &rc, page);
    let _ = gdi32::DeleteObject(page);

    let panel_pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_INPUT_BORDER);
    let panel_brush = gdi32::CreateSolidBrush(C_INPUT_BG);
    let old_pen = gdi32::SelectObject(mem_dc, panel_pen);
    let old_brush = gdi32::SelectObject(mem_dc, panel_brush);
    round_rect(mem_dc, 0, 0, w - 1, h - 1, r, r);

    // Highlight rows have no outline, so the highlight brush is all they need
    let row_brush = gdi32::CreateSolidBrush(C_ROW_SEL);
    let accent = gdi32::CreateSolidBrush(C_WHITE_CTA_BG);
    gdi32::SelectObject(mem_dc, gdi32::GetStockObject(NULL_PEN_C));
    gdi32::SelectObject(mem_dc, row_brush);
    gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);
    let old_font = if !st.font.is_null() { gdi32::SelectObject(mem_dc, st.font) } else { NULL_HANDLE };
    gdi32::SetTextColor(mem_dc, C_TEXT_BODY);

    for (i, text) in st.items.iter().enumerate() {
        let top = st.pad + i as i32 * st.item_h;
        let row = RECT { left: st.pad, top, right: w - st.pad, bottom: top + st.item_h };
        if i as i32 == st.hover {
            round_rect(mem_dc, row.left, row.top, row.right, row.bottom, r, r);
        }
        if i as i32 == st.current {
            let bar = RECT {
                left: st.pad + s(4, st.scale),
                top: top + s(9, st.scale),
                right: st.pad + s(7, st.scale),
                bottom: top + st.item_h - s(9, st.scale),
            };
            gdi32::FillRect(mem_dc, &bar, accent);
        }
        let mut trc = RECT { left: st.pad + s(14, st.scale), top, right: row.right - s(8, st.scale), bottom: row.bottom };
        let wt = wstr(text);
        gdi32::DrawTextW(
            mem_dc, wt.as_ptr(), -1, &mut trc,
            DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG | DT_END_ELLIPSIS_C,
        );
    }

    // Put the stock objects back, so nothing we created is still selected when it's deleted
    if !old_font.is_null() {
        gdi32::SelectObject(mem_dc, old_font);
    }
    gdi32::SelectObject(mem_dc, old_brush);
    gdi32::SelectObject(mem_dc, old_pen);
    gdi32::BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
    gdi32::SelectObject(mem_dc, old_bm);
    let _ = gdi32::DeleteObject(row_brush);
    let _ = gdi32::DeleteObject(accent);
    let _ = gdi32::DeleteObject(panel_brush);
    let _ = gdi32::DeleteObject(panel_pen);
    let _ = gdi32::DeleteObject(mem_bm);
    let _ = gdi32::DeleteDC(mem_dc);
}

unsafe extern "system" fn dropdown_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = dropdown_state() {
                paint_dropdown(hwnd, hdc, st);
            }
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_MOUSEMOVE => {
            // Only a position over a row changes the highlight. Moves outside the rows (including
            // the one Windows sends right after the list opens) keep the keyboard highlight.
            if let Some(st) = dropdown_state() {
                let (inside, row) = dropdown_hit(hwnd, lp, st);
                if inside && row >= 0 && row != st.hover {
                    st.hover = row;
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            return 0;
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN_C => {
            if let Some(st) = dropdown_state() {
                let (inside, _) = dropdown_hit(hwnd, lp, st);
                if !inside {
                    close_dropdown();
                }
            }
            return 0;
        }
        WM_LBUTTONUP => {
            if let Some(st) = dropdown_state() {
                let (inside, row) = dropdown_hit(hwnd, lp, st);
                if inside && row >= 0 {
                    commit_dropdown(row);
                }
            }
            return 0;
        }
        WM_CAPTURECHANGED_C => {
            close_dropdown();
            return 0;
        }
        WM_NCDESTROY => {
            let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DropdownState;
            if !st.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(st));
            }
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

struct LboxState {
    old_proc: LONG_PTR,
    #[allow(dead_code)]
    scale: u32,
    #[allow(dead_code)]
    font: HFONT,
}

static mut LBOX_STATES: Option<HashMap<usize, LboxState>> = None;

#[allow(static_mut_refs)]
unsafe fn get_lbox_states() -> &'static mut HashMap<usize, LboxState> {
    if LBOX_STATES.is_none() {
        LBOX_STATES = Some(HashMap::new());
    }
    LBOX_STATES.as_mut().unwrap()
}

unsafe extern "system" fn lbox_subclass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let key = hwnd as usize;
    let old_proc = {
        let states = get_lbox_states();
        if let Some(s) = states.get(&key) {
            s.old_proc
        } else {
            0
        }
    };

    match msg {
        0x0085 /* WM_NCPAINT */ => {
            let hdc = GetWindowDC(hwnd);
            if !hdc.is_null() {
                let mut rc = RECT::default();
                GetWindowRect(hwnd, &mut rc);
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;
                let pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
                let brush = gdi32::GetStockObject(5 /* NULL_BRUSH */);
                let old_p = gdi32::SelectObject(hdc, pen);
                let old_b = gdi32::SelectObject(hdc, brush);
                Rectangle(hdc, 0, 0, w, h);
                gdi32::SelectObject(hdc, old_b);
                gdi32::SelectObject(hdc, old_p);
                let _ = gdi32::DeleteObject(pen);
                ReleaseDC(hwnd, hdc);
            }
            return 0;
        }
        WM_ERASEBKGND => {
            let hdc = wp as HDC;
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);
            let brush = gdi32::CreateSolidBrush(C_INPUT_BG);
            gdi32::FillRect(hdc, &rc, brush);
            let _ = gdi32::DeleteObject(brush);
            return 1;
        }
        WM_NCDESTROY => {
            let states = get_lbox_states();
            states.remove(&key);
        }
        _ => {}
    }

    if old_proc != 0 {
        CallWindowProcW(old_proc, hwnd, msg, wp, lp)
    } else {
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

unsafe fn subclass_lbox(lbox: HWND, scale: u32, font: HFONT) {
    if lbox.is_null() { return; }
    let key = lbox as usize;
    let states = get_lbox_states();
    if states.contains_key(&key) { return; }

    dwmapi::dark_title_bar(lbox);
    uxtheme::allow_dark_mode_for_window(lbox);

    let ex_style = GetWindowLongPtrW(lbox, GWL_EXSTYLE) as u32;
    SetWindowLongPtrW(lbox, GWL_EXSTYLE, (ex_style & !0x00000200) as LONG_PTR);

    let pref: u32 = 2; // DWMWCP_ROUND
    let _ = dwmapi::DwmSetWindowAttribute(lbox, 33, &pref as *const _ as *const c_void_raw, 4);
    let border_clr = C_PANEL_BORDER;
    let _ = dwmapi::DwmSetWindowAttribute(lbox, 34, &border_clr as *const _ as *const c_void_raw, 4);

    let old = SetWindowLongPtrW(lbox, GWLP_WNDPROC, lbox_subclass_proc as LONG_PTR);
    states.insert(key, LboxState {
        old_proc: old,
        scale,
        font,
    });
}

unsafe fn subclass_combobox(cb: HWND, scale: u32, bg_parent: COLORREF, font: HFONT) {
    let key = cb as usize;
    uxtheme::set_theme(cb, "");
    let old = SetWindowLongPtrW(cb, GWLP_WNDPROC, combobox_subclass_proc as LONG_PTR);
    let states = get_cb_states();
    states.insert(key, ComboboxState {
        old_proc: old,
        hovered: false,
        scale,
        bg_parent,
        font,
    });
}

static mut EDIT_STATES: Option<HashMap<usize, LONG_PTR>> = None;

#[allow(static_mut_refs)]
unsafe fn get_edit_states() -> &'static mut HashMap<usize, LONG_PTR> {
    if EDIT_STATES.is_none() {
        EDIT_STATES = Some(HashMap::new());
    }
    EDIT_STATES.as_mut().unwrap()
}

unsafe extern "system" fn edit_subclass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let key = hwnd as usize;
    let old_proc = {
        let states = get_edit_states();
        states.get(&key).copied().unwrap_or(0)
    };
    if old_proc == 0 {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }

    match msg {
        WM_PAINT => {
            let res = CallWindowProcW(old_proc, hwnd, msg, wp, lp);
            // Hint inside the picker's search box while it's empty
            if GetDlgCtrlID(hwnd) == IDC_PROC_FILTER && SendMessageW(hwnd, 0x000E /* WM_GETTEXTLENGTH */, 0, 0) == 0 {
                draw_edit_cue(hwnd, "Search by name or PID");
            }
            return res;
        }
        WM_ERASEBKGND => {
            // Completely eliminate the blank background erase pass that causes text flickering when typing
            return 1;
        }
        WM_NCDESTROY => {
            let states = get_edit_states();
            states.remove(&key);
        }
        _ => {}
    }

    CallWindowProcW(old_proc, hwnd, msg, wp, lp)
}

// Hint text inside an empty edit box. The native cue banner needs a Common Controls manifest,
// which this build doesn't have, so the hint is drawn here. It uses the edit's left margin.
unsafe fn draw_edit_cue(hwnd: HWND, text: &str) {
    let hdc = GetDC(hwnd);
    if hdc.is_null() {
        return;
    }
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    rc.left += s(4, dlg_scale(GetParent(hwnd)));
    let font = SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT;
    let old_font = if !font.is_null() { gdi32::SelectObject(hdc, font) } else { NULL_HANDLE };
    gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
    gdi32::SetTextColor(hdc, C_TEXT_MUTED);
    let wt = wstr(text);
    gdi32::DrawTextW(hdc, wt.as_ptr(), -1, &mut rc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
    if !old_font.is_null() {
        gdi32::SelectObject(hdc, old_font);
    }
    ReleaseDC(hwnd, hdc);
}

unsafe fn subclass_edit(edit: HWND) {
    if edit.is_null() { return; }
    let key = edit as usize;
    let states = get_edit_states();
    if states.contains_key(&key) { return; }
    let old = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_subclass_proc as LONG_PTR);
    states.insert(key, old);
}

// A combo box is always as tall as its closed field plus the edge the system adds around it. The item
// height is shortened by that edge, so the closed box comes out the same height as a button (s(32)).
unsafe fn fit_combo_closed_height(cb: HWND, scale: u32) {
    SendMessageW(cb, CB_SETITEMHEIGHT, (-1isize) as WPARAM, s(32, scale) as LPARAM);
    let mut rc = RECT::default();
    GetWindowRect(cb, &mut rc);
    let extra = (rc.bottom - rc.top) - s(32, scale);
    SendMessageW(cb, CB_SETITEMHEIGHT, (-1isize) as WPARAM, (s(32, scale) - extra).max(1) as LPARAM);
}

unsafe fn update_combobox_scale(cb: HWND, scale: u32, font: HFONT) {
    let key = cb as usize;
    let states = get_cb_states();
    if let Some(st) = states.get_mut(&key) {
        st.scale = scale;
        st.font = font;
    }
}

// Render Owner-Draw ComboBox dropdown item
unsafe fn draw_owner_combobox(dis: &DRAWITEMSTRUCT, scale: u32, font: HFONT) {
    // If this is the combobox static/edit field (closed state), skip it completely.
    // The closed combobox is rendered across its entire width by combobox_subclass_proc.
    if (dis.itemID as i32) < 0 || (dis.itemState & 0x1000 /* ODS_COMBOBOXEDIT */) != 0 {
        return;
    }

    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let is_selected = (dis.itemState & ODS_SELECTED) != 0;

    let bg_color = if is_selected {
        C_ROW_SEL
    } else {
        C_INPUT_BG
    };

    let brush = gdi32::CreateSolidBrush(bg_color);
    gdi32::FillRect(hdc, &rc, brush);
    let _ = gdi32::DeleteObject(brush);

    // Get item text
    let mut text_buf = [0u16; 128];
    let mut text_len = 0;
    let idx = dis.itemID as i32;

    if idx >= 0 {
        let r = SendMessageW(dis.hwndItem, CB_GETLBTEXT, idx as WPARAM, text_buf.as_mut_ptr() as LPARAM);
        if r >= 0 {
            text_len = text_buf.iter().position(|&c| c == 0).unwrap_or(r as usize);
        }
    }

    if text_len > 0 {
        let old_font = if !font.is_null() { gdi32::SelectObject(hdc, font) } else { NULL_HANDLE };
        let old_bk = gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
        gdi32::SetTextColor(hdc, if is_selected { 0x00FFFFFF } else { C_TEXT_BODY });
        let mut txt_rc = RECT {
            left: rc.left + s(12, scale),
            top: rc.top,
            right: rc.right - s(8, scale),
            bottom: rc.bottom,
        };
        gdi32::DrawTextW(hdc, text_buf.as_ptr(), text_len as i32, &mut txt_rc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
        gdi32::SetBkMode(hdc, old_bk);
        if !old_font.is_null() {
            gdi32::SelectObject(hdc, old_font);
        }
    }
}

unsafe fn draw_owner_button(dis: &DRAWITEMSTRUCT, scale: u32) {
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let w = (rc.right - rc.left) as usize;
    let h = (rc.bottom - rc.top) as usize;
    if w == 0 || h == 0 { return; }

    let key = dis.hwndItem as usize;
    let states = get_btn_states();
    let (hovered, is_primary, is_checkbox, checked) = if let Some(st) = states.get(&key) {
        (st.hovered, st.is_primary, st.is_checkbox, st.checked)
    } else {
        (false, false, false, false)
    };
    let is_pressed = (dis.itemState & 0x0001) != 0; // ODS_SELECTED
    let surface = surface_color(GetDlgCtrlID(dis.hwndItem));

    if is_checkbox {
        let brush = gdi32::CreateSolidBrush(surface);
        gdi32::FillRect(hdc, &rc, brush);
        let _ = gdi32::DeleteObject(brush);

        let box_size = ((rc.bottom - rc.top) * 3 / 4).clamp(s(16, scale), s(26, scale));
        let cb_y = (rc.bottom - rc.top - box_size) / 2;
        draw_custom_checkbox(hdc, 0, cb_y, box_size, checked, hovered, scale, surface);

        let mut buf = [0u16; 128];
        let len = GetWindowTextW(dis.hwndItem, buf.as_mut_ptr(), 128);
        if len > 0 {
            let mut trc = RECT {
                left: box_size + s(8, scale),
                top: rc.top,
                right: rc.right,
                bottom: rc.bottom,
            };
            gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
            gdi32::SetTextColor(hdc, if checked { C_TEXT_BODY } else { 0x00B0B0B0 });
            gdi32::DrawTextW(hdc, buf.as_ptr(), len, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
        }
        return;
    }

    // Corners are blended against the surface the button sits on, so they don't show a square
    let bg_color = surface;

    let (bg, border, text_clr) = if is_primary {
        if is_pressed {
            (0x00D8C8C8, 0x00989090, C_WHITE_CTA_TEXT)
        } else if hovered {
            (0x00ECE0E0, 0x00B8B0B0, C_WHITE_CTA_TEXT)
        } else {
            (C_WHITE_CTA_BG, 0x00D0C8C8, C_WHITE_CTA_TEXT)
        }
    } else {
        if is_pressed {
            (0x003A3837, 0x004E4C4A, C_SLATE_TEXT)
        } else if hovered {
            (0x005E5A57, 0x00787570, C_SLATE_TEXT)
        } else {
            (0x004A4846, 0x00605D59, C_SLATE_TEXT)
        }
    };

    let mut pixels = vec![0u32; w * h];
    let radius = s(R_BUTTON, scale) as f32;
    draw_aa_rounded_rect(&mut pixels, w, h, radius, bg_color, bg, border);

    let mut buf = [0u16; 128];
    let len = GetWindowTextW(dis.hwndItem, buf.as_mut_ptr(), 128);
    // A "+ " or "- " prefix is drawn as an anti-aliased plus or minus. GDI text draws them jagged at this size.
    let has_plus = len >= 2 && (buf[0] == '+' as u16 || buf[0] == '-' as u16) && buf[1] == ' ' as u16;
    let (label_ptr, label_len) = if has_plus { (buf.as_ptr().add(2), len - 2) } else { (buf.as_ptr(), len) };
    let press_dy = if is_pressed { 1.0 } else { 0.0 };
    let mut label_left = rc.left;
    if has_plus {
        let plus = (h as f32 * 0.34).round();
        let gap = s(6, scale) as f32;
        let mut calc = RECT::default();
        gdi32::DrawTextW(hdc, label_ptr, label_len, &mut calc, DT_LEFT_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG | DT_CALCRECT_FLAG);
        let text_w = (calc.right - calc.left) as f32;
        // Plus and label are centred together, so the gap between them stays fixed
        let start = (w as f32 - (plus + gap + text_w)) / 2.0;
        let cy = h as f32 / 2.0 + press_dy;
        let mid = start + plus / 2.0;
        let half_w = (plus * 0.09).max(1.0);
        draw_aa_stroke(&mut pixels, w, h, (start, cy), (start + plus, cy), half_w, text_clr);
        // The minus is the horizontal stroke alone; the plus adds the vertical one
        if buf[0] == '+' as u16 {
            draw_aa_stroke(&mut pixels, w, h, (mid, cy - plus / 2.0), (mid, cy + plus / 2.0), half_w, text_clr);
        }
        label_left = rc.left + (start + plus + gap).round() as i32;
    }

    blit_dib(hdc, rc.left, rc.top, w, h, &pixels);

    if len > 0 {
        let mut trc = rc;
        trc.left = label_left;
        if is_pressed {
            trc.top += 1;
            trc.bottom += 1;
        }
        let align = if has_plus { DT_LEFT_FLAG } else { DT_CENTER_FLAG };
        gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
        gdi32::SetTextColor(hdc, text_clr);
        gdi32::DrawTextW(hdc, label_ptr, label_len, &mut trc, align | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
    }
}

static LOGO_HICON: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Writes the embedded .ico to a temp file (LoadImage wants a path) and returns it.
unsafe fn icon_file() -> Option<std::path::PathBuf> {
    let mut buf = [0u16; MAX_PATH];
    let n = kernel32::GetTempPathW(MAX_PATH as u32, buf.as_mut_ptr());
    if n == 0 {
        return None;
    }
    let temp = String::from_utf16_lossy(&buf[..n as usize]);
    let dir = std::path::Path::new(temp.trim_end_matches('\\')).join("BlackBoxInjector");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("blackbox.ico");
    let stale = std::fs::read(&path).map(|b| b != LOGO_ICO).unwrap_or(true);
    if stale {
        let _ = std::fs::write(&path, LOGO_ICO);
    }
    Some(path)
}

unsafe fn load_embedded_icon(size: i32) -> HICON {
    let Some(path) = icon_file() else { return std::ptr::null_mut() };
    let name = wstr(&path.display().to_string());
    LoadImageW(NULL_HANDLE, name.as_ptr(), IMAGE_ICON, size, size, LR_LOADFROMFILE) as HICON
}

unsafe fn apply_app_icons(hwnd: HWND) {
    // Size the title-bar icons for the actual DPI. Sending a fixed 16px icon at 150-200%
    // makes Windows stretch it, which is what makes the logo look blurred in the title bar.
    let dpi = GetDpiForSystem().max(96) as i32;
    let small_px = (16 * dpi / 96).max(16);
    let big_px = (32 * dpi / 96).max(32);
    let big = load_embedded_icon(big_px);
    if !big.is_null() {
        SendMessageW(hwnd, WM_SETICON, 1, big as LPARAM);
    }
    let small = load_embedded_icon(small_px);
    if !small.is_null() {
        SendMessageW(hwnd, WM_SETICON, 0, small as LPARAM);
    }
}

unsafe fn logo_icon() -> HICON {
    use std::sync::atomic::Ordering;
    let cached = LOGO_HICON.load(Ordering::Relaxed);
    if !cached.is_null() {
        return cached;
    }
    let icon = load_embedded_icon(128);
    LOGO_HICON.store(icon, Ordering::Relaxed);
    icon
}

unsafe fn draw_syringe_logo(hdc: HDC, x: i32, y: i32, w: i32, h: i32) {
    let icon = logo_icon();
    if icon.is_null() {
        return;
    }
    DrawIconEx(hdc, x, y, icon, w, h, 0, NULL_HANDLE, 0x0003 /* DI_NORMAL */);
}
// Find Window Title for PID
unsafe fn find_main_window_title(pid: u32) -> String {
    unsafe extern "system" fn enum_proc(hwnd: HWND, lp: LPARAM) -> BOOL {
        let pair = &mut *(lp as *mut (u32, String));
        let mut w_pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut w_pid);
        if w_pid == pair.0 && IsWindowVisible(hwnd) != 0 {
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
            if len > 0 {
                pair.1 = from_wsz(buf.as_ptr());
                return 0;
            }
        }
        1
    }
    let mut data = (pid, String::new());
    EnumWindows(Some(enum_proc), &mut data as *mut _ as LPARAM);
    data.1
}

// Windowed desktop PIDs collector for process filtering
unsafe fn get_windowed_pids() -> std::collections::HashSet<u32> {
    unsafe extern "system" fn enum_win(w: HWND, lp: LPARAM) -> BOOL {
        if IsWindowVisible(w) != 0 {
            let mut title_buf = [0u16; 64];
            let len = GetWindowTextW(w, title_buf.as_mut_ptr(), 64);
            if len > 0 {
                let mut rc = RECT::default();
                GetWindowRect(w, &mut rc);
                if (rc.right - rc.left) > 20 && (rc.bottom - rc.top) > 20 {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(w, &mut pid);
                    if pid > 0 {
                        let set = &mut *(lp as *mut std::collections::HashSet<u32>);
                        set.insert(pid);
                    }
                }
            }
        }
        1
    }
    let mut set = std::collections::HashSet::new();
    EnumWindows(Some(enum_win), &mut set as *mut _ as LPARAM);
    set
}

// Dynamic Process Icon Extractor using Windows Shell SHGetFileInfoW
unsafe fn fetch_process_icon(pid: u32, proc_name: &str) -> Option<HICON> {
    fetch_process_icon_sized(pid, proc_name, false)
}

unsafe fn fetch_process_icon_sized(pid: u32, proc_name: &str, small: bool) -> Option<HICON> {
    fetch_icon_with_alternates(pid, proc_name, small, &[])
}

// `alternates` are other pids running the same executable, tried if `pid` can't be opened.
// Callers that already hold a process list pass it in, so no fresh snapshot is taken per icon.
unsafe fn fetch_icon_with_alternates(pid: u32, proc_name: &str, small: bool, alternates: &[u32]) -> Option<HICON> {
    let mut exe_path: Option<Vec<u16>> = None;

    // 1. If PID > 0, query image path from process handle
    if pid > 0 {
        let mut hproc = kernel32::OpenProcess(0x1000 /* PROCESS_QUERY_LIMITED_INFORMATION */, 0, pid);
        if hproc.is_null() {
            hproc = kernel32::OpenProcess(0x0400 /* PROCESS_QUERY_INFORMATION */, 0, pid);
        }
        if !hproc.is_null() {
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            if QueryFullProcessImageNameW(hproc, 0, buf.as_mut_ptr(), &mut size) != 0 && size > 0 {
                let mut path_vec = Vec::with_capacity((size + 1) as usize);
                path_vec.extend_from_slice(&buf[..size as usize]);
                path_vec.push(0);
                exe_path = Some(path_vec);
            }
            kernel32::CloseHandle(hproc);
        }
    }

    // 2. Another process running the same executable that we are allowed to open
    if exe_path.is_none() {
        for &other in alternates {
            let mut hproc = kernel32::OpenProcess(0x1000, 0, other);
            if hproc.is_null() {
                hproc = kernel32::OpenProcess(0x0400, 0, other);
            }
            if !hproc.is_null() {
                let mut buf = [0u16; 1024];
                let mut size = buf.len() as u32;
                if QueryFullProcessImageNameW(hproc, 0, buf.as_mut_ptr(), &mut size) != 0 && size > 0 {
                    let mut path_vec = Vec::with_capacity((size + 1) as usize);
                    path_vec.extend_from_slice(&buf[..size as usize]);
                    path_vec.push(0);
                    exe_path = Some(path_vec);
                }
                kernel32::CloseHandle(hproc);
            }
            if exe_path.is_some() {
                break;
            }
        }
    }

    // 3. Fallback: SearchPathW
    if exe_path.is_none() && !proc_name.is_empty() {
        let name_clean = if proc_name.ends_with(".exe") { proc_name.to_string() } else { format!("{}.exe", proc_name) };
        let mut buf = [0u16; 1024];
        let mut file_part = std::ptr::null_mut();
        let ext = wstr(".exe");
        let name_w = wstr(&name_clean);
        let len = SearchPathW(std::ptr::null(), name_w.as_ptr(), ext.as_ptr(), 1024, buf.as_mut_ptr(), &mut file_part);
        if len > 0 {
            let mut path_vec = Vec::with_capacity((len + 1) as usize);
            path_vec.extend_from_slice(&buf[..len as usize]);
            path_vec.push(0);
            exe_path = Some(path_vec);
        }
    }

    let icon_flags = 0x100 /* SHGFI_ICON */ | if small { 0x0001 /* SHGFI_SMALLICON */ } else { 0x0000 /* SHGFI_LARGEICON */ };

    // 4. Extract icon via SHGetFileInfoW
    if let Some(path_w) = exe_path {
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let res = SHGetFileInfoW(path_w.as_ptr(), 0, &mut shfi, std::mem::size_of::<SHFILEINFOW>() as u32, icon_flags);
        if res != 0 && !shfi.hIcon.is_null() {
            return Some(shfi.hIcon);
        }
        let res2 = SHGetFileInfoW(path_w.as_ptr(), 0x80 /* FILE_ATTRIBUTE_NORMAL */, &mut shfi, std::mem::size_of::<SHFILEINFOW>() as u32, icon_flags | 0x10);
        if res2 != 0 && !shfi.hIcon.is_null() {
            return Some(shfi.hIcon);
        }
    }

    // 5. If path not found or failed, try SHGetFileInfoW with proc_name directly
    if !proc_name.is_empty() {
        let name_clean = if proc_name.ends_with(".exe") { proc_name.to_string() } else { format!("{}.exe", proc_name) };
        let name_w = wstr(&name_clean);
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let res = SHGetFileInfoW(name_w.as_ptr(), 0x80, &mut shfi, std::mem::size_of::<SHFILEINFOW>() as u32, icon_flags | 0x10);
        if res != 0 && !shfi.hIcon.is_null() {
            return Some(shfi.hIcon);
        }
    }

    None
}

// Double-Buffered Paint for Main Window
unsafe fn paint_main_double_buffered(hwnd: HWND, hdc: HDC, app: &App) {
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;

    let mem_dc = gdi32::CreateCompatibleDC(hdc);
    let mem_bm = gdi32::CreateCompatibleBitmap(hdc, w, h);
    let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

    let scale = resolve_scale(app.settings.ui_scale);

    // 1. Fill Canvas (#1e1e1e)
    gdi32::FillRect(mem_dc, &rc, app.bg_brush);

    // 2. Target Process Card Panel
    let card_l = s(16, scale);
    let card_t = s(14, scale);
    let card_r = rc.right - s(16, scale);
    let card_b = s(104, scale);

    let p_pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
    let old_p = gdi32::SelectObject(mem_dc, p_pen);
    let old_b = gdi32::SelectObject(mem_dc, app.panel_brush);
    round_rect(mem_dc, card_l, card_t, card_r, card_b, s(R_CARD, scale), s(R_CARD, scale));
    gdi32::SelectObject(mem_dc, old_b);
    gdi32::SelectObject(mem_dc, old_p);
    let _ = gdi32::DeleteObject(p_pen);

    // Process icon (or the syringe logo) drawn straight on the card, no box behind it,
    // centred on the target input box (its top and bottom edges are at s(40) and s(72))
    let icon_sz = s(40, scale);
    let icon_x = card_l + s(10, scale);
    let icon_y = (s(40, scale) + s(72, scale)) / 2 - icon_sz / 2;
    if let Some(hicon) = app.proc_icon {
        DrawIconEx(mem_dc, icon_x, icon_y, hicon, icon_sz, icon_sz, 0, NULL_HANDLE, 0x0003 /* DI_NORMAL */);
    } else {
        draw_syringe_logo(mem_dc, icon_x, icon_y, icon_sz, icon_sz);
    }

    // Card Header: TARGET PROCESS
    gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);
    gdi32::SetTextColor(mem_dc, C_TEXT_HEAD);
    let old_f = gdi32::SelectObject(mem_dc, app.hfont_small);
    let mut rc_head = RECT {
        left: card_l + s(58, scale),
        top: s(18, scale),
        right: card_r - s(10, scale),
        bottom: s(36, scale),
    };
    let w_h = wstr("TARGET PROCESS");
    gdi32::DrawTextW(mem_dc, w_h.as_ptr(), -1, &mut rc_head, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG);

    // Target status line under the input box (process title + PID)
    let mut rc_status = RECT {
        left: card_l + s(58, scale),
        // Tall enough for the font's full line height: the ellipsis clips to this rectangle,
        // and a shorter one cut off the descenders of letters like g
        top: s(75, scale),
        right: card_r - s(10, scale),
        bottom: s(97, scale),
    };
    gdi32::SetTextColor(mem_dc, C_TEXT_MUTED);
    let status_str = {
        let sh = app.shared.lock().unwrap();
        if sh.target_pid > 0 {
            if app.proc_title.is_empty() || app.proc_title == app.proc_sub {
                app.proc_sub.clone()
            } else {
                format!("{}   |   {}", app.proc_sub, app.proc_title)
            }
        } else if !sh.process_name.is_empty() {
            "Waiting for process...   |   PID: N/A".to_string()
        } else {
            "No process selected".to_string()
        }
    };
    let w_status = wstr(&status_str);
    gdi32::DrawTextW(mem_dc, w_status.as_ptr(), -1, &mut rc_status, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG | 0x00008000 /* DT_END_ELLIPSIS */);

    // Target Process Input Container Box
    let ipen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
    let ibrush = gdi32::CreateSolidBrush(C_INPUT_BG);
    let o_ip = gdi32::SelectObject(mem_dc, ipen);
    let o_ib = gdi32::SelectObject(mem_dc, ibrush);
    round_rect(mem_dc, s(74, scale), s(40, scale), s(358, scale), s(72, scale), s(6, scale), s(6, scale));
    gdi32::SelectObject(mem_dc, o_ib);
    gdi32::SelectObject(mem_dc, o_ip);
    let _ = gdi32::DeleteObject(ipen);
    let _ = gdi32::DeleteObject(ibrush);

    // Section Label: ITEMS
    let mut rc_mod_lbl = RECT {
        left: s(16, scale),
        top: s(110, scale),
        right: rc.right - s(16, scale),
        bottom: s(126, scale),
    };
    let w_mod = wstr("MODULES TO INJECT");
    gdi32::DrawTextW(mem_dc, w_mod.as_ptr(), -1, &mut rc_mod_lbl, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG);

    // Rounded Card Boundary for ITEMS table
    let lv_w = s(BASE_CLIENT_W - 32, scale);
    let card_pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
    let card_br = gdi32::CreateSolidBrush(C_PANEL);
    let ob = gdi32::SelectObject(mem_dc, card_br);
    let op = gdi32::SelectObject(mem_dc, card_pen);
    round_rect(mem_dc, s(15, scale), s(129, scale), s(17, scale) + lv_w, s(130, scale) + s(183, scale) + 1, s(R_CARD, scale), s(R_CARD, scale));
    gdi32::SelectObject(mem_dc, ob);
    gdi32::SelectObject(mem_dc, op);
    let _ = gdi32::DeleteObject(card_pen);
    let _ = gdi32::DeleteObject(card_br);

    // LOG section header for the activity panel below the buttons
    let mut rc_log_lbl = RECT {
        left: s(16, scale),
        top: s(397, scale),
        right: s(140, scale),
        bottom: s(415, scale),
    };
    gdi32::SelectObject(mem_dc, app.hfont);
    gdi32::SetTextColor(mem_dc, C_TEXT_HEAD);
    let w_log_lbl = wstr("LOG");
    gdi32::DrawTextW(mem_dc, w_log_lbl.as_ptr(), -1, &mut rc_log_lbl, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

    gdi32::SelectObject(mem_dc, old_f);

    // Blit to screen
    gdi32::BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);

    gdi32::SelectObject(mem_dc, old_bm);
    let _ = gdi32::DeleteObject(mem_bm);
    let _ = gdi32::DeleteDC(mem_dc);
}

const TARGET_REFRESH_TIMER_ID: usize = 4243;
const TARGET_REFRESH_TIMER_MS: u32 = 100;

unsafe fn invalidate_target_card(hwnd: HWND, scale: u32) {
    let rc = RECT {
        left: 0,
        top: 0,
        right: 2000,
        bottom: s(108, scale),
    };
    InvalidateRect(hwnd, &rc, 0);
}

unsafe fn is_pid_alive_matching(pid: u32, expected_name: &str) -> bool {
    if pid == 0 {
        return false;
    }
    let h = kernel32::OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if h.is_null() {
        return false;
    }
    let mut code = 0u32;
    let ok = kernel32::GetExitCodeProcess(h, &mut code);
    if ok == 0 || code != STILL_ACTIVE {
        let _ = kernel32::CloseHandle(h);
        return false;
    }
    let mut buf = [0u16; 512];
    let mut size = buf.len() as u32;
    let q_ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
    let _ = kernel32::CloseHandle(h);
    if q_ok != 0 && size > 0 {
        let full_path = String::from_utf16_lossy(&buf[..size as usize]);
        let exe_name = std::path::Path::new(&full_path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");
        let norm = |s: &str| s.trim().trim_end_matches(".exe").to_ascii_lowercase();
        return norm(exe_name) == norm(expected_name);
    }
    true
}

unsafe fn poll_target_process_status(hwnd: HWND, app: &mut App) {
    let (current_pid, proc_name) = {
        let sh = app.shared.lock().unwrap();
        (sh.target_pid, sh.process_name.clone())
    };

    let trimmed = proc_name.trim();
    let scale = resolve_scale(app.settings.ui_scale);

    if trimmed.is_empty() {
        if current_pid != 0 || app.proc_sub != "PID: N/A" {
            app.shared.lock().unwrap().target_pid = 0;
            app.proc_title = "Waiting for process...".to_string();
            app.proc_sub = "PID: N/A".to_string();
            if let Some(old_icon) = app.proc_icon.take() {
                DestroyIcon(old_icon);
            }
            invalidate_target_card(hwnd, scale);
        }
        return;
    }

    if current_pid > 0 && is_pid_alive_matching(current_pid, trimmed) {
        // Target is actively running: keep title in sync if it changes
        let title = find_main_window_title(current_pid);
        let title_display = if title.is_empty() { trimmed.to_string() } else { title };
        let sub = format!("PID: {} (0x{:X})", current_pid, current_pid);
        if app.proc_title != title_display || app.proc_sub != sub {
            app.proc_title = title_display;
            app.proc_sub = sub;
            invalidate_target_card(hwnd, scale);
        }
    } else {
        // Process is closed or unlinked: check if a running instance exists
        if let Some(found) = crate::process::find_by_name(trimmed) {
            let new_pid = found.pid;
            app.shared.lock().unwrap().target_pid = new_pid;
            if let Some(old_icon) = app.proc_icon.take() {
                DestroyIcon(old_icon);
            }
            app.proc_icon = fetch_process_icon(new_pid, trimmed);
            let title = find_main_window_title(new_pid);
            app.proc_title = if title.is_empty() { trimmed.to_string() } else { title };
            app.proc_sub = format!("PID: {} (0x{:X})", new_pid, new_pid);
            invalidate_target_card(hwnd, scale);
        } else if current_pid != 0 || app.proc_sub != "PID: N/A" {
            // Process terminated and no instance is running
            app.shared.lock().unwrap().target_pid = 0;
            app.proc_title = "Waiting for process...".to_string();
            app.proc_sub = "PID: N/A".to_string();
            invalidate_target_card(hwnd, scale);
        }
    }
}

unsafe fn update_process_info_with_pid(hwnd: HWND, app: &mut App, proc_name: &str, pid: u32) {
    let (old_pid, old_name) = {
        let sh = app.shared.lock().unwrap();
        (sh.target_pid, sh.process_name.clone())
    };
    if old_pid == pid && (pid == 0 || old_name == proc_name) {
        app.shared.lock().unwrap().process_name = proc_name.to_string();
        return;
    }

    app.shared.lock().unwrap().target_pid = pid;
    app.shared.lock().unwrap().process_name = proc_name.to_string();

    if let Some(old_icon) = app.proc_icon.take() {
        DestroyIcon(old_icon);
    }
    app.proc_icon = fetch_process_icon(pid, proc_name);

    if pid > 0 {
        let title = find_main_window_title(pid);
        app.proc_title = if title.is_empty() { proc_name.to_string() } else { title };
        app.proc_sub = format!("PID: {} (0x{:X})", pid, pid);
    } else {
        app.proc_title = "Waiting for process...".to_string();
        app.proc_sub = "PID: N/A".to_string();
    }

    let scale = resolve_scale(app.settings.ui_scale);
    invalidate_target_card(hwnd, scale);
}

// Update Process Status Info
unsafe fn update_process_info(hwnd: HWND, app: &mut App) {
    let edit = GetDlgItem(hwnd, IDC_PROC_EDIT);
    let mut buf = [0u16; 260];
    let len = GetWindowTextW(edit, buf.as_mut_ptr(), 260);
    let proc_name = if len > 0 { from_wsz(buf.as_ptr()) } else { String::new() };

    let pid = if !proc_name.is_empty() {
        crate::process::find_by_name(&proc_name).map(|p| p.pid).unwrap_or(0)
    } else {
        0
    };

    update_process_info_with_pid(hwnd, app, &proc_name, pid);
}

// ===== Control Helpers =====
unsafe fn mk(parent: HWND, hinst: HINSTANCE, class: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: i32, font: HFONT) -> HWND {
    mk_ex(parent, hinst, class, text, style, 0, x, y, w, h, id, font)
}

unsafe fn mk_ex(parent: HWND, hinst: HINSTANCE, class: &str, text: &str, style: u32, ex_style: u32, x: i32, y: i32, w: i32, h: i32, id: i32, font: HFONT) -> HWND {
    let wclass = wstr(class);
    let wtext = wstr(text);
    let hwnd = CreateWindowExW(
        ex_style,
        wclass.as_ptr(),
        wtext.as_ptr(),
        style,
        x,
        y,
        w,
        h,
        parent,
        id as isize as HMENU,
        hinst,
        std::ptr::null_mut(),
    );
    if !font.is_null() && !hwnd.is_null() {
        SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
    }
    hwnd
}

// Create Main Controls
unsafe fn create_main_controls(hwnd: HWND, app: &mut App) {
    let hinst = get_module_handle();

    // 1. Process Edit
    let edit = mk_ex(
        hwnd, hinst, WC_EDIT_S, &app.settings.process_name,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL_C,
        0, 0, 0, 0, 0, IDC_PROC_EDIT, app.hfont
    );
    uxtheme::set_theme(edit, "DarkMode_CFD");
    subclass_edit(edit);

    // 2. Select Process Button
    let b_sel = mk(hwnd, hinst, WC_BUTTON_S, "Select Process", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, 0, 0, 0, 0, IDC_SELECT_BTN, app.hfont_bold);
    subclass_button(b_sel, false);

    // 3. Full-Width DLL ListView (Owner-Drawn Report View)
    let lv = mk_ex(
        hwnd, hinst, WC_LISTVIEW_S, "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | LVS_REPORT_C | LVS_SHOWSELALWAYS_C | LVS_OWNERDRAWFIXED_C,
        0, 0, 0, 0, 0, IDC_LIST, app.hfont
    );
    SendMessageW(lv, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT_E | LVS_EX_DOUBLEBUFFER_E) as LPARAM);
    SendMessageW(lv, LVM_SETBKCOLOR_ID, 0, C_PANEL as LPARAM);
    SendMessageW(lv, LVM_SETTEXTBKCOLOR_ID, 0, C_PANEL as LPARAM);
    SendMessageW(lv, LVM_SETTEXTCOLOR_ID, 0, C_TEXT_BODY as LPARAM);
    let orig_lv = SetWindowLongPtrW(lv, GWLP_WNDPROC, dark_listview_proc as usize as isize);
    SetPropW(lv, wstr("OrigLVProc").as_ptr(), orig_lv as HANDLE);
    uxtheme::set_theme(lv, "DarkMode_Explorer");
    uxtheme::allow_dark_mode_for_window(lv);

    let header = SendMessageW(lv, LVM_GETHEADER_ID, 0, 0) as HWND;
    if !header.is_null() {
        let orig_hdr = SetWindowLongPtrW(header, GWLP_WNDPROC, dark_header_proc as usize as isize);
        SetPropW(header, wstr("OrigHdrProc").as_ptr(), orig_hdr as HANDLE);
        uxtheme::allow_dark_mode_for_window(header);
    }

    // Insert Columns. The order and delay columns make the injection sequence visible and
    // editable; auto_fit_module_columns sizes them at layout time.
    let columns: [(&str, i32, i32); 8] = [
        ("#", LIST_ORDER_COL, 30),
        ("DLL Name", LIST_NAME_COL, 150),
        ("Arch", LIST_ARCH_COL, 50),
        ("Process", LIST_PROCESS_COL, 70),
        ("Method", LIST_METHOD_COL, 56),
        ("Delay", LIST_DELAY_COL, 62),
        ("Full Path", LIST_PATH_COL, 120),
        ("Action", LIST_ACTIONS_COL, 36),
    ];
    for (title, index, width) in columns {
        let wtitle = wstr(title);
        let mut lvc = LVCOLUMNW {
            mask: LVCF_TEXT_C | LVCF_WIDTH_C | LVCF_SUBITEM_C,
            cx: width,
            pszText: wtitle.as_ptr() as *mut u16,
            iSubItem: index,
            ..Default::default()
        };
        SendMessageW(lv, LVM_INSERTCOLUMNW, index as WPARAM, &mut lvc as *mut _ as LPARAM);
    }

    // Populate the rows for the saved list, which is already in Shared
    for m in &app.settings.modules {
        list_insert_row(hwnd, m);
    }

    // 4. Row Below Table: + Add Button
    let b_add = mk(hwnd, hinst, WC_BUTTON_S, "+ Add DLL", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, 0, 0, 0, 0, IDC_ADD, app.hfont_bold);
    subclass_button(b_add, false);

    // 5. Row Below Table: Method ComboBox Dropdown
    let cb_method = mk_ex(
        hwnd, hinst, WC_COMBOBOX_S, "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST_C | CBS_HASSTRINGS_C,
        0, 0, 0, 0, 0, IDC_MAIN_METHOD, app.hfont
    );
    for m in Settings::METHODS {
        SendMessageW(cb_method, CB_ADDSTRING, 0, wstr(m).as_ptr() as LPARAM);
    }
    SendMessageW(cb_method, CB_SETCURSEL, app.settings.method as WPARAM, 0);
    subclass_combobox(cb_method, resolve_scale(app.settings.ui_scale), C_BG, app.hfont);

    // Clear all: empties the list. Sits right of Add, and its "- " prefix is drawn as an anti-aliased minus
    let b_clear = mk(hwnd, hinst, WC_BUTTON_S, "- Clear all",WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, 0, 0, 0, 0, IDC_CLEAR, app.hfont_bold);
    subclass_button(b_clear, false);

    // 6. Bottom Action Row (About lives in Settings)
    let b_set = mk(hwnd, hinst, WC_BUTTON_S, "Settings", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, 0, 0, 0, 0, IDC_SETTINGS, app.hfont_bold);
    subclass_button(b_set, false);

    let b_inj = mk(hwnd, hinst, WC_BUTTON_S, "Inject", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, 0, 0, 0, 0, IDC_INJECT, app.hfont_btn);
    subclass_button(b_inj, true); // Porcelain White Primary CTA

    // Activity log panel
    create_log_panel(hwnd, hinst, resolve_scale(app.settings.ui_scale), app.hfont_small);

}

// ===== Main Dynamic Scaling & Layout =====
// Architecture label for a module file. Cached per path, since reading the PE header on every repaint is slow.
fn module_arch(cache: &mut HashMap<String, &'static str>, path: &str) -> &'static str {
    *cache.entry(path.to_string()).or_insert_with(|| match crate::process::dll_machine(path) {
        Ok(0x8664) => "x64",
        Ok(0x14C) => "x32",
        Ok(0xAA64) => "arm64",
        Ok(_) => "other",
        Err(_) => "n/a",
    })
}

// Width of a string in the DC's current font
unsafe fn text_width(hdc: HDC, text: &str) -> i32 {
    let w = wstr(text);
    let mut sz = SIZE::default();
    gdi32::GetTextExtentPoint32W(hdc, w.as_ptr(), (w.len() - 1) as i32, &mut sz);
    sz.cx
}

// Width of one list column, in pixels
unsafe fn lv_col_width(lv: HWND, col: i32) -> i32 {
    SendMessageW(lv, LVM_GETCOLUMNWIDTH_ID, col as WPARAM, 0) as i32
}

// What the method column shows: the per-file override, or "global" when the global method applies
fn module_method_label(method: Option<u32>) -> String {
    match method {
        None => "global".to_string(),
        Some(0) => "Std".to_string(),
        Some(1) => "Ldr".to_string(),
        Some(2) => "Map".to_string(),
        Some(3) => "Hijack".to_string(),
        Some(4) => "Nt".to_string(),
        Some(5) => "APC".to_string(),
        Some(_) => "global".to_string(),
    }
}

// What the delay column shows: the file's own delay, or "global" when the global delays apply
fn module_delay_label(ms: u32) -> String {
    if ms == 0 {
        "global".to_string()
    } else {
        format!("{} ms", ms)
    }
}

unsafe fn auto_fit_module_columns(hwnd: HWND, app: &mut App, scale: u32) {
    let lv = GetDlgItem(hwnd, IDC_LIST);
    if lv.is_null() { return; }

    // The real client width, so the columns always fit inside the list whether or not a scrollbar shows
    let mut client = RECT::default();
    GetClientRect(lv, &mut client);
    let lv_w = client.right;

    // Every column is as wide as its longest value, header included. The path column takes the rest.
    let modules: Vec<Module> = app.shared.lock().unwrap().modules.clone();
    let names: Vec<String> = modules.iter().map(|m| m.file_name()).collect();
    let arches: Vec<&'static str> = modules.iter().map(|m| module_arch(&mut app.arch_cache, &m.path)).collect();
    let delays: Vec<String> = modules.iter().map(|m| module_delay_label(m.delay_ms)).collect();
    let methods: Vec<String> = modules.iter().map(|m| module_method_label(m.method)).collect();
    let processes: Vec<String> = modules.iter().map(|m| if m.require_process.trim().is_empty() { "global".to_string() } else { m.require_process.clone() }).collect();

    // Names and headers are measured bold, method/arch/delay values regular, as the rows draw them
    let hdc = GetDC(lv);
    let old_font = gdi32::SelectObject(hdc, app.hfont_bold);
    let name_text_w = names.iter().map(|n| text_width(hdc, n)).max().unwrap_or(0);
    let name_head_w = text_width(hdc, "DLL Name");
    let method_head_w = text_width(hdc, "Method");
    let process_head_w = text_width(hdc, "Process");
    let arch_head_w = text_width(hdc, "Arch");
    let delay_head_w = text_width(hdc, "Delay");
    let action_head_w = text_width(hdc, "Action");
    let order_head_w = text_width(hdc, "#");
    gdi32::SelectObject(hdc, app.hfont);
    let arch_text_w = arches.iter().map(|a| text_width(hdc, a)).max().unwrap_or(0);
    gdi32::SelectObject(hdc, app.hfont_small);
    let delay_text_w = delays.iter().map(|d| text_width(hdc, d)).max().unwrap_or(0);
    let method_text_w = methods.iter().map(|d| text_width(hdc, d)).max().unwrap_or(0);
    let process_text_w = processes.iter().map(|p| text_width(hdc, p)).max().unwrap_or(0);
    gdi32::SelectObject(hdc, old_font);
    ReleaseDC(lv, hdc);

    // Order: the number and a little air either side
    let col_order_w = (order_head_w + s(18, scale)).max(s(30, scale));
    // Name: the checkbox and its gap (s(31) before the text), the longest name, and a small margin
    let min_col_name = s(92, scale);
    let col_name_need = name_text_w.max(name_head_w) + s(39, scale);
    // Method, arch and delay: the text sits s(6) in from each edge
    let col_method_w = (method_text_w.max(method_head_w) + s(14, scale)).max(s(52, scale));
    let col_arch_w = (arch_text_w.max(arch_head_w) + s(16, scale)).max(s(40, scale));
    let col_process_w = (process_text_w.max(process_head_w) + s(14, scale)).max(s(64, scale));
    let col_delay_w = (delay_text_w.max(delay_head_w) + s(16, scale)).max(s(54, scale));
    // Action: the dots are centred in the cell
    let col_actions_w = (action_head_w + s(16, scale)).max(s(36, scale));
    let min_path_w = s(80, scale);

    // Name gets what it needs, but never so much that the path column drops below its minimum
    let name_budget = (lv_w - col_order_w - col_method_w - col_arch_w - col_process_w - col_delay_w - col_actions_w - min_path_w).max(min_col_name);
    let col_name_w = col_name_need.clamp(min_col_name, name_budget);
    // The path column takes whatever is left, so all seven columns end exactly at the list's edge
    let col_path_w = (lv_w - col_order_w - col_name_w - col_method_w - col_arch_w - col_process_w - col_delay_w - col_actions_w).max(s(70, scale));

    for (col, w) in [
        (LIST_ORDER_COL, col_order_w),
        (LIST_NAME_COL, col_name_w),
        (LIST_ARCH_COL, col_arch_w),
        (LIST_PROCESS_COL, col_process_w),
        (LIST_METHOD_COL, col_method_w),
        (LIST_DELAY_COL, col_delay_w),
        (LIST_PATH_COL, col_path_w),
        (LIST_ACTIONS_COL, col_actions_w),
    ] {
        SendMessageW(lv, LVM_SETCOLUMNWIDTH_ID, col as WPARAM, w as LPARAM);
    }
}

unsafe fn init_app_fonts(app: &mut App, scale: u32) {
    let font_name = wstr("Segoe UI");
    if !app.hfont.is_null() { gdi32::DeleteObject(app.hfont); }
    if !app.hfont_bold.is_null() { gdi32::DeleteObject(app.hfont_bold); }
    if !app.hfont_small.is_null() { gdi32::DeleteObject(app.hfont_small); }
    if !app.hfont_italic.is_null() { gdi32::DeleteObject(app.hfont_italic); }
    if !app.hfont_btn.is_null() { gdi32::DeleteObject(app.hfont_btn); }
    if !app.hfont_title.is_null() { gdi32::DeleteObject(app.hfont_title); }

    app.hfont = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    app.hfont_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    app.hfont_small = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    app.hfont_italic = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 1, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    app.hfont_btn = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    app.hfont_title = gdi32::CreateFontW(s(-20, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
}

unsafe fn apply_scale(hwnd: HWND, app: &mut App) {
    let scale = resolve_scale(app.settings.ui_scale);

    // 1. Recreate fonts for current scale
    init_app_fonts(app, scale);

    // 2. Adjust Window Outer Size
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: s(BASE_CLIENT_W, scale),
        bottom: s(BASE_CLIENT_H, scale),
    };
    AdjustWindowRect(&mut rc, WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX, 0);
    let win_w = rc.right - rc.left;
    let win_h = rc.bottom - rc.top;
    let mut cur_rc = RECT::default();
    GetWindowRect(hwnd, &mut cur_rc);
    SetWindowPos(hwnd, NULL_HANDLE, cur_rc.left, cur_rc.top, win_w, win_h, 0x0020 /* SWP_FRAMECHANGED */ | SWP_NOZORDER | SWP_NOACTIVATE_FLAG);

    // 3. Move and Resize Controls
    // Process Edit & Select Button
    let edit = GetDlgItem(hwnd, IDC_PROC_EDIT);
    MoveWindow(edit, s(84, scale), s(45, scale), s(264, scale), s(22, scale), 1);
    SendMessageW(edit, WM_SETFONT, app.hfont as WPARAM, 1);
    SendMessageW(edit, 0x00D3 /* EM_SETMARGINS */, 1 /* EC_LEFTMARGIN */, 4 as LPARAM);

    let sel_btn = GetDlgItem(hwnd, IDC_SELECT_BTN);
    MoveWindow(sel_btn, s(368, scale), s(40, scale), s(124, scale), s(32, scale), 1);
    SendMessageW(sel_btn, WM_SETFONT, app.hfont_bold as WPARAM, 1);

    // Full-Width DLL ListView
    let lv = GetDlgItem(hwnd, IDC_LIST);
    let lv_w = s(BASE_CLIENT_W - 32, scale);
    let lv_h = s(183, scale);
    MoveWindow(lv, s(16, scale), s(130, scale), lv_w, lv_h, 1);
    SendMessageW(lv, WM_SETFONT, app.hfont as WPARAM, 1);
    let lv_rgn = CreateRoundRectRgn(0, 0, lv_w, lv_h, s(R_CARD, scale) * 2, s(R_CARD, scale) * 2);
    SetWindowRgn(lv, lv_rgn, 1);
    auto_fit_module_columns(hwnd, app, scale);
    let header = SendMessageW(lv, LVM_GETHEADER_ID, 0, 0) as HWND;
    if !header.is_null() {
        InvalidateRect(header, std::ptr::null(), 1);
    }

    // Controls Below Table, laid out as two equal columns: the add/clear pair and the
    // method picker on the first row, Settings and Inject on the second.
    let half_w = (BASE_CLIENT_W - 32 - 8) / 2;

    // + Add DLL Button
    let b_add = GetDlgItem(hwnd, IDC_ADD);
    MoveWindow(b_add, s(16, scale), s(321, scale), s((half_w - 8) / 2, scale), s(32, scale), 1);
    SendMessageW(b_add, WM_SETFONT, app.hfont_bold as WPARAM, 1);

    // - Clear all Button, right beside it
    let b_clear = GetDlgItem(hwnd, IDC_CLEAR);
    MoveWindow(b_clear, s(16 + (half_w - 8) / 2 + 8, scale), s(321, scale), s((half_w - 8) / 2, scale), s(32, scale), 1);
    SendMessageW(b_clear, WM_SETFONT, app.hfont_bold as WPARAM, 1);

    // Method ComboBox: same width as the whole left column, on its own right
    let cb_meth = GetDlgItem(hwnd, IDC_MAIN_METHOD);
    let combo_left = s(16 + half_w + 8, scale);
    MoveWindow(cb_meth, combo_left, s(321, scale), s(half_w, scale), s(200, scale), 1);
    SendMessageW(cb_meth, WM_SETFONT, app.hfont as WPARAM, 1);
    // The list drops wider than the closed field so the full method names fit
    SendMessageW(cb_meth, CB_SETDROPPEDWIDTH, s(340, scale) as WPARAM, 0);
    fit_combo_closed_height(cb_meth, scale);
    SendMessageW(cb_meth, CB_SETITEMHEIGHT, 0, s(26, scale) as LPARAM);
    update_combobox_scale(cb_meth, scale, app.hfont);

    // Bottom Action Row: Settings under the add/clear pair, Inject under the method picker
    let b_set = GetDlgItem(hwnd, IDC_SETTINGS);
    MoveWindow(b_set, s(16, scale), s(361, scale), s(half_w, scale), s(32, scale), 1);
    SendMessageW(b_set, WM_SETFONT, app.hfont_bold as WPARAM, 1);

    let b_inj = GetDlgItem(hwnd, IDC_INJECT);
    MoveWindow(b_inj, combo_left, s(361, scale), s(half_w, scale), s(32, scale), 1);
    SendMessageW(b_inj, WM_SETFONT, app.hfont_btn as WPARAM, 1);

    // Activity log panel: full width, under the action buttons
    let log = GetDlgItem(hwnd, IDC_LOG_PANEL);
    if !log.is_null() {
        let w = s(BASE_CLIENT_W - 32, scale);
        let h = s(120, scale);
        MoveWindow(log, s(16, scale), s(417, scale), w, h, 1);
        let rgn = CreateRoundRectRgn(0, 0, w, h, s(R_CARD, scale) * 2, s(R_CARD, scale) * 2);
        SetWindowRgn(log, rgn, 1);
        SendMessageW(log, WM_SETFONT, app.hfont_small as WPARAM, 1);
        InvalidateRect(log, std::ptr::null(), 0);
    }


    InvalidateRect(hwnd, std::ptr::null(), 1);
    RedrawWindow(hwnd, std::ptr::null(), NULL_HANDLE, 0x0001 | 0x0002 | 0x0004 | 0x0080 | 0x0100);
}

// ===== Main Window Procedure =====
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if app_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let app = &mut *app_ptr;
    let scale = resolve_scale(app.settings.ui_scale);

    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_main_double_buffered(hwnd, hdc, app);
            EndPaint(hwnd, &ps);
            0
        }
        0x020A /* WM_MOUSEWHEEL */ => {
            // Route the wheel to the log whenever the cursor is over it, focused or not
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            let panel = GetDlgItem(hwnd, IDC_LOG_PANEL);
            if !panel.is_null() {
                let mut prc = RECT::default();
                GetWindowRect(panel, &mut prc);
                if pt.x >= prc.left && pt.x < prc.right && pt.y >= prc.top && pt.y < prc.bottom {
                    SendMessageW(panel, msg, wp, lp);
                    return 0;
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_PANEL);
            app.panel_brush as LRESULT
        }
        WM_CTLCOLOREDIT => {
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_INPUT_BG);
            app.input_brush as LRESULT
        }
        WM_CTLCOLORLISTBOX => {
            let lbox = lp as HWND;
            if !lbox.is_null() {
                subclass_lbox(lbox, scale, app.hfont);
            }
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_INPUT_BG);
            app.input_brush as LRESULT
        }
        WM_MEASUREITEM => {
            // Module list rows: just taller than the font's line height. Any shorter and the descenders (the "g")
            // get cut off by the ellipsis clip.
            let mis = &mut *(lp as *mut MEASUREITEMSTRUCT);
            if mis.CtlType == ODT_LISTVIEW {
                mis.itemHeight = s(19, scale) as u32;
                return 1;
            }
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, scale);
                return 1;
            } else if dis.CtlType == ODT_COMBOBOX {
                draw_owner_combobox(dis, scale, app.hfont);
                return 1;
            } else if dis.CtlType == ODT_LISTVIEW {
                // Owner-draw DLL row: order, checkbox + name, arch, per-file delay, path, dots
                let idx = dis.itemID as usize;
                let sh = app.shared.lock().unwrap();
                if idx < sh.modules.len() {
                    let m = &sh.modules[idx];
                    let filename = m.file_name();
                    let checked = m.enabled;

                    let is_selected = (dis.itemState & ODS_SELECTED) != 0;
                    let hdc = dis.hDC;
                    let rc = dis.rcItem;

                    // Row background
                    let bg_color = if is_selected {
                        C_ROW_SEL
                    } else if idx % 2 == 1 {
                        C_PANEL_ALT
                    } else {
                        C_PANEL
                    };
                    let brush = gdi32::CreateSolidBrush(bg_color);
                    gdi32::FillRect(hdc, &rc, brush);
                    let _ = gdi32::DeleteObject(brush);

                    // Cell edges, so the columns line up with the header
                    let lv = dis.hwndItem;
                    let w_order = lv_col_width(lv, LIST_ORDER_COL);
                    let w_name = lv_col_width(lv, LIST_NAME_COL);
                    let w_method = lv_col_width(lv, LIST_METHOD_COL);
                    let w_arch = lv_col_width(lv, LIST_ARCH_COL);
                    let w_process = lv_col_width(lv, LIST_PROCESS_COL);
                    let w_delay = lv_col_width(lv, LIST_DELAY_COL);
                    let w_path = lv_col_width(lv, LIST_PATH_COL);
                    let w_actions = lv_col_width(lv, LIST_ACTIONS_COL);
                    let x_order = rc.left;
                    let x_name = x_order + w_order;
                    let x_arch = x_name + w_name;
                    let x_process = x_arch + w_arch;
                    let x_method = x_process + w_process;
                    let x_delay = x_method + w_method;
                    let x_path = x_delay + w_delay;
                    let x_actions = x_path + w_path;

                    let old_bk = gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
                    let old_f = gdi32::SelectObject(hdc, app.hfont_bold);

                    // 1) Order: the position this file is injected at (1-based)
                    gdi32::SelectObject(hdc, app.hfont_small);
                    gdi32::SetTextColor(hdc, if checked { C_TEXT_HEAD } else { 0x005A5A5A });
                    let mut rc_order = RECT {
                        left: x_order + s(4, scale),
                        top: rc.top,
                        right: x_order + w_order - s(4, scale),
                        bottom: rc.bottom,
                    };
                    let w_num = wstr(&format!("{}", idx + 1));
                    gdi32::DrawTextW(hdc, w_num.as_ptr(), -1, &mut rc_order, DT_CENTER_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                    // 2) Enabled checkbox and file name
                    let box_size = s(15, scale);
                    let cb_x = x_name + s(8, scale);
                    let cb_y = rc.top + (rc.bottom - rc.top - box_size) / 2;
                    draw_custom_checkbox(hdc, cb_x, cb_y, box_size, checked, false, scale, bg_color);

                    // A checked row's name is bright and bold
                    gdi32::SetTextColor(hdc, if checked { C_TEXT_BODY } else { 0x00808080 });
                    gdi32::SelectObject(hdc, if checked { app.hfont_bold } else { app.hfont });

                    let mut rc_name = RECT {
                        left: cb_x + box_size + s(8, scale),
                        top: rc.top,
                        right: x_name + w_name - s(6, scale),
                        bottom: rc.bottom,
                    };
                    let w_name_txt = wstr(&filename);
                    gdi32::DrawTextW(hdc, w_name_txt.as_ptr(), -1, &mut rc_name, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x00008000 /* DT_END_ELLIPSIS */ | DT_NOPREFIX_FLAG);

                    // 3) Per-file method override ("global" = the global method)
                    gdi32::SelectObject(hdc, app.hfont_small);
                    gdi32::SetTextColor(hdc, if m.method.is_some() { C_TEXT_BODY } else { C_TEXT_MUTED });
                    let mut rc_method = RECT {
                        left: x_method + s(6, scale),
                        top: rc.top,
                        right: x_method + w_method - s(4, scale),
                        bottom: rc.bottom,
                    };
                    let w_method_txt = wstr(&module_method_label(m.method));
                    gdi32::DrawTextW(hdc, w_method_txt.as_ptr(), -1, &mut rc_method, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                    // 4) Architecture from the PE header (x64 / x32). Files without one, such as text files, show n/a.
                    let arch = module_arch(&mut app.arch_cache, &m.path);

                    let mut rc_arch = RECT {
                        left: x_arch + s(6, scale),
                        top: rc.top,
                        right: x_arch + w_arch - s(6, scale),
                        bottom: rc.bottom,
                    };
                    // Muted and regular weight, so the architecture reads as data and not as a name
                    gdi32::SelectObject(hdc, app.hfont);
                    gdi32::SetTextColor(hdc, C_TEXT_HEAD);
                    let w_arch_txt = wstr(arch);

                    // Required process for this file ("global" = the current target)
                    gdi32::SelectObject(hdc, app.hfont_small);
                    let req_proc = m.require_process.trim();
                    gdi32::SetTextColor(hdc, if req_proc.is_empty() { C_TEXT_MUTED } else { C_TEXT_BODY });
                    let mut rc_proc = RECT {
                        left: x_process + s(6, scale),
                        top: rc.top,
                        right: x_process + w_process - s(6, scale),
                        bottom: rc.bottom,
                    };
                    let proc_label = if req_proc.is_empty() { "global".to_string() } else { req_proc.to_string() };
                    let w_proc_txt = wstr(&proc_label);
                    gdi32::DrawTextW(hdc, w_proc_txt.as_ptr(), -1, &mut rc_proc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x00008000 /* DT_END_ELLIPSIS */ | DT_NOPREFIX_FLAG);
                    gdi32::DrawTextW(hdc, w_arch_txt.as_ptr(), -1, &mut rc_arch, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                    // 5) Per-file delay. "global" means the global start / between-DLLs delays apply.
                    gdi32::SelectObject(hdc, app.hfont_small);
                    gdi32::SetTextColor(hdc, if m.delay_ms > 0 { C_TEXT_BODY } else { C_TEXT_MUTED });
                    let mut rc_delay = RECT {
                        left: x_delay + s(6, scale),
                        top: rc.top,
                        right: x_delay + w_delay - s(6, scale),
                        bottom: rc.bottom,
                    };
                    let w_delay_txt = wstr(&module_delay_label(m.delay_ms));
                    gdi32::DrawTextW(hdc, w_delay_txt.as_ptr(), -1, &mut rc_delay, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                    // 6) Full path
                    gdi32::SetTextColor(hdc, C_TEXT_MUTED);
                    let mut rc_path = RECT {
                        left: x_path + s(6, scale),
                        top: rc.top,
                        right: x_path + w_path - s(8, scale),
                        bottom: rc.bottom,
                    };
                    let w_path_txt = wstr(&m.path);
                    gdi32::DrawTextW(hdc, w_path_txt.as_ptr(), -1, &mut rc_path, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x00008000 /* DT_END_ELLIPSIS */ | DT_NOPREFIX_FLAG);

                    // 6) Actions cell: three dots that open the row menu
                    let dots_hot = DOTS_HOVER_ROW.load(std::sync::atomic::Ordering::Relaxed) == idx as i32;
                    draw_row_dots(hdc, x_actions, rc.top, w_actions, rc.bottom - rc.top, bg_color, dots_hot);

                    gdi32::SelectObject(hdc, old_f);
                    gdi32::SetBkMode(hdc, old_bk);
                }
                return 1;
            }
            0
        }
        WM_NOTIFY => {
            let nmhdr = &*(lp as *const NMHDR);
            if nmhdr.code == NM_CUSTOMDRAW_C {
                let lv = GetDlgItem(hwnd, IDC_LIST);
                let header = SendMessageW(lv, LVM_GETHEADER_ID, 0, 0) as HWND;
                if nmhdr.hwndFrom == header {
                    let nmcd = &*(lp as *const NMCUSTOMDRAW);
                    if nmcd.dwDrawStage == CDDS_PREPAINT_C {
                        return CDRF_NOTIFYITEMDRAW_C as LRESULT;
                    } else if nmcd.dwDrawStage == CDDS_ITEMPREPAINT_C {
                        let hdc = nmcd.hdc;
                        let rc = nmcd.rc;

                        let br = gdi32::CreateSolidBrush(0x00232120);
                        gdi32::FillRect(hdc, &rc, br);
                        gdi32::DeleteObject(br);

                        let border_br = gdi32::CreateSolidBrush(0x003C3C3C);
                        let b_rc = RECT { left: rc.left, top: rc.bottom - 1, right: rc.right, bottom: rc.bottom };
                        gdi32::FillRect(hdc, &b_rc, border_br);
                        let s_rc = RECT { left: rc.right - 1, top: rc.top + 3, right: rc.right, bottom: rc.bottom - 3 };
                        gdi32::FillRect(hdc, &s_rc, border_br);
                        gdi32::DeleteObject(border_br);

                        let mut text_buf = [0u16; 128];
                        let mut item = HDITEMW::default();
                        item.mask = HDI_TEXT_C;
                        item.pszText = text_buf.as_mut_ptr();
                        item.cchTextMax = 128;
                        SendMessageW(header, HDM_GETITEMW_C, nmcd.dwItemSpec as WPARAM, &mut item as *mut _ as LPARAM);

                        gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
                        gdi32::SetTextColor(hdc, 0x00A0A0A0);
                        let old_f = gdi32::SelectObject(hdc, app.hfont_bold);
                        let mut trc = RECT {
                            left: rc.left + 8,
                            top: rc.top,
                            right: rc.right - 8,
                            bottom: rc.bottom,
                        };
                        gdi32::DrawTextW(hdc, text_buf.as_ptr(), -1, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
                        gdi32::SelectObject(hdc, old_f);

                        return CDRF_SKIPDEFAULT_C as LRESULT;
                    }
                }
            }
            if nmhdr.idFrom == IDC_LIST as usize {
                if nmhdr.code == NM_CLICK || nmhdr.code == NM_DBLCLK {
                    let nm = &*(lp as *const NMITEMACTIVATE);
                    let dbl = nmhdr.code == NM_DBLCLK;
                    let x = nm.ptAction.x;
                    // Cell edges, so a click lands where the row was drawn
                    let lv = GetDlgItem(hwnd, IDC_LIST);
                    let w_order = lv_col_width(lv, LIST_ORDER_COL);
                    let w_name = lv_col_width(lv, LIST_NAME_COL);
                    let w_method = lv_col_width(lv, LIST_METHOD_COL);
                    let w_arch = lv_col_width(lv, LIST_ARCH_COL);
                    let w_process = lv_col_width(lv, LIST_PROCESS_COL);
                    let w_delay = lv_col_width(lv, LIST_DELAY_COL);
                    let w_path = lv_col_width(lv, LIST_PATH_COL);
                    let x_name = w_order;
                    let x_delay = x_name + w_name + w_arch + w_process + w_method;
                    let x_actions = x_delay + w_delay + w_path;

                    if nm.iItem >= 0 && !dbl && x >= x_actions {
                        // The dots open the row menu, the same as a right click on the row
                        PostMessageW(hwnd, WM_APP_ROW_MENU, nm.iItem as WPARAM, 0);
                    } else if nm.iItem >= 0 && dbl && x >= x_delay && x < x_actions {
                        // Double-clicking the delay cell edits the per-file delay
                        PostMessageW(hwnd, WM_APP_EDIT_DELAY, nm.iItem as WPARAM, 0);
                    } else if nm.iItem >= 0 {
                        // Clicking the checkbox area of the name cell, or double-clicking anywhere
                        // else on the row, toggles whether the file is injected
                        if (x >= x_name && x <= x_name + s(36, scale)) || dbl {
                            let mut sh = app.shared.lock().unwrap();
                            let idx = nm.iItem as usize;
                            if idx < sh.modules.len() {
                                sh.modules[idx].enabled = !sh.modules[idx].enabled;
                                drop(sh);
                                InvalidateRect(nmhdr.hwndFrom, std::ptr::null(), 0);
                                sync_ui_to_settings(hwnd, app);
                                app.settings.save();
                            }
                        }
                    }
                } else if nmhdr.code == NM_RCLICK {
                    let nm = &*(lp as *const NMITEMACTIVATE);
                    if nm.iItem >= 0 {
                        // Posted, not run here: the menu opens after this notification returns and the list has let go of the mouse
                        PostMessageW(hwnd, WM_APP_ROW_MENU, nm.iItem as WPARAM, 0);
                    }
                }
            }
            0
        }
        WM_DROPFILES => {
            handle_drop_files(hwnd, wp as HANDLE, app);
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            let code = hiword(wp as u32);

            if id == IDC_PROC_EDIT && code == 0x0300 /* EN_CHANGE */ {
                update_process_info(hwnd, app);
                sync_ui_to_settings(hwnd, app);
                // Let the typing settle, then swap in that process' saved setup
                SetTimer(hwnd, PROFILE_TIMER_ID, PROFILE_TIMER_MS, None);
            } else if id == IDC_MAIN_METHOD && (code == 1 /* CBN_SELCHANGE */ || code == 8 /* CBN_CLOSEUP */) {
                let cb = GetDlgItem(hwnd, IDC_MAIN_METHOD);
                if code == 1 {
                    let cur = SendMessageW(cb, CB_GETCURSEL, 0, 0) as i32;
                    if cur >= 0 {
                        app.settings.method = cur as u32;
                        app.settings.save();
                        // Keep the auto-inject worker on the same method
                        sync_ui_to_settings(hwnd, app);
                    }
                }
                InvalidateRect(cb, std::ptr::null(), 0);
            } else if id == IDC_SELECT_BTN {
                if let Some((proc_name, proc_pid)) = process_picker_dialog(hwnd, scale, &mut app.picker_cache) {
                    let edit = GetDlgItem(hwnd, IDC_PROC_EDIT);
                    SetWindowTextW(edit, wstr(&proc_name).as_ptr());
                    update_process_info_with_pid(hwnd, app, &proc_name, proc_pid);
                    sync_ui_to_settings(hwnd, app);
                    KillTimer(hwnd, PROFILE_TIMER_ID);
                    maybe_switch_profile(hwnd, app);
                }
            } else if id == IDC_ADD {
                on_add_dll(hwnd, app);
            } else if id == IDC_CLEAR {
                let count = app.shared.lock().unwrap().modules.len();
                if count > 0 {
                    let body = format!("Remove all {} file(s) from the injection list?\r\n\r\nThis cannot be undone.", count);
                    if confirm_dialog(hwnd, scale, "Clear all files?", &body, "Clear all") {
                        list_clear(hwnd, app);
                    }
                }
            } else if id == IDC_SETTINGS {
                show_settings_dialog(hwnd, app);
                apply_scale(hwnd, app);
            } else if id == IDC_INJECT {
                on_inject_click(hwnd, app);
            }
            0
        }
        WM_APP_ROW_MENU => {
            list_row_menu(hwnd, app, wp as i32, scale);
            0
        }
        WM_APP_MOVE_ROW => {
            list_move_by(hwnd, app, wp as i32, lp as i32);
            0
        }
        WM_APP_EDIT_DELAY => {
            on_edit_row_delay(hwnd, app, wp as i32, scale);
            0
        }
        WM_APP_MOVE_ROW_TO => {
            list_move_to(hwnd, app, wp as i32, lp as i32);
            0
        }
        WM_APP_FILE_OPTIONS => {
            on_file_options(hwnd, app, wp as i32, scale);
            0
        }
        WM_APP_DELETE_ROW => {
            let lv = GetDlgItem(hwnd, IDC_LIST);
            if !lv.is_null() {
                let mut rows = selected_rows(lv);
                if !rows.is_empty() {
                    let first_sel = rows[0];
                    rows.sort_unstable_by(|a, b| b.cmp(a));
                    for r in rows {
                        list_remove_index(hwnd, app, r);
                    }
                    let count = SendMessageW(lv, LVM_GETITEMCOUNT, 0, 0) as i32;
                    if count > 0 {
                        ensure_row_selected(lv, first_sel.min(count - 1));
                    }
                }
            }
            0
        }
        WM_TIMER => {
            if wp == PROFILE_TIMER_ID {
                KillTimer(hwnd, PROFILE_TIMER_ID);
                maybe_switch_profile(hwnd, app);
                0
            } else if wp == TARGET_REFRESH_TIMER_ID {
                poll_target_process_status(hwnd, app);
                0
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_APP_LOG => {
            let ptr = wp as *mut String;
            if !ptr.is_null() {
                let s = *Box::from_raw(ptr);
                push_log(app, &s);
            }
            0
        }
        crate::update::WM_APP_UPDATE => {
            if let Some(v) = crate::update::take_found() {
                push_log(app, &format!("[+] Update available: Version {v}"));
                show_update_dialog(hwnd, scale, &v);
            }
            0
        }
        WM_APP_INJECT_DONE => {
            app.injecting = false;
            let b_inj = GetDlgItem(hwnd, IDC_INJECT);
            SetWindowTextW(b_inj, wstr("Inject").as_ptr());
            EnableWindow(b_inj, 1);
            let ptr = lp as *mut InjectDone;
            let payload = if !ptr.is_null() {
                *Box::from_raw(ptr)
            } else {
                InjectDone { ok: wp != 0, crashed: false, target_gone: false, user_error: false, message: String::new(), backend_error: String::new(), report_body: None, report_path: None }
            };
            if let Some(path) = &payload.report_path {
                let kind = if payload.crashed { "Crash" } else { "Failure" };
                push_log(app, &format!("[!] {kind} report: {path}"));
            }
            if payload.crashed {
                push_log(app, "[!] Target crashed (see report for details)");
                if let Some(body) = &payload.report_body {
                    report_dialog(hwnd, scale, "Target crashed", &payload.message, body);
                } else {
                    show_message(hwnd, scale, MsgKind::Error, "Target crashed", &payload.message, "", "E2002 TARGET_CRASHED", run_context(app));
                }
            } else if payload.target_gone {
                push_log(app, &format!("[!] {}", payload.message));
                let mut details = run_context(app);
                if !payload.backend_error.is_empty() {
                    details.push(format!("Backend message: {}", payload.backend_error));
                }
                show_message(hwnd, scale, MsgKind::Error, "Target not running", &payload.message, "The selected process exited before the DLL could be injected. Start it again and retry.", "E2003 TARGET_NOT_RUNNING", details);
            } else if payload.user_error {
                push_log(app, &format!("[!] {}", payload.message));
                let mut details = run_context(app);
                if !payload.backend_error.is_empty() {
                    details.push(format!("Backend message: {}", payload.backend_error));
                }
                show_message(hwnd, scale, MsgKind::Error, "Symbols unavailable", &payload.message, "Connect to the internet and retry. The PDB is downloaded once and cached for later injections.", "TLS-PDB-01", details);
            } else if payload.ok {
                if app.settings.close_on_inject {
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                } else {
                    show_message(hwnd, scale, MsgKind::Success, "Success", &payload.message, "", "", Vec::new());
                }
            } else if let Some(body) = &payload.report_body {
                push_log(app, &format!("[!] {}", payload.message));
                report_dialog(hwnd, scale, "Injection failed", &payload.message, body);
            } else {
                let mut details = run_context(app);
                details.push(format!("Backend message: {}", payload.message));
                show_message(hwnd, scale, MsgKind::Error, "Failed", &payload.message, "Copy the error code if you need to report this.", "E2001 INJECT_FAILED", details);
            }
            0
        }
        WM_CLOSE => {
            sync_ui_to_settings(hwnd, app);
            remember_current_profile(app);
            app.settings.save();
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TARGET_REFRESH_TIMER_ID);
            if let Some(hicon) = app.proc_icon.take() {
                DestroyIcon(hicon);
            }
            for f in [app.hfont, app.hfont_bold, app.hfont_small, app.hfont_italic, app.hfont_btn, app.hfont_title] {
                if !f.is_null() {
                    gdi32::DeleteObject(f);
                }
            }
            for b in [app.bg_brush, app.panel_brush, app.input_brush] {
                gdi32::DeleteObject(b);
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(app_ptr));
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ===== Context Menu =====
// Right-clicks used the system menu, which is white and ignores the dark palette. This popup
// uses the same palette as the dropdown lists and captures the mouse while it's open.
const SM_CXSCREEN_C: i32 = 0;

struct MenuState {
    rows: Vec<(String, u32)>,
    hover: i32,
    result: u32,
    done: bool,
    item_h: i32,
    sep_h: i32,
    pad: i32,
    scale: u32,
    font: HFONT,
    sub_rows: Vec<(String, u32)>,
    sub_open: bool,
    sub_hover: i32,
    main_w: i32,
    sub_w: i32,
    gap: i32,
    sub_top: i32,
    height: i32,
    left: i32,
    top: i32,
}

unsafe fn menu_row_height(st: &MenuState, text: &str) -> i32 {
    if text.is_empty() { st.sep_h } else { st.item_h }
}

// Row under a y position in client coordinates, or -1 for separators and padding
unsafe fn menu_row_at(st: &MenuState, y: i32) -> i32 {
    let mut top = st.pad;
    for (i, (text, _)) in st.rows.iter().enumerate() {
        let h = menu_row_height(st, text);
        if y >= top && y < top + h {
            return if text.is_empty() { -1 } else { i as i32 };
        }
        top += h;
    }
    -1
}

// Moves the keyboard highlight to the next or previous real item, skipping separators
unsafe fn menu_step(st: &mut MenuState, dir: i32) {
    let n = st.rows.len() as i32;
    let mut i = st.hover;
    loop {
        i += dir;
        if i < 0 || i >= n {
            return;
        }
        if !st.rows[i as usize].0.is_empty() {
            st.hover = i;
            return;
        }
    }
}

unsafe fn menu_sub_row_at(st: &MenuState, y: i32) -> i32 {
    let mut top = st.sub_top;
    for (i, (text, _)) in st.sub_rows.iter().enumerate() {
        if y >= top && y < top + st.item_h && !text.is_empty() {
            return i as i32;
        }
        top += st.item_h;
    }
    -1
}

unsafe fn open_menu_submenu(hwnd: HWND, st: &mut MenuState) {
    if st.sub_open || st.sub_rows.is_empty() {
        return;
    }
    st.sub_open = true;
    let new_w = st.main_w + st.gap + st.sub_w;
    let r = s(R_INPUT, st.scale);
    let main_rgn = CreateRoundRectRgn(0, 0, st.main_w, st.height, r * 2, r * 2);
    let sub_top = (st.sub_top - st.pad).max(0);
    let sub_h = st.sub_rows.len() as i32 * st.item_h + st.pad * 2 + s(2, st.scale);
    let sub_rgn = CreateRoundRectRgn(st.main_w + st.gap, sub_top, new_w, (sub_top + sub_h + 1).min(st.height), r * 2, r * 2);
    let combined = CreateRectRgn(0, 0, 0, 0);
    CombineRgn(combined, main_rgn, sub_rgn, 2 /* RGN_OR */);
    SetWindowRgn(hwnd, combined, 1);
    let _ = gdi32::DeleteObject(main_rgn);
    let _ = gdi32::DeleteObject(sub_rgn);
    SetWindowPos(hwnd, (-1isize) as HWND, st.left, st.top, new_w, st.height, 0x0004 | 0x0010);
    InvalidateRect(hwnd, std::ptr::null(), 0);
}

unsafe fn close_menu_submenu(hwnd: HWND, st: &mut MenuState) {
    if !st.sub_open {
        return;
    }
    st.sub_open = false;
    st.sub_hover = -1;
    round_window(hwnd, st.main_w, st.height, s(R_INPUT, st.scale));
    SetWindowPos(hwnd, (-1isize) as HWND, st.left, st.top, st.main_w, st.height, 0x0004 | 0x0010);
    InvalidateRect(hwnd, std::ptr::null(), 0);
}
unsafe fn paint_menu(hwnd: HWND, hdc: HDC, st: &MenuState) {
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right, rc.bottom);
    let r = s(R_INPUT, st.scale);
    let main_w = if st.main_w > 0 { st.main_w.min(w) } else { w };

    let mem_dc = gdi32::CreateCompatibleDC(hdc);
    let mem_bm = gdi32::CreateCompatibleBitmap(hdc, w, h);
    let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

    let page = gdi32::CreateSolidBrush(C_BG);
    gdi32::FillRect(mem_dc, &rc, page);
    let _ = gdi32::DeleteObject(page);

    let panel_pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_INPUT_BORDER);
    let panel_brush = gdi32::CreateSolidBrush(C_INPUT_BG);
    let old_pen = gdi32::SelectObject(mem_dc, panel_pen);
    let old_brush = gdi32::SelectObject(mem_dc, panel_brush);
    round_rect(mem_dc, 0, 0, main_w - 1, h - 1, r, r);

    let row_brush = gdi32::CreateSolidBrush(C_ROW_SEL);
    gdi32::SelectObject(mem_dc, gdi32::GetStockObject(NULL_PEN_C));
    gdi32::SelectObject(mem_dc, row_brush);
    gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);
    let old_font = if !st.font.is_null() { gdi32::SelectObject(mem_dc, st.font) } else { NULL_HANDLE };

    let mut top = st.pad;
    for (i, (text, id)) in st.rows.iter().enumerate() {
        let row_h = menu_row_height(st, text);
        if text.is_empty() {
            let pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_INPUT_BORDER);
            let old = gdi32::SelectObject(mem_dc, pen);
            let mid = top + row_h / 2;
            gdi32::MoveToEx(mem_dc, st.pad + s(10, st.scale), mid, std::ptr::null_mut());
            gdi32::LineTo(mem_dc, main_w - st.pad - s(10, st.scale), mid);
            gdi32::SelectObject(mem_dc, old);
            let _ = gdi32::DeleteObject(pen);
        } else {
            let row = RECT { left: st.pad, top, right: main_w - st.pad, bottom: top + row_h };
            if i as i32 == st.hover {
                round_rect(mem_dc, row.left, row.top, row.right, row.bottom, r, r);
            }
            let has_sub = *id == MENU_SET_METHOD && !st.sub_rows.is_empty();
            let mut trc = RECT { left: st.pad + s(14, st.scale), top, right: row.right - s(8, st.scale), bottom: row.bottom };
            let wt = wstr(text);
            gdi32::SetTextColor(mem_dc, C_TEXT_BODY);
            gdi32::DrawTextW(
                mem_dc, wt.as_ptr(), -1, &mut trc,
                DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG,
            );
            if has_sub {
                let ax = row.right - s(18, st.scale);
                let cy = (top + top + row_h) / 2;
                let pen = gdi32::CreatePen(gdi32::PS_SOLID, s(2, st.scale).max(1), C_TEXT_MUTED);
                let old = gdi32::SelectObject(mem_dc, pen);
                gdi32::MoveToEx(mem_dc, ax, cy - s(5, st.scale), std::ptr::null_mut());
                gdi32::LineTo(mem_dc, ax + s(4, st.scale), cy);
                gdi32::LineTo(mem_dc, ax, cy + s(5, st.scale));
                gdi32::SelectObject(mem_dc, old);
                let _ = gdi32::DeleteObject(pen);
            }
        }
        top += row_h;
    }

    if st.sub_open {
        let sub_x = main_w + st.gap;
        let panel_top = (st.sub_top - st.pad).max(0);
        let panel_bottom = (panel_top + st.sub_rows.len() as i32 * st.item_h + st.pad * 2 + s(2, st.scale)).min(h - 1);
        gdi32::SelectObject(mem_dc, panel_brush);
        gdi32::SelectObject(mem_dc, panel_pen);
        round_rect(mem_dc, sub_x, panel_top, w - 1, panel_bottom, r, r);
        gdi32::SelectObject(mem_dc, row_brush);
        gdi32::SelectObject(mem_dc, gdi32::GetStockObject(NULL_PEN_C));
        let mut stop = st.sub_top;
        for (i, (text, _)) in st.sub_rows.iter().enumerate() {
            if text.is_empty() {
                stop += st.item_h;
                continue;
            }
            let row = RECT { left: sub_x + st.pad, top: stop, right: w - st.pad, bottom: stop + st.item_h };
            if i as i32 == st.sub_hover {
                round_rect(mem_dc, row.left, row.top, row.right, row.bottom, r, r);
            }
            let mut trc = RECT { left: row.left + s(12, st.scale), top: stop, right: row.right - s(8, st.scale), bottom: row.bottom };
            let wt = wstr(text);
            gdi32::SetTextColor(mem_dc, C_TEXT_BODY);
            gdi32::DrawTextW(mem_dc, wt.as_ptr(), -1, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
            stop += st.item_h;
        }
    }

    if !old_font.is_null() {
        gdi32::SelectObject(mem_dc, old_font);
    }
    gdi32::SelectObject(mem_dc, old_brush);
    gdi32::SelectObject(mem_dc, old_pen);
    gdi32::BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
    gdi32::SelectObject(mem_dc, old_bm);
    let _ = gdi32::DeleteObject(row_brush);
    let _ = gdi32::DeleteObject(panel_brush);
    let _ = gdi32::DeleteObject(panel_pen);
    let _ = gdi32::DeleteObject(mem_bm);
    let _ = gdi32::DeleteDC(mem_dc);
}
unsafe extern "system" fn menu_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let stp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MenuState;
    if stp.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let st = &mut *stp;
    let x = (lp & 0xFFFF) as i16 as i32;
    let y = ((lp >> 16) & 0xFFFF) as i16 as i32;
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let inside = x >= 0 && y >= 0 && x < rc.right && y < rc.bottom;

    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_menu(hwnd, hdc, st);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_MOUSEMOVE => {
            if st.sub_open && x >= st.main_w - s(10, st.scale) {
                KillTimer(hwnd, 7);
                let srow = if x >= st.main_w + st.gap { menu_sub_row_at(st, y) } else { st.sub_hover };
                if srow != st.sub_hover {
                    st.sub_hover = srow;
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                return 0;
            }
            if !inside {
                KillTimer(hwnd, 7);
                if st.sub_open {
                    close_menu_submenu(hwnd, st);
                }
                return 0;
            }
            if x < st.main_w {
                let row = menu_row_at(st, y);
                if row >= 0 && row != st.hover {
                    st.hover = row;
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                let row_id = if row >= 0 { st.rows[row as usize].1 } else { 0 };
                if row_id == MENU_SET_METHOD && !st.sub_rows.is_empty() {
                    open_menu_submenu(hwnd, st);
                } else {
                    KillTimer(hwnd, 7);
                    if st.sub_open {
                        close_menu_submenu(hwnd, st);
                    }
                }
            } else {
                KillTimer(hwnd, 7);
                if st.sub_open {
                    close_menu_submenu(hwnd, st);
                }
            }
            return 0;
        }
        WM_TIMER => {
            if wp == 7 {
                KillTimer(hwnd, 7);
                open_menu_submenu(hwnd, st);
            }
            return 0;
        }
        WM_LBUTTONUP => {
            if inside {
                if st.sub_open && x >= st.main_w + st.gap {
                    let srow = menu_sub_row_at(st, y);
                    if srow >= 0 {
                        st.result = st.sub_rows[srow as usize].1;
                        st.done = true;
                    }
                } else if x < st.main_w {
                    let row = menu_row_at(st, y);
                    if row >= 0 {
                        let id = st.rows[row as usize].1;
                        if id == MENU_SET_METHOD && !st.sub_rows.is_empty() {
                            open_menu_submenu(hwnd, st);
                        } else {
                            st.result = id;
                            st.done = true;
                        }
                    }
                }
            }
            return 0;
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN_C => {
            // A click outside the menu dismisses it and is not passed on
            if !inside {
                st.done = true;
            }
            return 0;
        }
        WM_CAPTURECHANGED_C => {
            st.done = true;
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

// Clips a popup window to a rounded rectangle, so its corners are transparent instead of showing
// the square window behind them. The region is owned by the window once it's set.
unsafe fn round_window(hwnd: HWND, width: i32, height: i32, radius: i32) {
    let rgn = CreateRoundRectRgn(0, 0, width, height, radius * 2, radius * 2);
    SetWindowRgn(hwnd, rgn, 1);
}

// Shows the menu at a screen position and runs until something is picked or dismissed.
// Returns the chosen id, or 0 if the menu was dismissed.
unsafe fn show_context_menu(owner: HWND, rows: &[(&str, u32)], x: i32, y: i32, scale: u32, font: HFONT) -> u32 {
    show_context_menu_sub(owner, rows, &[], x, y, scale, font)
}

unsafe fn show_context_menu_sub(
    owner: HWND,
    rows: &[(&str, u32)],
    sub_rows: &[(&str, u32)],
    x: i32,
    y: i32,
    scale: u32,
    font: HFONT,
) -> u32 {
    let mut st = MenuState {
        rows: rows.iter().map(|(text, id)| (text.to_string(), *id)).collect(),
        hover: -1,
        result: 0,
        done: false,
        item_h: s(30, scale),
        sep_h: s(9, scale),
        pad: s(4, scale),
        scale,
        font,
        sub_rows: sub_rows.iter().map(|(text, id)| (text.to_string(), *id)).collect(),
        sub_open: false,
        sub_hover: -1,
        main_w: 0,
        sub_w: 0,
        gap: s(6, scale),
        sub_top: 0,
        height: 0,
        left: 0,
        top: 0,
    };

    let hdc = GetDC(owner);
    let old_font = if !font.is_null() { gdi32::SelectObject(hdc, font) } else { NULL_HANDLE };
    let mut widest = 0;
    for (text, _) in &st.rows {
        if text.is_empty() { continue; }
        let wt = wstr(text);
        let mut sz = SIZE::default();
        gdi32::GetTextExtentPoint32W(hdc, wt.as_ptr(), (wt.len() - 1) as i32, &mut sz);
        widest = widest.max(sz.cx);
    }
    let mut sub_widest = 0;
    for (text, _) in &st.sub_rows {
        let wt = wstr(text);
        let mut sz = SIZE::default();
        gdi32::GetTextExtentPoint32W(hdc, wt.as_ptr(), (wt.len() - 1) as i32, &mut sz);
        sub_widest = sub_widest.max(sz.cx);
    }
    if !old_font.is_null() { gdi32::SelectObject(hdc, old_font); }
    ReleaseDC(owner, hdc);

    let width = (widest + s(48, scale)).max(s(170, scale));
    let sub_w = if st.sub_rows.is_empty() { 0 } else { (sub_widest + s(36, scale)).max(s(190, scale)) };
    let height = st.pad * 2 + st.rows.iter().map(|(text, _)| menu_row_height(&st, text)).sum::<i32>();

    // The submenu starts at the "Set injection method..." row
    let mut row_top = st.pad;
    for (text, id) in &st.rows {
        if *id == MENU_SET_METHOD {
            st.sub_top = row_top;
        }
        row_top += menu_row_height(&st, text);
    }

    st.main_w = width;
    st.sub_w = sub_w;
    st.height = height;
    st.left = if x + width + st.gap + sub_w > GetSystemMetrics(SM_CXSCREEN_C) { (x - width).max(0) } else { x };
    st.top = if y + height > GetSystemMetrics(SM_CYSCREEN_C) { (y - height).max(0) } else { y };

    let hinst = get_module_handle();
    let cls = wstr("AppContextMenu");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(menu_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW_C | WS_EX_TOPMOST as u32, cls.as_ptr(), wstr("").as_ptr(), WS_POPUP,
        st.left, st.top, width, height, NULL_HANDLE, NULL_HANDLE, hinst, std::ptr::null_mut(),
    );
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut st as *mut MenuState as LONG_PTR);
    round_window(hwnd, width, height, s(R_INPUT, scale));
    SetWindowPos(hwnd, HWND_TOPMOST_C, st.left, st.top, width, height, SWP_SHOWWINDOW_C | SWP_NOACTIVATE_FLAG);
    UpdateWindow(hwnd);
    SetCapture(hwnd);

    let stp = &mut st as *mut MenuState;
    let mut msg = new_msg();
    while !(*stp).done && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        if msg.message == WM_KEYDOWN {
            match msg.wParam {
                VK_ESCAPE_C => {
                    (*stp).done = true;
                    continue;
                }
                VK_UP_C | VK_DOWN_C => {
                    menu_step(&mut *stp, if msg.wParam == VK_UP_C { -1 } else { 1 });
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                    continue;
                }
                VK_RETURN_C => {
                    let hovered = (*stp).hover;
                    if hovered >= 0 {
                        (*stp).result = (&*stp).rows[hovered as usize].1;
                    }
                    (*stp).done = true;
                    continue;
                }
                _ => {}
            }
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    ReleaseCapture();
    DestroyWindow(hwnd);
    (*stp).result
}

unsafe fn handle_drop_files(hwnd: HWND, drop: HANDLE, app: &mut App) {
    let count = shell32::DragQueryFileW(drop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
    for i in 0..count {
        let mut buf = [0u16; 1024];
        let len = shell32::DragQueryFileW(drop, i, buf.as_mut_ptr(), 1024);
        if len == 0 { continue; }
        let path = from_wsz(buf.as_ptr());
        if !path.to_lowercase().ends_with(".dll") { continue; }
        if app.shared.lock().unwrap().modules.iter().any(|m| m.path.eq_ignore_ascii_case(&path)) {
            continue;
        }
        list_add(hwnd, &Module::new(&path));
    }
    shell32::DragFinish(drop);
    sync_ui_to_settings(hwnd, app);
    app.settings.save();
    let scale = resolve_scale(app.settings.ui_scale);
    auto_fit_module_columns(hwnd, app, scale);
    InvalidateRect(GetDlgItem(hwnd, IDC_LIST), std::ptr::null(), 0);
}

// ===== Add DLL Dialog =====
#[repr(C)]
struct OpenFileNameW64 {
    lStructSize: u32,
    hwndOwner: HWND,
    hInstance: HINSTANCE,
    lpstrFilter: *const u16,
    lpstrCustomFilter: *mut u16,
    nMaxCustFilter: u32,
    nFilterIndex: u32,
    lpstrFile: *mut u16,
    nMaxFile: u32,
    lpstrFileTitle: *mut u16,
    nMaxFileTitle: u32,
    lpstrInitialDir: *const u16,
    lpstrTitle: *const u16,
    Flags: u32,
    nFileOffset: u16,
    nFileExtension: u16,
    lpstrDefExt: *const u16,
    lCustData: LPARAM,
    lpfnHook: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> usize>,
    lpTemplateName: *const u16,
    pvReserved: *mut c_void_raw,
    dwReserved: u32,
    FlagsEx: u32,
}

unsafe fn on_add_dll(hwnd: HWND, app: &mut App) {
    let mut file_buf = vec![0u16; 32768];
    let filter = wstr("Dynamic Link Libraries (*.dll)\0*.dll\0All Files (*.*)\0*.*\0\0");
    let title = wstr("Select DLL(s) to Inject");

    let mut ofn: OpenFileNameW64 = std::mem::zeroed();
    ofn.lStructSize = std::mem::size_of::<OpenFileNameW64>() as u32; // 152 bytes for x64
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = 0x00080000 | 0x00001000 | 0x00000800 | 0x00000200; // EXPLORER | FILEMUSTEXIST | PATHMUSTEXIST | ALLOWMULTISELECT

    if GetOpenFileNameW(&mut ofn as *mut _ as *mut _) != 0 {
        let mut offset = 0;
        let mut strings = Vec::new();
        while offset < file_buf.len() && file_buf[offset] != 0 {
            let s = from_wsz(file_buf[offset..].as_ptr());
            offset += s.encode_utf16().count() + 1;
            strings.push(s);
        }

        if strings.len() == 1 {
            let path = &strings[0];
            if !app.shared.lock().unwrap().modules.iter().any(|m| m.path.eq_ignore_ascii_case(path)) {
                list_add(hwnd, &Module::new(path));
            }
        } else if strings.len() > 1 {
            let dir = &strings[0];
            for file in &strings[1..] {
                let full = format!("{}\\{}", dir.trim_end_matches('\\'), file);
                if !app.shared.lock().unwrap().modules.iter().any(|m| m.path.eq_ignore_ascii_case(&full)) {
                    list_add(hwnd, &Module::new(&full));
                }
            }
        }
        sync_ui_to_settings(hwnd, app);
        app.settings.save();
        let scale = resolve_scale(app.settings.ui_scale);
        auto_fit_module_columns(hwnd, app, scale);
        InvalidateRect(GetDlgItem(hwnd, IDC_LIST), std::ptr::null(), 0);
    }
}

// ===== Inject Button Action =====
// The options the injector runs with come from the settings (see `Settings::inject_options`),
// so the Inject button and the auto-inject worker always inject the same way.
unsafe fn on_inject_click(hwnd: HWND, app: &mut App) {
    if app.injecting { return; }
    let scale = resolve_scale(app.settings.ui_scale);
    sync_ui_to_settings(hwnd, app);
    remember_current_profile(app);

    let pid = app.shared.lock().unwrap().target_pid;
    let proc_name = app.shared.lock().unwrap().process_name.clone();

    let final_pid = if pid > 0 && is_pid_alive_matching(pid, &proc_name) {
        pid
    } else if !proc_name.is_empty() {
        if let Some(p) = crate::process::find_by_name(&proc_name) {
            app.shared.lock().unwrap().target_pid = p.pid;
            p.pid
        } else {
            show_message(hwnd, scale, MsgKind::Error, "Error", &format!("Process '{}' is not running.", proc_name), "Start the process, or pick it again with Select Process.", "E1002 PROCESS_NOT_RUNNING", run_context(app));
            return;
        }
    } else {
        show_message(hwnd, scale, MsgKind::Error, "Error", "No target process is set.", "Pick one with Select Process, then press Execute again.", "E1001 NO_TARGET", run_context(app));
        return;
    };

    let modules: Vec<Module> = app.shared.lock().unwrap().modules.clone();
    if modules.iter().filter(|m| m.enabled).count() == 0 {
        push_log(app, "[!] No items checked: enable at least one DLL");
        show_message(hwnd, scale, MsgKind::Warning, "Warning", "No items are checked.", "Tick at least one file in the list to inject it.", "E1003 NO_ITEMS", run_context(app));
        return;
    }

    // Check the queue before touching the target: missing files, wrong architecture, a module
    // that is already loaded, missing VC++ runtime. Fatal problems abort with a clear reason.
    let problems = crate::queue::validate(final_pid, &modules);
    for w in problems.iter().filter(|p| !p.fatal) {
        push_log(app, &format!("[!] {}", w.line()));
    }
    let fatal: Vec<String> = problems.iter().filter(|p| p.fatal).map(|p| p.line()).collect();
    if !fatal.is_empty() {
        for line in &fatal {
            push_log(app, &format!("[!] {line}"));
        }
        push_log(app, "[!] Injection aborted: invalid queue items");
        show_message(
            hwnd,
            scale,
            MsgKind::Error,
            "Cannot inject yet",
            "The queue has problems that would fail.",
            "Fix the listed items, then press Execute again.",
            "E1005 VALIDATION",
            fatal,
        );
        return;
    }

    // Check if any queued DLL needs the VC++ runtime and it is not installed
    if let Some((vc_arch, missing_deps)) = crate::queue::check_missing_vcredist(final_pid, &modules) {
        let arch_str = match vc_arch {
            crate::process::Arch::X64 => "x64 (64-bit)",
            crate::process::Arch::X86 => "x86 (32-bit)",
        };
        let dep_list = missing_deps.join(", ");
        let msg = format!(
            "The queued DLL requires the Microsoft Visual C++ 2015-2022 {} runtime ({}), which is not installed on this PC.\r\n\r\nWithout it, injection will fail with error 0xC0000135 (DLL not found).",
            arch_str, dep_list
        );
        let hint = "Would you like BlackBox to automatically download and install it now?";

        let should_install = show_confirm_dialog(
            hwnd,
            scale,
            "Visual C++ Runtime Required",
            &msg,
            hint,
            "Install Now",
            "Cancel",
        );

        if should_install {
            let res = crate::vcredist::install_interactive_for_arch(vc_arch, |line| {
                push_log(app, line);
            });
            if res.is_err() {
                push_log(app, "[!] Injection halted due to missing runtime. Run installer or fix the error above, then click Inject again.");
                return;
            }
        } else {
            push_log(app, "[-] Skipped VC++ runtime installation by user.");
            push_log(app, "[!] Note: MSVC-built DLLs may fail to load with 0xC0000135 if dependencies are missing.");
        }
    }

    let qopts = crate::queue::QueueOptions {
        start_delay: app.settings.delay,
        between_delay: app.settings.delay_between,
        base: app.settings.inject_options(),
        retries: 0,
    };

    app.injecting = true;
    let b_inj = GetDlgItem(hwnd, IDC_INJECT);
    SetWindowTextW(b_inj, wstr("Injecting...").as_ptr());
    EnableWindow(b_inj, 0);

    let hwnd_usize = hwnd as usize;
    let target_name = {
        let name = app.shared.lock().unwrap().process_name.clone();
        if name.is_empty() {
            crate::process::enumerate()
                .into_iter()
                .find(|p| p.pid == final_pid)
                .map(|p| p.name)
                .unwrap_or_default()
        } else {
            name
        }
    };
    let arch_name = crate::process::process_arch_quick(final_pid)
        .map(crate::diag::arch_name)
        .unwrap_or("unknown")
        .to_string();
    let method_name = Settings::METHODS
        .get(qopts.base.method as usize)
        .copied()
        .unwrap_or("unknown")
        .to_string();
    let on_off = |v: bool| if v { "on" } else { "off" };
    let header = vec![
        format!("method: {} - {}", qopts.base.method, method_name),
        format!("global delays: start {} ms, between {} ms", qopts.start_delay, qopts.between_delay),
        format!(
            "options: erase-pe={} hide-module={} stealth={} hide-debugger={} manual-imports={} disable-seh={}",
            on_off(qopts.base.erase_pe),
            on_off(qopts.base.hide_module),
            on_off(qopts.base.stealth_inject),
            on_off(qopts.base.hide_from_debugger),
            on_off(qopts.base.manual_resolve_imports),
            on_off(qopts.base.disable_seh_validation),
        ),
        format!("module count: {}", modules.iter().filter(|m| m.enabled).count()),
    ];
    let queue_lines: Vec<String> = modules
        .iter()
        .filter(|m| m.enabled)
        .enumerate()
        .map(|(i, m)| {
            format!(
                "#{} {} | method {} | wait {} ms | after {} ms | wait-for {} | ignore-errors {}",
                i + 1,
                m.path,
                m.method.map(|v| v.to_string()).unwrap_or_else(|| "global".to_string()),
                m.delay_ms,
                m.delay_after_ms,
                if m.wait_for_module.is_empty() { "-" } else { m.wait_for_module.as_str() },
                on_off(m.ignore_errors)
            )
        })
        .collect();

    std::thread::spawn(move || {
        let hwnd = hwnd_usize as HWND;
        let logger = |msg: &str| unsafe { post_ui_log(hwnd, msg) };
        let result = crate::queue::run(final_pid, &modules, &qopts, &logger);

        let mut result_lines: Vec<String> = result.verifications.iter().map(crate::diag::verification_line).collect();
        for (p, e) in &result.failed {
            result_lines.push(format!("FAILED {}: {}", p, e));
        }
        if let Some(c) = &result.crash {
            result_lines.push(format!("CRASHED with {} after about {} ms", crate::diag::exit_text(c.exit_code), c.after_ms));
        }

        // Attribute the crash to a module and explain the likely cause
        let injected: Vec<(String, u32)> = modules
            .iter()
            .filter(|m| m.enabled)
            .map(|m| (m.path.clone(), m.effective_method(qopts.base.method)))
            .collect();
        let diagnosis = result
            .crash
            .as_ref()
            .map(|c| crate::diag::diagnose(c, &target_name, &injected))
            .unwrap_or_default();
        for hint in &diagnosis {
            unsafe {
                post_ui_log(hwnd, &format!("[!] Diagnosis: {hint}"));
            }
        }

        let report = crate::diag::Report {
            target: &target_name,
            pid: final_pid,
            arch: &arch_name,
            header,
            queue: queue_lines,
            results: result_lines,
            crash: result.crash.clone(),
            diagnosis,
        };
        let crashed = result.crash.is_some();
        let failed = !result.failed.is_empty();
        // A dead target or a missing ntdll symbol cache is a user/environment problem, not an
        // injector defect. Show the existing error dialog, but do not write a failure report.
        let report_out = if (crashed || failed) && !result.target_gone && !result.user_error {
            crate::diag::write_report(&report)
        } else {
            None
        };
        let ok = !failed && !crashed;
        let res_msg = if crashed {
            format!(
                "The target crashed ({}) after the injection. Full details are in the report.",
                crate::diag::exit_text(result.crash.as_ref().map(|c| c.exit_code).unwrap_or(0))
            )
        } else if result.target_gone {
            "The target process is not running. Start it again, select it, and retry.".to_string()
        } else if result.user_error {
            "Static TLS needs the ntdll symbol file for this Windows build, but it is not cached and could not be downloaded.".to_string()
        } else if ok {
            let verified = result.verifications.iter().filter(|v| v.listed).count();
            if verified == result.injected {
                format!("Successfully injected {} module(s) into PID {} and verified them in the target.", result.injected, final_pid)
            } else {
                format!("Successfully injected {} module(s) into PID {}.", result.injected, final_pid)
            }
        } else {
            result
                .failed
                .first()
                .map(|(p, e)| format!("Failed to inject '{}': {}", p, e))
                .unwrap_or_else(|| "Injection failed".to_string())
        };

        let payload = InjectDone {
            ok,
            crashed,
            target_gone: result.target_gone,
            user_error: result.user_error,
            message: res_msg,
            backend_error: result
                .failed
                .first()
                .map(|(_, e)| e.clone())
                .unwrap_or_default(),
            report_body: report_out.as_ref().map(|r| r.body.clone()),
            report_path: report_out.as_ref().map(|r| r.path.clone()),
        };
        PostMessageW(hwnd, WM_APP_INJECT_DONE, ok as WPARAM, Box::into_raw(Box::new(payload)) as LPARAM);
    });
}
// ===== ListView Helpers =====
// Adds the row to the ListView only. The saved list is already in Shared when the window is
// built, so those rows must not be pushed a second time.
unsafe fn list_insert_row(hwnd: HWND, module: &Module) -> i32 {
    let lv = GetDlgItem(hwnd, IDC_LIST);
    if lv.is_null() { return -1; }
    let count = SendMessageW(lv, LVM_GETITEMCOUNT, 0, 0) as i32;
    let filename = module.file_name();
    let wname = wstr(&filename);
    let mut item = LVITEMW {
        mask: LVIF_TEXT_C,
        iItem: count,
        iSubItem: 0,
        pszText: wname.as_ptr() as *mut u16,
        ..Default::default()
    };
    SendMessageW(lv, LVM_INSERTITEMW, 0, &mut item as *mut _ as LPARAM) as i32
}

// Appends one module to the list and to the shared injection list. The list order is
// the injection order, so appending puts the file at the end of the sequence. A file
// that is already listed is ignored, which keeps the rows and the shared list 1:1.
unsafe fn list_add(hwnd: HWND, module: &Module) {
    let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if app_ptr.is_null() { return; }
    if (*app_ptr).shared.lock().unwrap().modules.iter().any(|m| m.path.eq_ignore_ascii_case(&module.path)) {
        return;
    }
    if list_insert_row(hwnd, module) >= 0 {
        (*app_ptr).shared.lock().unwrap().modules.push(module.clone());
    }
}

// Rewrites the text a row stores for column 0. Only the file name is stored; everything
// else is drawn from the shared list.
unsafe fn set_lv_row_text(lv: HWND, row: i32, text: &str) {
    let w = wstr(text);
    let mut item = LVITEMW {
        mask: LVIF_TEXT_C,
        iItem: row,
        iSubItem: 0,
        pszText: w.as_ptr() as *mut u16,
        ..Default::default()
    };
    SendMessageW(lv, LVM_SETITEMW, 0, &mut item as *mut _ as LPARAM);
}

unsafe fn list_remove_index(hwnd: HWND, app: &mut App, idx: i32) {
    if idx < 0 { return; }
    let lv = GetDlgItem(hwnd, IDC_LIST);
    SendMessageW(lv, LVM_DELETEITEM, idx as WPARAM, 0);

    let mut sh = app.shared.lock().unwrap();
    if (idx as usize) < sh.modules.len() {
        sh.modules.remove(idx as usize);
    }
    drop(sh);
    sync_ui_to_settings(hwnd, app);
    app.settings.save();
    let scale = resolve_scale(app.settings.ui_scale);
    auto_fit_module_columns(hwnd, app, scale);
    InvalidateRect(lv, std::ptr::null(), 0);
}

unsafe fn list_clear(hwnd: HWND, app: &mut App) {
    let lv = GetDlgItem(hwnd, IDC_LIST);
    SendMessageW(lv, LVM_DELETEALLITEMS, 0, 0);
    app.shared.lock().unwrap().modules.clear();
    sync_ui_to_settings(hwnd, app);
    app.settings.save();
    let scale = resolve_scale(app.settings.ui_scale);
    auto_fit_module_columns(hwnd, app, scale);
    InvalidateRect(lv, std::ptr::null(), 0);
}

// Moves the file at `row` by `delta` places, which changes when it is injected.
unsafe fn list_move_by(hwnd: HWND, app: &mut App, row: i32, delta: i32) {
    if delta == 0 { return; }
    let lv = GetDlgItem(hwnd, IDC_LIST);
    let count = SendMessageW(lv, LVM_GETITEMCOUNT, 0, 0) as i32;
    let to = row + delta;
    if row < 0 || row >= count || to < 0 || to >= count { return; }

    let (moved_name, other_name) = {
        let mut sh = app.shared.lock().unwrap();
        sh.modules.swap(row as usize, to as usize);
        // After the swap the moved file is at `to` and the displaced one at `row`
        (sh.modules[to as usize].file_name(), sh.modules[row as usize].file_name())
    };
    set_lv_row_text(lv, to, &moved_name);
    set_lv_row_text(lv, row, &other_name);

    // Keep the moved file selected and scrolled into view
    let mut sel = LVITEMW {
        state: LVIS_SELECTED,
        stateMask: LVIS_SELECTED,
        iItem: to,
        ..Default::default()
    };
    SendMessageW(lv, LVM_SETITEMSTATE, to as WPARAM, &mut sel as *mut _ as LPARAM);
    let mut focus = LVITEMW {
        state: 0x0001 /* LVIS_FOCUSED */,
        stateMask: 0x0001,
        iItem: to,
        ..Default::default()
    };
    SendMessageW(lv, LVM_SETITEMSTATE, to as WPARAM, &mut focus as *mut _ as LPARAM);
    SendMessageW(lv, 0x1000 + 13 /* LVM_ENSUREVISIBLE */, to as WPARAM, 0);

    InvalidateRect(lv, std::ptr::null(), 0);
    sync_ui_to_settings(hwnd, app);
    app.settings.save();
}

// Stores one file's own delay (0 = fall back to the global delays).
unsafe fn set_row_delay(hwnd: HWND, app: &mut App, row: i32, delay_ms: u32) {
    {
        let mut sh = app.shared.lock().unwrap();
        match sh.modules.get_mut(row as usize) {
            Some(m) => m.delay_ms = delay_ms.min(MAX_MODULE_DELAY_MS),
            None => return,
        }
    }
    InvalidateRect(GetDlgItem(hwnd, IDC_LIST), std::ptr::null(), 0);
    sync_ui_to_settings(hwnd, app);
    app.settings.save();
    let scale = resolve_scale(app.settings.ui_scale);
    auto_fit_module_columns(hwnd, app, scale);
}

// Opens the delay editor for one row and stores what it returns.
unsafe fn on_edit_row_delay(hwnd: HWND, app: &mut App, row: i32, scale: u32) {
    let (name, current) = {
        let sh = app.shared.lock().unwrap();
        match sh.modules.get(row as usize) {
            Some(m) => (m.file_name(), m.delay_ms),
            None => return,
        }
    };
    if let Some(ms) = delay_dialog(hwnd, &name, current, scale) {
        set_row_delay(hwnd, app, row, ms);
    }
}

// Context menu for one list row. A right click on the row and a click on its dots both come here.
unsafe fn selected_rows(lv: HWND) -> Vec<i32> {
    let mut out = Vec::new();
    let mut idx = SendMessageW(lv, LVM_GETNEXTITEM, (-1isize) as WPARAM, LVNI_SELECTED_C as LPARAM) as i32;
    while idx >= 0 && out.len() < 1024 {
        out.push(idx);
        idx = SendMessageW(lv, LVM_GETNEXTITEM, idx as WPARAM, LVNI_SELECTED_C as LPARAM) as i32;
    }
    out
}

unsafe fn ensure_row_selected(lv: HWND, row: i32) {
    let state = SendMessageW(lv, LVM_FIRST_ID + 44 /* LVM_GETITEMSTATE */, row as WPARAM, 0x0002 /* LVIS_SELECTED */) as u32;
    if state & 0x0002 == 0 {
        let mut clear = LVITEMW { state: 0, stateMask: 0x0002, iItem: -1, ..Default::default() };
        SendMessageW(lv, LVM_SETITEMSTATE, (-1isize) as WPARAM, &mut clear as *mut _ as LPARAM);
        let mut sel = LVITEMW { state: 0x0003 /* SELECTED | FOCUSED */, stateMask: 0x0003, iItem: row, ..Default::default() };
        SendMessageW(lv, LVM_SETITEMSTATE, row as WPARAM, &mut sel as *mut _ as LPARAM);
        InvalidateRect(lv, std::ptr::null(), 0);
    }
}

/// Rebuilds every row from the shared list (used after a drag & drop move).
unsafe fn rebuild_list_rows(hwnd: HWND) {
    let lv = GetDlgItem(hwnd, IDC_LIST);
    if lv.is_null() { return; }
    SendMessageW(lv, LVM_DELETEALLITEMS, 0, 0);
    let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if app_ptr.is_null() { return; }
    let modules = (*app_ptr).shared.lock().unwrap().modules.clone();
    for m in &modules {
        list_insert_row(hwnd, m);
    }
    InvalidateRect(lv, std::ptr::null(), 0);
}

/// Moves the row `from` to position `to` (drag & drop).
unsafe fn list_move_to(hwnd: HWND, app: &mut App, from: i32, to: i32) {
    let lv = GetDlgItem(hwnd, IDC_LIST);
    let count = SendMessageW(lv, LVM_GETITEMCOUNT, 0, 0) as i32;
    if from < 0 || to < 0 || from >= count || to >= count || from == to { return; }
    {
        let mut sh = app.shared.lock().unwrap();
        let item = sh.modules.remove(from as usize);
        sh.modules.insert(to as usize, item);
    }
    rebuild_list_rows(hwnd);
    let mut sel = LVITEMW { state: 0x0003, stateMask: 0x0003, iItem: to, ..Default::default() };
    SendMessageW(lv, LVM_SETITEMSTATE, to as WPARAM, &mut sel as *mut _ as LPARAM);
    SendMessageW(lv, 0x1000 + 13 /* LVM_ENSUREVISIBLE */, to as WPARAM, 0);
    sync_ui_to_settings(hwnd, app);
    app.settings.save();
}

/// The pid the row actions operate on: the current target, re-resolved by name when needed.
unsafe fn resolve_target_pid(app: &App) -> Option<u32> {
    let (pid, name) = {
        let sh = app.shared.lock().unwrap();
        (sh.target_pid, sh.process_name.clone())
    };
    if pid != 0 {
        let h = kernel32::OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !h.is_null() {
            let _ = kernel32::CloseHandle(h);
            return Some(pid);
        }
    }
    if name.is_empty() {
        return None;
    }
    crate::process::find_by_name(&name).map(|p| p.pid)
}

/// Unloads the selected files from the target on a worker thread (GUI stays responsive).
unsafe fn unload_rows(hwnd: HWND, app: &mut App, rows: &[i32], wipe: bool) {
    let names: Vec<String> = {
        let sh = app.shared.lock().unwrap();
        rows.iter().filter_map(|&r| sh.modules.get(r as usize).map(|m| m.file_name())).collect()
    };
    if names.is_empty() { return; }
    let scale = resolve_scale(app.settings.ui_scale);
    let Some(pid) = resolve_target_pid(app) else {
        show_message(
            hwnd,
            scale,
            MsgKind::Error,
            "Unload failed",
            "No target process is running.",
            "Pick the process with Select Process, then try again.",
            "E3001 NO_TARGET",
            Vec::new(),
        );
        return;
    };
    let hwnd_num = hwnd as usize;
    std::thread::spawn(move || {
        let hwnd = hwnd_num as HWND;
        for name in names {
            post_ui_log(hwnd, &format!("[*] Unloading {name}..."));
            let logger = |line: &str| post_ui_log(hwnd, line);
            match crate::queue::unload(pid, &name, wipe, &logger) {
                Ok(()) => post_ui_log(hwnd, &format!("[+] Unloaded {name}")),
                Err(e) => post_ui_log(hwnd, &format!("[!] Could not unload {name}: {e}")),
            }
        }
    });
}

unsafe fn list_row_menu(hwnd: HWND, app: &mut App, row: i32, scale: u32) {
    if row < 0 { return; }
    let lv = GetDlgItem(hwnd, IDC_LIST);
    let count = SendMessageW(lv, LVM_GETITEMCOUNT, 0, 0) as i32;
    if row >= count { return; }
    ensure_row_selected(lv, row);
    let mut rows = selected_rows(lv);
    if rows.is_empty() { rows.push(row); }
    let first = rows[0];
    let n = rows.len();
    let many = n > 1;

    let mut pt = POINT::default();
    GetCursorPos(&mut pt);
    // Offer the opposite of what the row is now: enabled files can be disabled, and vice versa
    let enabled = {
        let sh = app.shared.lock().unwrap();
        sh.modules.get(first as usize).map(|m| m.enabled).unwrap_or(true)
    };
    let toggle_label = if many {
        format!("{} {} files", if enabled { "Disable" } else { "Enable" }, n)
    } else {
        (if enabled { "Disable" } else { "Enable" }).to_string()
    };
    let remove_label = if many { format!("Remove {n} files") } else { "Remove".to_string() };
    let unload_label = if many { format!("Unload {n} from target") } else { "Unload from target".to_string() };
    let unload_wipe_label = if many { format!("Unload {n} + erase headers") } else { "Unload + erase headers".to_string() };

    // An empty label is a separator
    let items: [(&str, u32); 13] = [
        (toggle_label.as_str(), MENU_ENABLED),
        ("Move up   (Ctrl+Up)", MENU_MOVE_UP),
        ("Move down   (Ctrl+Down)", MENU_MOVE_DOWN),
        ("", 0),
        ("Set injection method...", MENU_SET_METHOD),
        ("Per-file options...", MENU_FILE_OPTIONS),
        ("DLL details (imports)...", MENU_DLL_DETAILS),
        ("", 0),
        (unload_label.as_str(), MENU_UNLOAD),
        (unload_wipe_label.as_str(), MENU_UNLOAD_WIPE),
        (remove_label.as_str(), MENU_REMOVE),
        ("", 0),
        ("Open File Location", MENU_OPEN_LOCATION),
    ];
    let mut method_choices: Vec<(&str, u32)> = vec![("Use global method", 601)];
    for (i, name) in Settings::METHODS.iter().enumerate() {
        method_choices.push((name, 602 + i as u32));
    }
    let cmd = show_context_menu_sub(hwnd, &items, &method_choices, pt.x, pt.y, scale, app.hfont);
    if (601..=607).contains(&cmd) {
        let method = if cmd == 601 { None } else { Some(cmd - 602) };
        let mut changed = false;
        {
            let mut sh = app.shared.lock().unwrap();
            for &r in &rows {
                if let Some(m) = sh.modules.get_mut(r as usize) {
                    m.method = method;
                    changed = true;
                }
            }
        }
        if changed {
            rebuild_list_rows(hwnd);
            sync_ui_to_settings(hwnd, app);
            app.settings.save();
        }
        return;
    }


    match cmd {
        MENU_MOVE_UP if !many => list_move_by(hwnd, app, first, -1),
        MENU_MOVE_DOWN if !many => list_move_by(hwnd, app, first, 1),
        MENU_SET_DELAY => on_edit_row_delay(hwnd, app, first, scale),
        MENU_DEFAULT_DELAY => set_row_delay(hwnd, app, first, 0),
        MENU_FILE_OPTIONS => {
            PostMessageW(hwnd, WM_APP_FILE_OPTIONS, first as WPARAM, 0);
        }
        MENU_DLL_DETAILS => {
            on_dll_details(hwnd, app, first, scale);
        }
        MENU_ENABLED => {
            let mut changed = false;
            {
                let mut sh = app.shared.lock().unwrap();
                for &r in &rows {
                    if let Some(m) = sh.modules.get_mut(r as usize) {
                        m.enabled = !m.enabled;
                        changed = true;
                    }
                }
            }
            if changed {
                InvalidateRect(lv, std::ptr::null(), 0);
                sync_ui_to_settings(hwnd, app);
                app.settings.save();
            }
        }
        MENU_UNLOAD => unload_rows(hwnd, app, &rows, false),
        MENU_UNLOAD_WIPE => unload_rows(hwnd, app, &rows, true),
        MENU_REMOVE => {
            let mut sorted = rows.clone();
            sorted.sort_unstable_by(|a, b| b.cmp(a));
            for r in sorted {
                list_remove_index(hwnd, app, r);
            }
        }
        MENU_OPEN_LOCATION => {
            let sh = app.shared.lock().unwrap();
            let idx = first as usize;
            if idx < sh.modules.len() {
                let path = sh.modules[idx].path.clone();
                drop(sh);
                let _ = std::process::Command::new("explorer.exe")
                    .arg(format!("/select,\"{}\"", path))
                    .spawn();
            }
        }
        _ => {}
    }
}

unsafe fn read_edit_text(edit: HWND) -> String {
    let mut buf = vec![0u16; 512];
    let n = GetWindowTextW(edit, buf.as_mut_ptr(), 512);
    if n > 0 {
        from_wsz(buf.as_ptr())
    } else {
        String::new()
    }
}

unsafe fn sync_ui_to_settings(hwnd: HWND, app: &mut App) {
    let edit = GetDlgItem(hwnd, IDC_PROC_EDIT);
    let mut buf = [0u16; 260];
    let len = GetWindowTextW(edit, buf.as_mut_ptr(), 260);
    app.settings.process_name = if len > 0 { from_wsz(buf.as_ptr()) } else { String::new() };

    let mut sh = app.shared.lock().unwrap();
    app.settings.modules = sh.modules.clone();
    app.settings.auto_inject = sh.auto_inject;
    app.settings.close_on_inject = sh.close_on_inject;
    app.settings.delay = sh.delay;
    app.settings.delay_between = sh.delay_between;
    app.settings.auto_wait_window = sh.auto_wait_window;
    app.settings.auto_retry = sh.auto_retry;
    app.settings.inject_children = sh.inject_children;
    app.settings.auto_child_filter = sh.auto_child_filter.clone();
    app.settings.auto_window_title = sh.auto_window_title.clone();
    // Auto-inject runs with whatever the UI currently says
    sh.opts = app.settings.inject_options();
}

// ===== Per-process profiles =====
const PROFILE_TIMER_ID: usize = 4242;
const PROFILE_TIMER_MS: u32 = 550;

/// Stores the live setup under the current target name (per-process mode only).
unsafe fn remember_current_profile(app: &mut App) {
    if app.settings.scope != SettingsScope::PerProcess {
        return;
    }
    let name = app.settings.process_name.clone();
    if name.trim().is_empty() {
        return;
    }
    app.settings.remember_profile(&name);
    app.active_profile = name;
}

/// Once the target name settles, swap in that process' remembered setup.
unsafe fn maybe_switch_profile(hwnd: HWND, app: &mut App) {
    if app.settings.scope != SettingsScope::PerProcess {
        return;
    }
    let name = app.settings.process_name.clone();
    if name.eq_ignore_ascii_case(&app.active_profile) {
        return;
    }
    // Keep what was set up for the process being left
    if !app.active_profile.is_empty() {
        app.settings.remember_profile(&app.active_profile);
    }
    app.active_profile = name.clone();
    if name.trim().is_empty() {
        return;
    }
    if app.settings.apply_profile(&name) {
        apply_profile_to_ui(hwnd, app);
    }
    app.settings.save();
}

/// Rebuilds the list and controls after a profile was applied.
unsafe fn apply_profile_to_ui(hwnd: HWND, app: &mut App) {
    let lv = GetDlgItem(hwnd, IDC_LIST);
    if !lv.is_null() {
        SendMessageW(lv, LVM_DELETEALLITEMS, 0, 0);
    }
    app.shared.lock().unwrap().modules.clear();
    for m in app.settings.modules.clone() {
        list_add(hwnd, &m);
    }

    let cb = GetDlgItem(hwnd, IDC_MAIN_METHOD);
    if !cb.is_null() {
        SendMessageW(cb, CB_SETCURSEL, app.settings.method as WPARAM, 0);
        InvalidateRect(cb, std::ptr::null(), 0);
    }

    {
        let mut sh = app.shared.lock().unwrap();
        sh.delay = app.settings.delay;
        sh.delay_between = app.settings.delay_between;
        sh.auto_inject = app.settings.auto_inject;
        sh.close_on_inject = app.settings.close_on_inject;
        sh.opts = app.settings.inject_options();
    }

    auto_fit_module_columns(hwnd, app, resolve_scale(app.settings.ui_scale));
    if !lv.is_null() {
        InvalidateRect(lv, std::ptr::null(), 0);
    }
}

// ===== Per-File Delay Editor =====
// Small modal that edits one row's own delay. Cancelling or closing it returns None, and
// the caller keeps the old value.
const IDC_DELAY_INPUT: i32 = 9101;
const BASE_DELAY_DLG_W: i32 = 380;
const BASE_DELAY_DLG_H: i32 = 194;

struct DelayDlgState {
    scale: u32,
    done: bool,
    result: Option<u32>,
    // The file name in UTF-16, for painting
    name: Vec<u16>,
    font: HFONT,
    font_bold: HFONT,
    font_small: HFONT,
}

// Edit-box background, created once like the window-class brush
static INPUT_BG_BRUSH: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

unsafe fn input_bg_brush() -> HBRUSH {
    use std::sync::atomic::Ordering;
    let cur = INPUT_BG_BRUSH.load(Ordering::Relaxed);
    if !cur.is_null() {
        return cur;
    }
    let brush = gdi32::CreateSolidBrush(C_INPUT_BG);
    INPUT_BG_BRUSH.store(brush, Ordering::Relaxed);
    brush
}


unsafe fn delay_dialog(parent: HWND, module_name: &str, current_ms: u32, scale: u32) -> Option<u32> {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxDelayDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(delay_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let font_name = wstr("Segoe UI");
    let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_small = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let client_w = s(BASE_DELAY_DLG_W, scale);
    let client_h = s(BASE_DELAY_DLG_H, scale);
    let mut wr = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;


    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let start_x = parent_rc.left + ((parent_rc.right - parent_rc.left) - win_w) / 2;
    let start_y = parent_rc.top + ((parent_rc.bottom - parent_rc.top) - win_h) / 2;

    let title_w = wstr(&crate::version::window_title("Injection Delay"));
    let hwnd = CreateWindowExW(
        0,
        cls.as_ptr(),
        title_w.as_ptr(),
        WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x,
        start_y,
        win_w,
        win_h,
        parent,
        NULL_HANDLE,
        hinst,
        std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        gdi32::DeleteObject(font);
        gdi32::DeleteObject(font_bold);
        gdi32::DeleteObject(font_small);
        return None;
    }

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let mut st = DelayDlgState {
        scale,
        done: false,
        result: None,
        name: module_name.encode_utf16().chain(std::iter::once(0)).collect(),
        font,
        font_bold,
        font_small,
    };
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut st as *mut DelayDlgState as LONG_PTR);

    // The value field, styled and sized like the timing inputs in Settings, pre-filled and
    // fully selected so typing replaces it
    let edit = mk_ex(
        hwnd,
        hinst,
        WC_EDIT_S,
        &format!("{}", current_ms),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_NUMBER_C | 0x0001 /* ES_CENTER */,
        0,
        s(260, scale),
        s(76, scale),
        s(70, scale),
        s(20, scale),
        IDC_DELAY_INPUT,
        font,
    );
    uxtheme::set_theme(edit, "DarkMode_CFD");
    subclass_edit(edit);
    SendMessageW(edit, 0x00B1 /* EM_SETSEL */, 0, -1);

    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, "Save", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_DELAY_DLG_W - 112, scale), s(146, scale), s(96, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);
    let b_cancel = mk(hwnd, hinst, WC_BUTTON_S, "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_SETTINGS_W - 16 - 120 - 8 - 96, scale), s(774, scale), s(96, scale), s(32, scale), 2, font_bold);
    subclass_button(b_cancel, false);

    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    SetFocus(edit);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        if st.done {
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    gdi32::DeleteObject(font);
    gdi32::DeleteObject(font_bold);
    st.result
}

unsafe extern "system" fn delay_dlg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let stp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DelayDlgState;
    let scale = if stp.is_null() { 100 } else { (*stp).scale };

    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let bg = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(hdc, &rc, bg);
            let _ = gdi32::DeleteObject(bg);

            if !stp.is_null() {
                let st = &*stp;
                paint_dialog_card(hdc, s(16, scale), s(12, scale), s(BASE_DELAY_DLG_W - 32, scale), s(120, scale), "PER-FILE DELAY", scale);

                gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);

                // Which file this delay belongs to
                gdi32::SelectObject(hdc, st.font_bold);
                gdi32::SetTextColor(hdc, C_TEXT_BODY);
                let mut rc_name = RECT {
                    left: s(32, scale), top: s(40, scale),
                    right: s(BASE_DELAY_DLG_W - 32, scale), bottom: s(62, scale),
                };
                gdi32::DrawTextW(hdc, st.name.as_ptr(), -1, &mut rc_name, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x00008000 /* DT_END_ELLIPSIS */ | DT_NOPREFIX_FLAG);

                // The input's container and label, styled like the timing row in Settings
                let pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_INPUT_BORDER);
                let brush = gdi32::CreateSolidBrush(C_INPUT_BG);
                let o_pen = gdi32::SelectObject(hdc, pen);
                let o_br = gdi32::SelectObject(hdc, brush);
                round_rect(hdc, s(258, scale), s(70, scale), s(332, scale), s(102, scale), s(R_INPUT, scale), s(R_INPUT, scale));
                gdi32::SelectObject(hdc, o_br);
                gdi32::SelectObject(hdc, o_pen);
                let _ = gdi32::DeleteObject(pen);
                let _ = gdi32::DeleteObject(brush);

                gdi32::SelectObject(hdc, st.font);
                gdi32::SetTextColor(hdc, C_TEXT_HEAD);
                let mut rc_lbl = RECT {
                    left: s(32, scale), top: s(70, scale),
                    right: s(246, scale), bottom: s(102, scale),
                };
                let wl = wstr("Delay before this file (ms):");
                gdi32::DrawTextW(hdc, wl.as_ptr(), -1, &mut rc_lbl, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                // What 0 means
                gdi32::SelectObject(hdc, st.font_small);
                gdi32::SetTextColor(hdc, C_TEXT_MUTED);
                let mut rc_hint = RECT {
                    left: s(32, scale), top: s(104, scale),
                    right: s(BASE_DELAY_DLG_W - 32, scale), bottom: s(124, scale),
                };
                let hint = wstr("0 uses the global start / between-DLLs delay.");
                gdi32::DrawTextW(hdc, hint.as_ptr(), -1, &mut rc_hint, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
            }

            EndPaint(hwnd, &ps);
            0
        }
        WM_CTLCOLOREDIT => {
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_INPUT_BG);
            input_bg_brush() as LRESULT
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, scale);
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if !stp.is_null() {
                if id == 1 {
                    let edit = GetDlgItem(hwnd, IDC_DELAY_INPUT);
                    let mut buf = [0u16; 16];
                    let len = GetWindowTextW(edit, buf.as_mut_ptr(), 16);
                    // An empty field means "no delay of its own"
                    let ms = if len > 0 { from_wsz(buf.as_ptr()).trim().parse::<u32>().unwrap_or(0) } else { 0 };
                    (*stp).result = Some(ms.min(MAX_MODULE_DELAY_MS));
                    (*stp).done = true;
                } else if id == 2 {
                    (*stp).done = true;
                }
            }
            0
        }
        WM_CLOSE => {
            // Same as Cancel: done, no result, and the caller keeps the old delay
            if !stp.is_null() {
                (*stp).done = true;
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ===== Scrollable text report dialog =====
const BASE_REPORT_W: i32 = 760;
const BASE_REPORT_H: i32 = 620;

struct ReportState {
    scale: u32,
    done: bool,
    heading: Vec<u16>,
    body: Vec<u16>,
    font_bold: HFONT,
    view: HWND,
}

/// Custom-drawn report text: dark background, wrapped lines, wheel/key scrolling and a
/// scrollbar. A read-only EDIT control painted itself white in this theme, which is why the
/// report is drawn here instead.
struct ReportView {
    scale: u32,
    font: HFONT,
    raw: Vec<String>,
    wrapped: Vec<String>,
    wrap_w: i32,
    offset: usize,
}

unsafe fn report_view_paint(hwnd: HWND, vp: *mut ReportView) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let w = rc.right.max(1);
    let h = rc.bottom.max(1);

    if !vp.is_null() {
        let view = &mut *vp;
        let bg = gdi32::CreateSolidBrush(C_INPUT_BG);
        gdi32::FillRect(hdc, &rc, bg);
        let _ = gdi32::DeleteObject(bg);
        let border = gdi32::CreateSolidBrush(C_PANEL_BORDER);
        for edge in [
            RECT { left: 0, top: 0, right: w, bottom: 1 },
            RECT { left: 0, top: h - 1, right: w, bottom: h },
            RECT { left: 0, top: 0, right: 1, bottom: h },
            RECT { left: w - 1, top: 0, right: w, bottom: h },
        ] {
            gdi32::FillRect(hdc, &edge, border);
        }
        let _ = gdi32::DeleteObject(border);

        gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
        let old_font = gdi32::SelectObject(hdc, view.font);
        let line_h = s(16, view.scale).max(1);
        let pad = s(8, view.scale);
        let bar_w = s(4, view.scale).max(2);
        let text_w = (w - pad * 2 - bar_w - s(6, view.scale)).max(s(20, view.scale));
        if view.wrap_w != text_w {
            view.wrapped.clear();
            for line in &view.raw {
                for part in wrap_log_text(hdc, line, text_w) {
                    view.wrapped.push(part);
                }
            }
            view.wrap_w = text_w;
        }
        let total = view.wrapped.len();
        let visible = ((h - pad * 2) / line_h).max(1) as usize;
        let max_off = total.saturating_sub(visible);
        if view.offset > max_off {
            view.offset = max_off;
        }
        let end = (view.offset + visible).min(total);
        for (i, line) in view.wrapped[view.offset..end].iter().enumerate() {
            let y = pad + (i as i32) * line_h;
            let mut trc = RECT { left: pad, top: y, right: w - pad - bar_w - s(4, view.scale), bottom: y + line_h };
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            let wt = wstr(line);
            gdi32::DrawTextW(hdc, wt.as_ptr(), -1, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
        }

        if total > visible {
            let bar_x = w - pad - bar_w;
            let track = RECT { left: bar_x, top: pad, right: bar_x + bar_w, bottom: h - pad };
            let tb = gdi32::CreateSolidBrush(C_PANEL_BORDER);
            gdi32::FillRect(hdc, &track, tb);
            let _ = gdi32::DeleteObject(tb);
            let track_h = (h - pad * 2).max(1);
            let thumb_h = (((visible as f32 / total as f32) * track_h as f32).max(s(18, view.scale) as f32) as i32).min(track_h);
            let max_top = track_h - thumb_h;
            let frac = if max_off > 0 { view.offset as f32 / max_off as f32 } else { 0.0 };
            let top = pad + (max_top as f32 * frac) as i32;
            let thumb = RECT { left: bar_x, top, right: bar_x + bar_w, bottom: top + thumb_h };
            let thb = gdi32::CreateSolidBrush(C_TEXT_MUTED);
            gdi32::FillRect(hdc, &thumb, thb);
            let _ = gdi32::DeleteObject(thb);
        }
        gdi32::SelectObject(hdc, old_font);
    }

    EndPaint(hwnd, &ps);
}

unsafe extern "system" fn report_view_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let vp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ReportView;
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            report_view_paint(hwnd, vp);
            0
        }
        WM_SIZE => {
            if !vp.is_null() {
                (*vp).wrap_w = 0;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        0x020A /* WM_MOUSEWHEEL */ => {
            if !vp.is_null() {
                let delta = ((wp >> 16) & 0xFFFF) as i16 as i32;
                let view = &mut *vp;
                if delta > 0 {
                    view.offset = view.offset.saturating_sub(3);
                } else {
                    view.offset = view.offset.saturating_add(3);
                }
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_KEYDOWN => {
            if wp == 0x43 && (GetKeyState(0x11) as u16 & 0x8000) != 0 {
                let parent = GetParent(hwnd);
                let sp = GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut ReportState;
                if !sp.is_null() {
                    let _ = copy_text_to_clipboard(hwnd, &(*sp).body);
                }
                return 0;
            }
            if !vp.is_null() {
                let view = &mut *vp;
                match wp {
                    0x26 => view.offset = view.offset.saturating_sub(1),  // Up
                    0x28 => view.offset = view.offset.saturating_add(1),  // Down
                    0x21 => view.offset = view.offset.saturating_sub(10), // Page Up
                    0x22 => view.offset = view.offset.saturating_add(10), // Page Down
                    0x24 => view.offset = 0,                              // Home
                    0x23 => view.offset = usize::MAX / 2,                 // End (paint clamps)
                    _ => {}
                }
                InvalidateRect(hwnd, std::ptr::null(), 0);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN => {
            SetFocus(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Shows a resizable, scrollable, copyable text report (crash report / DLL details).
unsafe fn report_dialog(parent: HWND, scale: u32, title: &str, heading: &str, body: &str) {
    let scale = fit_dialog_scale(scale, BASE_REPORT_W, BASE_REPORT_H);
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxReportDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(report_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let view_cls = wstr("BlackBoxReportView");
    let mut vwc = WNDCLASSEXW::default();
    vwc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    vwc.lpfnWndProc = Some(report_view_proc);
    vwc.hInstance = hinst;
    vwc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    vwc.lpszClassName = view_cls.as_ptr();
    vwc.hbrBackground = class_bg_brush();
    RegisterClassExW(&vwc);

    let font_name = wstr("Segoe UI");
    let font_bold = gdi32::CreateFontW(s(-16, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let mono_name = wstr("Consolas");
    let font_mono = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, mono_name.as_ptr());

    let client_w = s(BASE_REPORT_W, scale);
    let client_h = s(BASE_REPORT_H, scale);
    let mut wr = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU | 0x00040000 /* WS_THICKFRAME */, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;

    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let start_x = parent_rc.left + ((parent_rc.right - parent_rc.left) - win_w) / 2;
    let start_y = parent_rc.top + ((parent_rc.bottom - parent_rc.top) - win_h) / 2;

    let title_w = wstr(&crate::version::window_title(title));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title_w.as_ptr(),
        WS_CAPTION | WS_SYSMENU | 0x00040000 /* WS_THICKFRAME */ | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        gdi32::DeleteObject(font_bold);
        gdi32::DeleteObject(font_mono);
        return;
    }
    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let mut st = ReportState {
        scale,
        done: false,
        heading: heading.encode_utf16().chain(std::iter::once(0)).collect(),
        body: body.replace("\r\n", "\n").replace('\n', "\r\n").encode_utf16().chain(std::iter::once(0)).collect(),
        font_bold,
        view: NULL_HANDLE,
    };
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut st as *mut ReportState as LONG_PTR);

    let mut view = Box::new(ReportView {
        scale,
        font: font_mono,
        raw: body.replace("\r\n", "\n").split('\n').map(|l| l.to_string()).collect(),
        wrapped: Vec::new(),
        wrap_w: 0,
        offset: 0,
    });

    let view_hwnd = CreateWindowExW(
        0,
        view_cls.as_ptr(),
        wstr("").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        s(20, scale), s(56, scale),
        client_w - s(40, scale), client_h - s(120, scale),
        hwnd, NULL_HANDLE, hinst, std::ptr::null_mut(),
    );
    SetWindowLongPtrW(view_hwnd, GWLP_USERDATA, &mut *view as *mut ReportView as LONG_PTR);
    st.view = view_hwnd;

    let b_copy = mk(hwnd, hinst, WC_BUTTON_S, "Copy report", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, client_w - s(20, scale) - s(96, scale) - s(8, scale) - s(160, scale), client_h - s(48, scale), s(160, scale), s(32, scale), 2, st.font_bold);
    subclass_button(b_copy, false);
    let b_close = mk(hwnd, hinst, WC_BUTTON_S, "Close", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, client_w - s(20, scale) - s(96, scale), client_h - s(48, scale), s(96, scale), s(32, scale), 1, st.font_bold);
    subclass_button(b_close, true);

    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    SetFocus(view_hwnd);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        if st.done {
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    gdi32::DeleteObject(font_bold);
    drop(view);
}

unsafe fn on_dll_details(hwnd: HWND, app: &mut App, row: i32, scale: u32) {
    let path = {
        let sh = app.shared.lock().unwrap();
        sh.modules.get(row as usize).map(|m| m.path.clone())
    };
    let Some(path) = path else { return };
    let pid = resolve_target_pid(app).unwrap_or(0);
    let heading = format!(
        "{}  -  imports & dependencies{}",
        std::path::Path::new(&path).file_name().and_then(|f| f.to_str()).unwrap_or(&path),
        if pid == 0 { " (no target selected)" } else { "" }
    );
    match crate::diag::dll_report(&path, pid) {
        Ok(lines) => report_dialog(hwnd, scale, "DLL details", &heading, &lines.join("\r\n")),
        Err(e) => show_message(hwnd, scale, MsgKind::Error, "DLL details failed", &e, "The file may not be a valid DLL.", "E4001 DLL_DETAILS", Vec::new()),
    }
}
unsafe extern "system" fn report_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let stp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ReportState;
    let scale = if stp.is_null() { 100 } else { (*stp).scale };

    match msg {
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);
            let cw = rc.right;
            let ch = rc.bottom;
            if !stp.is_null() && !(*stp).view.is_null() {
                MoveWindow((*stp).view, s(20, scale), s(56, scale), cw - s(40, scale), (ch - s(120, scale)).max(s(60, scale)), 1);
            }
            let close = GetDlgItem(hwnd, 1);
            let copy = GetDlgItem(hwnd, 2);
            if !close.is_null() {
                MoveWindow(close, cw - s(20, scale) - s(96, scale), ch - s(48, scale), s(96, scale), s(32, scale), 1);
            }
            if !copy.is_null() {
                MoveWindow(copy, cw - s(20, scale) - s(96, scale) - s(8, scale) - s(160, scale), ch - s(48, scale), s(160, scale), s(32, scale), 1);
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);
            let bg = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(hdc, &rc, bg);
            let _ = gdi32::DeleteObject(bg);
            if !stp.is_null() {
                gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
                gdi32::SelectObject(hdc, (*stp).font_bold);
                gdi32::SetTextColor(hdc, C_TEXT_BODY);
                let mut trc = RECT { left: s(20, scale), top: s(16, scale), right: rc.right - s(20, scale), bottom: s(44, scale) };
                gdi32::DrawTextW(hdc, (*stp).heading.as_ptr(), -1, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x00008000 /* DT_END_ELLIPSIS */ | DT_NOPREFIX_FLAG);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, scale);
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if !stp.is_null() {
                if id == 1 {
                    (*stp).done = true;
                } else if id == 2 {
                    let _ = copy_text_to_clipboard(hwnd, &(*stp).body);
                }
            }
            0
        }
        WM_CLOSE => {
            if !stp.is_null() {
                (*stp).done = true;
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
// ===== Confirmation dialog =====
const BASE_CONFIRM_W: i32 = 430;
const BASE_CONFIRM_H: i32 = 200;

struct ConfirmState {
    scale: u32,
    done: bool,
    confirmed: bool,
    title: Vec<u16>,
    body: Vec<u16>,
    font: HFONT,
    font_bold: HFONT,
}

/// Small modal yes/no question. Returns true only when the confirm button was pressed.
unsafe fn confirm_dialog(parent: HWND, scale: u32, title: &str, body: &str, confirm_label: &str) -> bool {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxConfirmDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(confirm_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let font_name = wstr("Segoe UI");
    let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let client_w = s(BASE_CONFIRM_W, scale);
    let client_h = s(BASE_CONFIRM_H, scale);
    let mut wr = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;


    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let start_x = parent_rc.left + ((parent_rc.right - parent_rc.left) - win_w) / 2;
    let start_y = parent_rc.top + ((parent_rc.bottom - parent_rc.top) - win_h) / 2;

    let title_w = wstr(&crate::version::window_title(title));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title_w.as_ptr(),
        WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        gdi32::DeleteObject(font);
        gdi32::DeleteObject(font_bold);
        return false;
    }
    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let mut st = ConfirmState {
        scale,
        done: false,
        confirmed: false,
        title: title.encode_utf16().chain(std::iter::once(0)).collect(),
        body: body.encode_utf16().chain(std::iter::once(0)).collect(),
        font,
        font_bold,
    };
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut st as *mut ConfirmState as LONG_PTR);

    let btn_y = s(BASE_CONFIRM_H - 48, scale);
    let b_cancel = mk(hwnd, hinst, WC_BUTTON_S, "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_SETTINGS_W - 16 - 120 - 8 - 96, scale), s(774, scale), s(96, scale), s(32, scale), 2, font_bold);
    subclass_button(b_cancel, false);
    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, confirm_label, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_CONFIRM_W - 16 - 96, scale), btn_y, s(96, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);

    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    SetFocus(b_cancel);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        if st.done {
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    gdi32::DeleteObject(font);
    gdi32::DeleteObject(font_bold);
    st.confirmed
}

unsafe extern "system" fn confirm_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let stp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ConfirmState;
    let scale = if stp.is_null() { 100 } else { (*stp).scale };

    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let bg = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(hdc, &rc, bg);
            let _ = gdi32::DeleteObject(bg);

            if !stp.is_null() {
                let st = &*stp;
                paint_dialog_card(hdc, s(16, scale), s(12, scale), s(BASE_CONFIRM_W - 32, scale), s(124, scale), "CONFIRM", scale);
                gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);

                gdi32::SelectObject(hdc, st.font_bold);
                gdi32::SetTextColor(hdc, C_TEXT_BODY);
                let mut rc_title = RECT { left: s(32, scale), top: s(38, scale), right: s(BASE_CONFIRM_W - 32, scale), bottom: s(60, scale) };
                gdi32::DrawTextW(hdc, st.title.as_ptr(), -1, &mut rc_title, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                gdi32::SelectObject(hdc, st.font);
                gdi32::SetTextColor(hdc, C_TEXT_MUTED);
                let mut rc_body = RECT { left: s(32, scale), top: s(66, scale), right: s(BASE_CONFIRM_W - 32, scale), bottom: s(126, scale) };
                gdi32::DrawTextW(hdc, st.body.as_ptr(), -1, &mut rc_body, DT_LEFT_FLAG | DT_WORDBREAK_FLAG | DT_NOPREFIX_FLAG);
            }

            EndPaint(hwnd, &ps);
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, scale);
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if !stp.is_null() {
                if id == 1 {
                    (*stp).confirmed = true;
                    (*stp).done = true;
                } else if id == 2 {
                    (*stp).done = true;
                }
            }
            0
        }
        WM_CLOSE => {
            if !stp.is_null() {
                (*stp).done = true;
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
// ===== Per-File Options Editor =====
// Edits one row's method override, wait-for-module, delay-after, ignore-errors and the
// conditional "only inject when ..." options.
const BASE_FOPT_W: i32 = 420;
const BASE_FOPT_H: i32 = 448;

#[derive(Clone)]
struct FileOptionsValue {
    method: Option<u32>,
    wait_for: String,
    delay_after: u32,
    ignore: bool,
    require_module: String,
    require_window: bool,
    require_process: String,
    delay_ms: u32,
    enabled: bool,
}

struct FileOptState {
    scale: u32,
    done: bool,
    result: Option<FileOptionsValue>,
    name: Vec<u16>,
    font: HFONT,
    font_bold: HFONT,
    enabled: bool,
}

unsafe fn force_foreground(hwnd: HWND) {
    let fg = GetForegroundWindow();
    let fg_thread = if fg.is_null() {
        0
    } else {
        GetWindowThreadProcessId(fg, std::ptr::null_mut())
    };
    let me = kernel32::GetCurrentThreadId();
    if fg_thread != 0 && fg_thread != me {
        let _ = AttachThreadInput(fg_thread, me, 1);
    }
    let _ = SetForegroundWindow(hwnd);
    let _ = BringWindowToTop(hwnd);
    let _ = SetActiveWindow(hwnd);
    if fg_thread != 0 && fg_thread != me {
        let _ = AttachThreadInput(fg_thread, me, 0);
    }
}
unsafe fn file_options_dialog(parent: HWND, scale: u32, module: &Module) -> Option<FileOptionsValue> {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxFileOptionsDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(file_options_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let font_name = wstr("Segoe UI");
    let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let client_w = s(BASE_FOPT_W, scale);
    let client_h = s(BASE_FOPT_H, scale);
    let mut wr = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;


    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let start_x = parent_rc.left + ((parent_rc.right - parent_rc.left) - win_w) / 2;
    let start_y = parent_rc.top + ((parent_rc.bottom - parent_rc.top) - win_h) / 2;

    let title_w = wstr(&crate::version::window_title(&format!("File Options ({})", module.file_name())));
    let hwnd = CreateWindowExW(
        0,
        cls.as_ptr(),
        title_w.as_ptr(),
        WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x,
        start_y,
        win_w,
        win_h,
        parent,
        NULL_HANDLE,
        hinst,
        std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        gdi32::DeleteObject(font);
        gdi32::DeleteObject(font_bold);
        return None;
    }

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let mut st = FileOptState {
        scale,
        done: false,
        result: None,
        name: module.file_name().encode_utf16().chain(std::iter::once(0)).collect(),
        font,
        font_bold,
        enabled: module.enabled,
    };
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut st as *mut FileOptState as LONG_PTR);

    let edit_w = s(134, scale);
    let edit_x = s(252, scale);
    let combo_w = edit_w + s(4, scale);
    let combo_x = edit_x - s(2, scale);
    let cb_method = mk_ex(
        hwnd, hinst, WC_COMBOBOX_S, "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST_C | CBS_HASSTRINGS_C,
        0, combo_x, s(78, scale), combo_w, s(200, scale), IDC_FOPT_METHOD, font,
    );
    for name in ["Global", "Standard", "LdrLoadDll", "Manual Map", "Hijack", "NtCreateThreadEx", "APC"] {
        SendMessageW(cb_method, CB_ADDSTRING, 0, wstr(name).as_ptr() as LPARAM);
    }
    let method_sel = module.method.map(|m| m as i32 + 1).unwrap_or(0);
    SendMessageW(cb_method, CB_SETCURSEL, method_sel as WPARAM, 0);
    subclass_combobox(cb_method, scale, C_PANEL, font);
    fit_combo_closed_height(cb_method, scale);

    let e_wait = mk_ex(hwnd, hinst, WC_EDIT_S, &module.wait_for_module, WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, edit_x, s(122, scale), edit_w, s(20, scale), IDC_FOPT_WAIT, font);
    uxtheme::set_theme(e_wait, "DarkMode_CFD");
    subclass_edit(e_wait);

    let after_text = if module.delay_after_ms > 0 { module.delay_after_ms.to_string() } else { String::new() };
    let e_after = mk_ex(hwnd, hinst, WC_EDIT_S, &after_text, WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_NUMBER_C | 0x0001 /* ES_CENTER */, 0, edit_x, s(162, scale), edit_w, s(20, scale), IDC_FOPT_AFTER, font);
    uxtheme::set_theme(e_after, "DarkMode_CFD");
    subclass_edit(e_after);

    let e_require = mk_ex(hwnd, hinst, WC_EDIT_S, &module.require_module, WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, edit_x, s(202, scale), edit_w, s(20, scale), IDC_FOPT_REQUIRE, font);
    uxtheme::set_theme(e_require, "DarkMode_CFD");
    subclass_edit(e_require);

    let e_process = mk_ex(
        hwnd, hinst, WC_EDIT_S, &module.require_process,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL_C,
        0, edit_x, s(242, scale), edit_w, s(20, scale), IDC_FOPT_PROCESS, font,
    );
    uxtheme::set_theme(e_process, "DarkMode_CFD");
    subclass_edit(e_process);

    let delay_text = if module.delay_ms > 0 { module.delay_ms.to_string() } else { "0".to_string() };
    let e_delay_file = mk_ex(hwnd, hinst, WC_EDIT_S, &delay_text, WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_NUMBER_C, 0, edit_x, s(282, scale), edit_w, s(20, scale), IDC_FOPT_DELAY, font);
    uxtheme::set_theme(e_delay_file, "DarkMode_CFD");
    subclass_edit(e_delay_file);

    let c_ignore = mk(hwnd, hinst, WC_BUTTON_S, "Ignore errors for this file", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(32, scale), s(318, scale), s(300, scale), s(22, scale), IDC_FOPT_IGNORE, font);
    subclass_checkbox(c_ignore, module.ignore_errors);

    let c_window = mk(hwnd, hinst, WC_BUTTON_S, "Only inject when the target has a window", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(32, scale), s(346, scale), s(340, scale), s(22, scale), IDC_FOPT_REQUIRE_WINDOW, font);
    subclass_checkbox(c_window, module.require_window);

    let toggle_label = if module.enabled { "Disable" } else { "Enable" };
    let b_toggle = mk(hwnd, hinst, WC_BUTTON_S, toggle_label, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(16, scale), s(394, scale), s(100, scale), s(32, scale), 5, font);
    subclass_button(b_toggle, false);

    let b_save = mk(hwnd, hinst, WC_BUTTON_S, "Save", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_FOPT_W - 16 - 72, scale), s(394, scale), s(72, scale), s(32, scale), 1, font_bold);
    subclass_button(b_save, true);
    let b_cancel = mk(hwnd, hinst, WC_BUTTON_S, "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_FOPT_W - 16 - 72 - 8 - 72, scale), s(394, scale), s(72, scale), s(32, scale), 2, font_bold);
    subclass_button(b_cancel, false);

    ShowWindow(hwnd, SW_SHOW);
    force_foreground(hwnd);
    EnableWindow(parent, 0);
    SetFocus(cb_method);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        if st.done {
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    gdi32::DeleteObject(font);
    gdi32::DeleteObject(font_bold);
    st.result
}

unsafe extern "system" fn file_options_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let stp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut FileOptState;
    let scale = if stp.is_null() { 100 } else { (*stp).scale };

    match msg {
        WM_ERASEBKGND => 1,
        WM_CTLCOLOREDIT => {
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_INPUT_BG);
            input_bg_brush() as LRESULT
        }
        WM_CTLCOLORSTATIC => {
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_PANEL);
            class_bg_brush() as LRESULT
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let bg = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(hdc, &rc, bg);
            let _ = gdi32::DeleteObject(bg);

            if !stp.is_null() {
                let st = &*stp;
                paint_dialog_card(hdc, s(16, scale), s(12, scale), s(BASE_FOPT_W - 32, scale), s(368, scale), "PER-FILE OPTIONS", scale);
                gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);

                gdi32::SelectObject(hdc, st.font_bold);
                gdi32::SetTextColor(hdc, C_TEXT_BODY);
                let mut rc_name = RECT {
                    left: s(32, scale), top: s(38, scale),
                    right: s(BASE_FOPT_W - 32, scale), bottom: s(60, scale),
                };
                gdi32::DrawTextW(hdc, st.name.as_ptr(), -1, &mut rc_name, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x00008000 /* DT_END_ELLIPSIS */ | DT_NOPREFIX_FLAG);

                // Input containers
                let pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_INPUT_BORDER);
                let brush = gdi32::CreateSolidBrush(C_INPUT_BG);
                let o_pen = gdi32::SelectObject(hdc, pen);
                let o_br = gdi32::SelectObject(hdc, brush);
                for y in [118, 158, 198, 238, 278] {
                    round_rect(hdc, s(250, scale), s(y, scale), s(388, scale), s(y + 32, scale), s(R_INPUT, scale), s(R_INPUT, scale));
                }
                gdi32::SelectObject(hdc, o_br);
                gdi32::SelectObject(hdc, o_pen);
                let _ = gdi32::DeleteObject(pen);
                let _ = gdi32::DeleteObject(brush);

                gdi32::SelectObject(hdc, st.font);
                gdi32::SetTextColor(hdc, C_TEXT_HEAD);
                for (text, y) in [
                    ("Injection method:", 78),
                    ("Wait for module (name):", 118),
                    ("Delay after inject (ms):", 158),
                    ("Inject only if module:", 198),
                    ("Required process:", 238),
                    ("Delay before this file (ms):", 278),
                ] {
                    let mut trc = RECT { left: s(32, scale), top: s(y, scale), right: s(238, scale), bottom: s(y + 32, scale) };
                    let wt = wstr(text);
                    gdi32::DrawTextW(hdc, wt.as_ptr(), -1, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
                }
            }

            EndPaint(hwnd, &ps);
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, scale);
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if !stp.is_null() {
                if id == 5 {
                    (*stp).enabled = !(*stp).enabled;
                    let label = if (*stp).enabled { "Disable" } else { "Enable" };
                    let b = GetDlgItem(hwnd, 5);
                    SetWindowTextW(b, wstr(label).as_ptr());
                    InvalidateRect(b, std::ptr::null(), 1);
                } else if id == 1 {
                    let read = |ctrl: i32| -> Vec<u16> {
                        let h = GetDlgItem(hwnd, ctrl);
                        let mut buf = vec![0u16; 260];
                        let len = GetWindowTextW(h, buf.as_mut_ptr(), 260);
                        buf.truncate(len as usize);
                        buf
                    };
                    let method_sel = SendMessageW(GetDlgItem(hwnd, IDC_FOPT_METHOD), CB_GETCURSEL, 0, 0) as i32;
                    let method = if method_sel <= 0 { None } else { Some((method_sel - 1) as u32) };
                    let wait = from_wsz(read(IDC_FOPT_WAIT).as_ptr()).trim().to_string();
                    let after = from_wsz(read(IDC_FOPT_AFTER).as_ptr()).trim().parse::<u32>().unwrap_or(0).min(MAX_MODULE_DELAY_MS);
                    let require = from_wsz(read(IDC_FOPT_REQUIRE).as_ptr()).trim().to_string();
                    let ignore = get_checkbox_state(GetDlgItem(hwnd, IDC_FOPT_IGNORE));
                    let require_window = get_checkbox_state(GetDlgItem(hwnd, IDC_FOPT_REQUIRE_WINDOW));
                    let require_process = from_wsz(read(IDC_FOPT_PROCESS).as_ptr()).trim().to_string();
                    let delay_ms = from_wsz(read(IDC_FOPT_DELAY).as_ptr()).trim().parse::<u32>().unwrap_or(0).min(MAX_MODULE_DELAY_MS);
                    (*stp).result = Some(FileOptionsValue { delay_ms, enabled: (*stp).enabled, method, wait_for: wait, delay_after: after, ignore, require_module: require, require_window, require_process });
                    (*stp).done = true;
                } else if id == 2 {
                    (*stp).done = true;
                }
            }
            0
        }
        WM_CLOSE => {
            if !stp.is_null() {
                (*stp).done = true;
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn on_file_options(hwnd: HWND, app: &mut App, row: i32, scale: u32) {
    let current = {
        let sh = app.shared.lock().unwrap();
        sh.modules.get(row as usize).cloned()
    };
    let Some(current) = current else { return };
    let Some(value) = file_options_dialog(hwnd, scale, &current) else {
        return;
    };
    {
        let mut sh = app.shared.lock().unwrap();
        if let Some(m) = sh.modules.get_mut(row as usize) {
            m.delay_ms = value.delay_ms;
            m.enabled = value.enabled;
            m.method = value.method;
            m.wait_for_module = value.wait_for;
            m.delay_after_ms = value.delay_after;
            m.ignore_errors = value.ignore;
            m.require_module = value.require_module;
            m.require_window = value.require_window;
            m.require_process = value.require_process;
        }
    }
    let lv = GetDlgItem(hwnd, IDC_LIST);
    if !lv.is_null() {
        InvalidateRect(lv, std::ptr::null(), 0);
    }
    sync_ui_to_settings(hwnd, app);
    app.settings.save();
}
// ===== Activity log panel =====

fn log_color(level: LogLevel) -> COLORREF {
    match level {
        LogLevel::Success => 0x0080_DE4A,
        LogLevel::Warn => 0x0037_B5F0,
        LogLevel::Error => 0x004D_48E5,
        LogLevel::Info => C_TEXT_BODY,
    }
}

unsafe fn log_panel_of(app: &App) -> HWND {
    let main = {
        let sh = app.shared.lock().unwrap();
        sh.hwnd
    } as HWND;
    GetDlgItem(main, IDC_LOG_PANEL)
}

/// Posts one line to the UI thread, which appends it to the in-window log.
unsafe fn post_ui_log(hwnd: HWND, text: &str) {
    PostMessageW(hwnd, WM_APP_LOG, Box::into_raw(Box::new(text.to_string())) as WPARAM, 0);
}

/// Appends a line to the in-window log (from the UI thread).
unsafe fn push_log(app: &mut App, text: &str) {
    let level = log_level_for(text);
    if app.log.len() >= MAX_LOG_LINES {
        app.log.remove(0);
    }
    app.log.push((level, text.to_string()));
    // If the user scrolled up to read, keep that position; otherwise stay at the newest line
    let panel = log_panel_of(app);
    if !panel.is_null() {
        InvalidateRect(panel, std::ptr::null(), 0);
    }
}

unsafe fn copy_log(app: &App, owner: HWND) {
    let mut text = String::new();
    for (_, line) in &app.log {
        text.push_str(line);
        text.push_str("\r\n");
    }
    let w = wstr(&text);
    let _ = copy_text_to_clipboard(owner, &w);
}

/// Copies just the selected wrapped lines; returns false when there is no selection.
#[allow(dead_code)]
fn selection_text(wrapped: &[(String, LogLevel)], a: (usize, usize), b: (usize, usize)) -> String {
    if wrapped.is_empty() { return String::new(); }
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    let last = wrapped.len() - 1;
    let start = a.0.min(last);
    let end = b.0.min(last);
    let mut text = String::new();
    for li in start..=end {
        let line = &wrapped[li].0;
        let s0 = if li == start { a.1.min(line.len()) } else { 0 };
        let s1 = if li == end { b.1.min(line.len()) } else { line.len() };
        if s1 > s0 { text.push_str(&line[s0..s1]); }
        if li != end { text.push_str("\r\n"); }
    }
    text
}

#[allow(dead_code)]
unsafe fn copy_selection(app: &App, owner: HWND) -> bool {
    let Some((a, b)) = log_selection(app) else { return false };
    let text = selection_text(&app.log_wrapped, a, b);
    if text.is_empty() { return false; }
    let w = wstr(&text);
    let _ = copy_text_to_clipboard(owner, &w);
    true
}

unsafe fn log_measure(hdc: HDC, text: &str) -> i32 {
    let w = wstr(text);
    let mut sz = SIZE::default();
    gdi32::GetTextExtentPoint32W(hdc, w.as_ptr(), (w.len() - 1) as i32, &mut sz);
    sz.cx
}

/// Word-aware text wrapping that breaks at spaces where possible, or splits long tokens.
unsafe fn wrap_log_text(hdc: HDC, text: &str, max_w: i32) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        if word.is_empty() {
            if !cur.is_empty() {
                cur.push(' ');
            }
            continue;
        }
        let candidate = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if log_measure(hdc, &candidate) <= max_w {
            cur = candidate;
        } else {
            if !cur.is_empty() {
                out.push(cur);
                cur = String::new();
            }
            if log_measure(hdc, word) > max_w {
                for ch in word.chars() {
                    let mut trial = cur.clone();
                    trial.push(ch);
                    if !cur.is_empty() && log_measure(hdc, &trial) > max_w {
                        out.push(cur);
                        cur = String::new();
                        cur.push(ch);
                    } else {
                        cur = trial;
                    }
                }
            } else {
                cur = word.to_string();
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}


#[allow(dead_code)]
fn log_selection(app: &App) -> Option<((usize, usize), (usize, usize))> {
    let a = app.log_sel_anchor?;
    let b = app.log_sel_end?;
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    if a == b {
        None
    } else {
        Some((a, b))
    }
}

#[allow(dead_code)]
unsafe fn text_prefix_width(hdc: HDC, text: &str, byte: usize) -> i32 {
    let end = byte.min(text.len());
    let units: Vec<u16> = text[..end].encode_utf16().collect();
    if units.is_empty() {
        return 0;
    }
    let mut sz = SIZE::default();
    gdi32::GetTextExtentPoint32W(hdc, units.as_ptr(), units.len() as i32, &mut sz);
    sz.cx
}

unsafe fn paint_log_panel(hwnd: HWND, app: &mut App) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let w = rc.right.max(1);
    let h = rc.bottom.max(1);
    let scale = resolve_scale(app.settings.ui_scale);

    let brush = gdi32::CreateSolidBrush(C_PANEL);
    let pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
    let ob = gdi32::SelectObject(hdc, brush);
    let op = gdi32::SelectObject(hdc, pen);
    round_rect(hdc, 0, 0, w, h, s(R_CARD, scale), s(R_CARD, scale));
    gdi32::SelectObject(hdc, ob);
    gdi32::SelectObject(hdc, op);
    let _ = gdi32::DeleteObject(brush);
    let _ = gdi32::DeleteObject(pen);

    gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
    let old_font = gdi32::SelectObject(hdc, app.hfont_small);
    let line_h = s(LOG_LINE_H, scale).max(1);
    let pad = s(6, scale);
    let visible = (((h - pad * 2) / line_h).max(1)) as usize;
    let text_w = (w - s(20, scale)).max(s(20, scale));

    if app.log.is_empty() {
        gdi32::SetTextColor(hdc, C_TEXT_MUTED);
        let mut trc = RECT { left: s(10, scale), top: 0, right: w - s(10, scale), bottom: h };
        let hint = wstr("No activity yet.");
        gdi32::DrawTextW(hdc, hint.as_ptr(), -1, &mut trc, DT_CENTER_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
    } else {
        let mut wrapped: Vec<(String, LogLevel)> = Vec::new();
        for (level, text) in app.log.iter().rev().take(400) {
            let mut lines = wrap_log_text(hdc, text, text_w);
            lines.reverse();
            for l in lines {
                wrapped.push((l, *level));
            }
        }
        wrapped.reverse();
        let total = wrapped.len();
        let start = total.saturating_sub(visible);
        for (i, (line, level)) in wrapped[start..].iter().enumerate() {
            let y = pad + (i as i32) * line_h;
            let mut trc = RECT { left: s(10, scale), top: y, right: w - s(12, scale), bottom: y + line_h };
            gdi32::SetTextColor(hdc, log_color(*level));
            let wt = wstr(line);
            gdi32::DrawTextW(hdc, wt.as_ptr(), -1, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
        }
    }

    gdi32::SelectObject(hdc, old_font);
    EndPaint(hwnd, &ps);
}
unsafe extern "system" fn log_panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let main = GetParent(hwnd);
    let app_ptr = GetWindowLongPtrW(main, GWLP_USERDATA) as *mut App;

    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            if !app_ptr.is_null() {
                paint_log_panel(hwnd, &mut *app_ptr);
            }
            0
        }
        WM_LBUTTONDOWN => {
            SetFocus(hwnd);
            0
        }
        WM_KEYDOWN => {
            if wp == 0x43 && (GetKeyState(0x11) as u16 & 0x8000) != 0 {
                if !app_ptr.is_null() {
                    copy_log(&*app_ptr, hwnd);
                }
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        0x0205 /* WM_RBUTTONUP */ => {
            if !app_ptr.is_null() {
                let app = &mut *app_ptr;
                let scale = resolve_scale(app.settings.ui_scale);
                let mut pt = POINT::default();
                GetCursorPos(&mut pt);
                let cmd = show_context_menu(hwnd, &[("Copy all", 601), ("Clear logs", 602)], pt.x, pt.y, scale, app.hfont);
                match cmd {
                    601 => copy_log(app, hwnd),
                    602 => {
                        app.log.clear();
                        InvalidateRect(hwnd, std::ptr::null(), 0);
                    }
                    _ => {}
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn create_log_panel(hwnd: HWND, hinst: HINSTANCE, scale: u32, font: HFONT) {
    // Subclassed STATIC: it gets mouse input (SS_NOTIFY) and our custom painting, without
    // depending on a custom window class that CreateWindowEx could refuse to create.
    let panel = CreateWindowExW(
        0,
        wstr("STATIC").as_ptr(),
        wstr("").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | SS_NOTIFY,
        s(16, scale),
        s(417, scale),
        s(BASE_CLIENT_W - 32, scale),
        s(120, scale),
        hwnd,
        IDC_LOG_PANEL as isize as HMENU,
        hinst,
        std::ptr::null_mut(),
    );
    if panel.is_null() {
        return;
    }
    SetWindowLongPtrW(panel, GWLP_WNDPROC, log_panel_proc as isize);
    uxtheme::set_theme(panel, "DarkMode_Explorer");
    uxtheme::allow_dark_mode_for_window(panel);
    SendMessageW(panel, WM_SETFONT, font as WPARAM, 1);

    let w = s(BASE_CLIENT_W - 32, scale);
    let h = s(120, scale);
    let rgn = CreateRoundRectRgn(0, 0, w, h, s(R_CARD, scale) * 2, s(R_CARD, scale) * 2);
    SetWindowRgn(panel, rgn, 1);
}
// ===== Process Picker Dialog (Auto-refreshing & Windowed filter) =====
struct PickerState {
    filter: String,
    windowed_only: bool,
    // None forces a rebuild on the next refresh (used when the filter changes)
    last_procs: Option<Vec<(u32, String)>>,
    icon_cache: HashMap<String, i32>,
    himl: HANDLE,
    scale: u32,
    selected_name: String,
    selected_pid: u32,
    selected_icon: Option<HICON>,
    input_brush: HBRUSH,
    list_w: i32,
    // Architecture per pid, looked up once per process instead of on every rebuild and paint
    arch_cache: HashMap<u32, Option<crate::process::Arch>>,
    pending_icons: Vec<PendingIcon>,
    sort_col: usize,
    sort_asc: bool,
}

unsafe fn get_picker_selected_info(lv: HWND) -> Option<(String, u32)> {
    let sel = SendMessageW(lv, LVM_GETNEXTITEM, (-1isize) as WPARAM, LVNI_SELECTED_C as LPARAM) as i32;
    if sel < 0 { return None; }
    let mut buf = [0u16; 260];
    let mut item = LVITEMW {
        mask: LVIF_TEXT_C,
        iItem: sel,
        iSubItem: 0,
        pszText: buf.as_mut_ptr(),
        cchTextMax: 260,
        ..Default::default()
    };
    if SendMessageW(lv, LVM_GETITEMW_ID, 0, &mut item as *mut _ as LPARAM) != 0 {
        let name = from_wsz(buf.as_ptr());
        let mut pid_buf = [0u16; 32];
        let mut pid_item = LVITEMW {
            mask: LVIF_TEXT_C,
            iItem: sel,
            iSubItem: 1,
            pszText: pid_buf.as_mut_ptr(),
            cchTextMax: 32,
            ..Default::default()
        };
        let mut pid = 0u32;
        if SendMessageW(lv, LVM_GETITEMW_ID, 0, &mut pid_item as *mut _ as LPARAM) != 0 {
            let pid_str = from_wsz(pid_buf.as_ptr());
            pid = pid_str.parse().unwrap_or(0);
        }
        if !name.is_empty() { return Some((name, pid)); }
    }
    None
}

unsafe fn refresh_picker_if_changed(lv: HWND, filter: &str, windowed_only: bool, state: &mut PickerState) {
    let procs = crate::process::enumerate();
    let lower = filter.trim().to_lowercase();
    let windowed_set = if windowed_only { Some(get_windowed_pids()) } else { None };

    let mut current_filtered: Vec<(u32, String)> = Vec::with_capacity(procs.len());
    for p in procs {
        // PID 0 (System Idle Process) is an idle CPU execution context, not an accessible process
        if p.pid == 0 {
            continue;
        }
        if let Some(ref w_set) = windowed_set {
            if !w_set.contains(&p.pid) {
                continue;
            }
        }
        // Matches the process name or its PID, so typing 3864 finds process 3864
        if !lower.is_empty() && !p.name.to_lowercase().contains(&lower) && !p.pid.to_string().contains(&lower) {
            continue;
        }
        current_filtered.push((p.pid, p.name));
    }

    match state.sort_col {
        0 => {
            // Process Name: alphabetical (case-insensitive), tie-break on PID
            if state.sort_asc {
                current_filtered.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()).then_with(|| a.0.cmp(&b.0)));
            } else {
                current_filtered.sort_by(|a, b| b.1.to_ascii_lowercase().cmp(&a.1.to_ascii_lowercase()).then_with(|| b.0.cmp(&a.0)));
            }
        }
        1 => {
            // PID: numerical order
            if state.sort_asc {
                current_filtered.sort_by(|a, b| a.0.cmp(&b.0));
            } else {
                current_filtered.sort_by(|a, b| b.0.cmp(&a.0));
            }
        }
        2 => {
            // Architecture: x64 vs x86 vs unknown
            let rank = |pid: &u32| -> u8 {
                match state.arch_cache.get(pid).copied().flatten() {
                    Some(crate::process::Arch::X64) => 2,
                    Some(crate::process::Arch::X86) => 1,
                    None => 0,
                }
            };
            if state.sort_asc {
                current_filtered.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then_with(|| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase())));
            } else {
                current_filtered.sort_by(|a, b| rank(&b.0).cmp(&rank(&a.0)).then_with(|| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase())));
            }
        }
        _ => {}
    }

    if state.last_procs.as_ref() == Some(&current_filtered) {
        return;
    }

    let cur_sel = get_picker_selected_info(lv);

    SendMessageW(lv, WM_SETREDRAW, 0, 0);
    SendMessageW(lv, LVM_DELETEALLITEMS, 0, 0);

    // Pids per process name, so an icon can be taken from any instance that is accessible
    let mut pids_by_name: HashMap<String, Vec<u32>> = HashMap::new();
    for (pid, name) in &current_filtered {
        pids_by_name.entry(name.to_ascii_lowercase()).or_default().push(*pid);
    }

    let mut reselect_idx: Option<i32> = None;
    state.pending_icons.clear();
    for (idx, (pid, ref name)) in current_filtered.iter().enumerate() {
        // Icons not cached yet are loaded by load_pending_icons, so rows appear without waiting
        let icon_idx = match state.icon_cache.get(&name.to_ascii_lowercase()) {
            Some(&c_idx) => c_idx,
            None => {
                let key = name.to_ascii_lowercase();
                let alternates = pids_by_name.get(&key).cloned().unwrap_or_default();
                let idx = if let Some(hicon) = fetch_icon_with_alternates(*pid, name, true, &alternates) {
                    let r = ImageList_ReplaceIcon(state.himl, -1, hicon);
                    DestroyIcon(hicon);
                    r
                } else {
                    -1
                };
                state.icon_cache.insert(key, idx);
                idx
            }
        };

        let wname = wstr(name);
        let mut item = LVITEMW {
            mask: LVIF_TEXT_C | LVIF_IMAGE_C,
            iItem: idx as i32,
            iSubItem: 0,
            pszText: wname.as_ptr() as *mut u16,
            // Always set the image: rows without an icon must not inherit image 0
            iImage: if icon_idx >= 0 { icon_idx } else { I_IMAGENONE_C },
            ..Default::default()
        };
        let row = SendMessageW(lv, LVM_INSERTITEMW, 0, &mut item as *mut _ as LPARAM) as i32;
        if row >= 0 {
            let pid_str = wstr(&format!("{}", pid));
            let mut pid_item = LVITEMW {
                mask: LVIF_TEXT_C,
                iItem: row,
                iSubItem: 1,
                pszText: pid_str.as_ptr() as *mut u16,
                ..Default::default()
            };
            SendMessageW(lv, LVM_SETITEMW, 0, &mut pid_item as *mut _ as LPARAM);

            let arch = *state.arch_cache.entry(*pid).or_insert_with(|| crate::process::process_arch_quick(*pid));
            let arch_str = match arch {
                Some(crate::process::Arch::X64) => wstr("x64"),
                Some(crate::process::Arch::X86) => wstr("x86"),
                None => wstr("-"),
            };
            let mut arch_item = LVITEMW {
                mask: LVIF_TEXT_C,
                iItem: row,
                iSubItem: 2,
                pszText: arch_str.as_ptr() as *mut u16,
                ..Default::default()
            };
            SendMessageW(lv, LVM_SETITEMW, 0, &mut arch_item as *mut _ as LPARAM);

            if let Some((ref sel_name, sel_pid)) = cur_sel {
                if sel_name == name && sel_pid == *pid {
                    reselect_idx = Some(row);
                }
            }
        }
    }

    let live: std::collections::HashSet<u32> = current_filtered.iter().map(|(pid, _)| *pid).collect();
    state.arch_cache.retain(|pid, _| live.contains(pid));
    fit_picker_columns(lv, state.list_w, state.scale);

    let target_sel = reselect_idx.unwrap_or(if !current_filtered.is_empty() { 0 } else { -1 });
    if target_sel >= 0 {
        let mut sel_item = LVITEMW {
            mask: 0x0008,
            iItem: target_sel,
            state: 0x0002 | 0x0001,
            stateMask: 0x0002 | 0x0001,
            ..Default::default()
        };
        SendMessageW(lv, 0x1000 + 43 /* LVM_SETITEMSTATE */, target_sel as WPARAM, &mut sel_item as *mut _ as LPARAM);
        SendMessageW(lv, 0x1000 + 19 /* LVM_ENSUREVISIBLE */, target_sel as WPARAM, 0);
    }

    SendMessageW(lv, WM_SETREDRAW, 1, 0);
    InvalidateRect(lv, std::ptr::null(), 0);

    state.last_procs = Some(current_filtered);
}

unsafe fn update_picker_preview_card(hwnd: HWND, st: *mut PickerState) {
    if st.is_null() { return; }
    let lv = GetDlgItem(hwnd, IDC_PROC_LIST);
    // Nothing selected (e.g. the filter matches no rows) clears the preview, so a hidden
    // process can't be picked by Select or Enter
    let (name, pid) = get_picker_selected_info(lv).unwrap_or_default();
    if name != (*st).selected_name || pid != (*st).selected_pid {
        (*st).selected_name = name.clone();
        (*st).selected_pid = pid;
        if let Some(old) = (*st).selected_icon.take() {
            DestroyIcon(old);
        }
        (*st).selected_icon = if name.is_empty() { None } else { fetch_process_icon_sized(pid, &name, false) };
        let dlg_w = s(BASE_PICKER_W, (*st).scale);
        let mut rc = RECT {
            left: s(16, (*st).scale),
            top: s(12, (*st).scale),
            right: dlg_w - s(16, (*st).scale),
            bottom: s(82, (*st).scale),
        };
        InvalidateRect(hwnd, &mut rc, 0);
    }
}

// Text and image index of one cell of an owner-drawn list.
unsafe fn picker_cell(lv: HWND, row: i32, sub: i32) -> (Vec<u16>, i32) {
    let mut buf = [0u16; 260];
    let mut item = LVITEMW {
        mask: LVIF_TEXT_C | LVIF_IMAGE_C,
        iItem: row,
        iSubItem: sub,
        pszText: buf.as_mut_ptr(),
        cchTextMax: 260,
        ..Default::default()
    };
    SendMessageW(lv, LVM_GETITEMW_ID, 0, &mut item as *mut _ as LPARAM);
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    (buf[..len].to_vec(), item.iImage)
}

// Process row in the dark palette (same look as the main ITEMS list): icon, name, PID, arch.
unsafe fn draw_picker_row(dis: &DRAWITEMSTRUCT, himl: HANDLE, scale: u32) {
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let row = dis.itemID as i32;
    let lv = dis.hwndItem;
    let is_selected = (dis.itemState & ODS_SELECTED) != 0;

    let bg_color = if is_selected {
        C_ROW_SEL
    } else if row % 2 == 1 {
        C_PANEL_ALT
    } else {
        C_PANEL
    };
    let brush = gdi32::CreateSolidBrush(bg_color);
    gdi32::FillRect(hdc, &rc, brush);
    let _ = gdi32::DeleteObject(brush);

    let font = SendMessageW(lv, WM_GETFONT, 0, 0) as HFONT;
    let old_font = if !font.is_null() { gdi32::SelectObject(hdc, font) } else { NULL_HANDLE };
    gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);

    let pad = s(10, scale);
    let icon_sz = s(16, scale);
    let mut col_left = rc.left;
    for sub in 0..3 {
        let col_w = SendMessageW(lv, 0x1000 + 29 /* LVM_GETCOLUMNWIDTH */, sub as WPARAM, 0) as i32;
        let (text, icon) = picker_cell(lv, row, sub);
        let mut trc = RECT {
            left: col_left + pad,
            top: rc.top,
            right: col_left + col_w - pad,
            bottom: rc.bottom,
        };
        if sub == 0 {
            let icon_y = rc.top + (rc.bottom - rc.top - icon_sz) / 2;
            if icon >= 0 {
                ImageList_Draw(himl, icon, hdc, col_left + pad, icon_y, ILD_TRANSPARENT_C);
            } else {
                let fallback = LoadIconW(NULL_HANDLE, 32512usize as *const u16);
                if !fallback.is_null() {
                    DrawIconEx(hdc, col_left + pad, icon_y, fallback, icon_sz, icon_sz, 0, NULL_HANDLE, 0x0003);
                }
            }
            trc.left += icon_sz + s(8, scale);
        }
        let text_clr = if is_selected || sub == 0 { C_TEXT_BODY } else { C_TEXT_HEAD };
        gdi32::SetTextColor(hdc, text_clr);
        gdi32::DrawTextW(hdc, text.as_ptr(), text.len() as i32, &mut trc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG | 0x00008000 /* DT_END_ELLIPSIS */);
        col_left += col_w;
    }

    if !old_font.is_null() { gdi32::SelectObject(hdc, old_font); }
}

// An icon still to be loaded for one picker row
struct PendingIcon {
    row: i32,
    pid: u32,
    name: String,
    alternates: Vec<u32>,
}

// Loads icons for the next few rows that lack one. Runs from the picker's timer, so the list
// appears at once and the icons fill in row by row.
unsafe fn load_pending_icons(lv: HWND, state: &mut PickerState, budget: usize) {
    let n = budget.min(state.pending_icons.len());
    let batch: Vec<PendingIcon> = state.pending_icons.drain(..n).collect();
    for item in batch {
        let cache_key = item.name.to_ascii_lowercase();
        let icon_idx = match state.icon_cache.get(&cache_key) {
            Some(&c_idx) => c_idx,
            None => {
                let new_idx = if let Some(hicon) = fetch_icon_with_alternates(item.pid, &item.name, true, &item.alternates) {
                    let r = ImageList_ReplaceIcon(state.himl, -1, hicon);
                    DestroyIcon(hicon);
                    r
                } else {
                    -1
                };
                state.icon_cache.insert(cache_key, new_idx);
                new_idx
            }
        };
        if icon_idx >= 0 {
            let mut img = LVITEMW {
                mask: LVIF_IMAGE_C,
                iItem: item.row,
                iSubItem: 0,
                iImage: icon_idx,
                ..Default::default()
            };
            SendMessageW(lv, LVM_SETITEMW, 0, &mut img as *mut _ as LPARAM);
            // Setting only the image repaints just the icon strip; redraw the whole row so the
            // rest of it is painted with the icon in place
            SendMessageW(lv, 0x1000 + 21 /* LVM_REDRAWITEMS */, item.row as WPARAM, item.row as LPARAM);
        }
    }
}

// The name column takes whatever the PID and Arch columns and the scrollbar leave over.
unsafe fn fit_picker_columns(lv: HWND, list_w: i32, scale: u32) {
    let name_w = list_w - list_scrollbar_w(lv) - s(86, scale) - s(76, scale);
    SendMessageW(lv, LVM_SETCOLUMNWIDTH_ID, 0, name_w.max(s(120, scale)) as LPARAM);
}

unsafe fn process_picker_dialog(parent: HWND, scale: u32, cache: &mut PickerCache) -> Option<(String, u32)> {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxPickerDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(picker_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let dlg_w = s(BASE_PICKER_W, scale);
    let dlg_h = s(BASE_PICKER_H, scale);

    let mut wr = RECT { left: 0, top: 0, right: dlg_w, bottom: dlg_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;

    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let pw = parent_rc.right - parent_rc.left;
    let ph = parent_rc.bottom - parent_rc.top;
    let start_x = parent_rc.left + (pw - win_w) / 2;
    let start_y = parent_rc.top + (ph - win_h) / 2;

    let font_name = wstr("Segoe UI");
    let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let title = wstr(&crate::version::window_title("Select Process"));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );

    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);
    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    // Search filter edit box (sits inside the rounded well painted in picker_dlg_proc)
    let e_filter = mk_ex(hwnd, hinst, WC_EDIT_S, "", WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL_C, 0, s(26, scale), s(95, scale), s(190, scale), s(22, scale), IDC_PROC_FILTER, font);
    SendMessageW(e_filter, 0x00D3 /* EM_SETMARGINS */, 1 /* EC_LEFTMARGIN */, s(4, scale) as LPARAM);    uxtheme::set_theme(e_filter, "DarkMode_CFD");
    subclass_edit(e_filter);

    // Windowed processes only Checkbox
    let c_windowed = mk(hwnd, hinst, WC_BUTTON_S, "Windowed processes only", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(236, scale), s(90, scale), dlg_w - s(252, scale), s(32, scale), IDC_PROC_WINDOWED, font);
    subclass_checkbox(c_windowed, false);

    // Process ListView
    let lv_w = dlg_w - s(32, scale);
    let lv_h = dlg_h - s(192, scale);
    let lv = mk_ex(hwnd, hinst, WC_LISTVIEW_S, "", WS_CHILD | WS_VISIBLE | WS_TABSTOP | LVS_REPORT_C | LVS_SINGLESEL_C | LVS_SHOWSELALWAYS_C | LVS_OWNERDRAWFIXED_C, 0, s(16, scale), s(130, scale), lv_w, lv_h, IDC_PROC_LIST, font);
    SendMessageW(lv, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT_E | LVS_EX_DOUBLEBUFFER_E) as LPARAM);
    SendMessageW(lv, LVM_SETBKCOLOR_ID, 0, C_PANEL as LPARAM);
    SendMessageW(lv, LVM_SETTEXTBKCOLOR_ID, 0, C_PANEL as LPARAM);
    SendMessageW(lv, LVM_SETTEXTCOLOR_ID, 0, C_TEXT_BODY as LPARAM);
    let orig_plv = SetWindowLongPtrW(lv, GWLP_WNDPROC, dark_listview_proc as usize as isize);
    SetPropW(lv, wstr("OrigLVProc").as_ptr(), orig_plv as HANDLE);
    uxtheme::set_theme(lv, "DarkMode_Explorer");
    uxtheme::allow_dark_mode_for_window(lv);

    let lv_rgn = CreateRoundRectRgn(0, 0, lv_w, lv_h, s(R_CARD, scale) * 2, s(R_CARD, scale) * 2);
    SetWindowRgn(lv, lv_rgn, 1);

    // Attach Small ImageList (16x16)
    // The image list is reused across openings; icons in it are only rebuilt if the scale changes
    if cache.himl.is_null() || cache.himl_scale != scale {
        if !cache.himl.is_null() {
            ImageList_Destroy(cache.himl);
        }
        cache.himl = ImageList_Create(s(16, scale) as i32, s(16, scale) as i32, ILC_COLOR32 | ILC_MASK, 30, 30);
        cache.himl_scale = scale;
        cache.icons.clear();
        cache.arch.clear();
    }
    let himl = cache.himl;
    SendMessageW(lv, LVM_SETIMAGELIST, LVSIL_SMALL, himl as LPARAM);

    let header = SendMessageW(lv, LVM_GETHEADER_ID, 0, 0) as HWND;
    if !header.is_null() {
        let style = GetWindowLongPtrW(header, GWL_STYLE) as u32;
        SetWindowLongPtrW(header, GWL_STYLE, (style | 0x0002 /* HDS_BUTTONS */) as isize);
        let orig_hdr = SetWindowLongPtrW(header, GWLP_WNDPROC, dark_header_proc as usize as isize);
        SetPropW(header, wstr("OrigHdrProc").as_ptr(), orig_hdr as HANDLE);
        uxtheme::allow_dark_mode_for_window(header);
    }

    let col_proc = wstr("Process Name");
    let mut lvc = LVCOLUMNW { mask: LVCF_TEXT_C | LVCF_WIDTH_C, cx: s(230, scale), pszText: col_proc.as_ptr() as *mut u16, ..Default::default() };
    SendMessageW(lv, LVM_INSERTCOLUMNW, 0, &mut lvc as *mut _ as LPARAM);

    let col_pid = wstr("PID");
    let mut lvc_pid = LVCOLUMNW { mask: LVCF_TEXT_C | LVCF_WIDTH_C, cx: s(86, scale), pszText: col_pid.as_ptr() as *mut u16, ..Default::default() };
    SendMessageW(lv, LVM_INSERTCOLUMNW, 1, &mut lvc_pid as *mut _ as LPARAM);

    let col_arch = wstr("Arch");
    let mut lvc_arch = LVCOLUMNW { mask: LVCF_TEXT_C | LVCF_WIDTH_C, cx: s(76, scale), pszText: col_arch.as_ptr() as *mut u16, ..Default::default() };
    SendMessageW(lv, LVM_INSERTCOLUMNW, 2, &mut lvc_arch as *mut _ as LPARAM);

    let mut state = PickerState {
        filter: String::new(),
        windowed_only: false,
        last_procs: None,
        icon_cache: HashMap::new(),
        himl,
        scale,
        selected_name: String::new(),
        selected_pid: 0,
        selected_icon: None,
        input_brush: gdi32::CreateSolidBrush(C_INPUT_BG),
        list_w: lv_w,
        arch_cache: std::mem::take(&mut cache.arch),
        pending_icons: Vec::new(),
        sort_col: 0,
        sort_asc: true,
    };
    SetPropW(hwnd, wstr("PickerState").as_ptr(), &mut state as *mut _ as HANDLE);

    refresh_picker_if_changed(lv, "", false, &mut state);
    load_pending_icons(lv, &mut state, 16);

    // If an item exists, select first row
    let item_count = SendMessageW(lv, 0x1000 + 4 /* LVM_GETITEMCOUNT */, 0, 0) as i32;
    if item_count > 0 {
        let mut sel_item = LVITEMW {
            mask: 0x0008,
            iItem: 0,
            state: 0x0002 | 0x0001,
            stateMask: 0x0002 | 0x0001,
            ..Default::default()
        };
        SendMessageW(lv, 0x1000 + 43 /* LVM_SETITEMSTATE */, 0, &mut sel_item as *mut _ as LPARAM);
        update_picker_preview_card(hwnd, &mut state);
    }

    // Bottom Select & Cancel
    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, "Select", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, dlg_w - s(220, scale), dlg_h - s(48, scale), s(110, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);

    let b_can = mk(hwnd, hinst, WC_BUTTON_S, "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, dlg_w - s(98, scale), dlg_h - s(48, scale), s(82, scale), s(32, scale), 2, font_bold);
    subclass_button(b_can, false);

    // Set 150ms real-time auto-refresh timer
    SetTimer(hwnd, 999, 150, None);

    // Shown only now: the window is fully set up (dark title bar, controls, scale), so it
    // never appears white or half-built
    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    focus_first_control(hwnd);

    let mut result: Option<(String, u32)> = None;
    let mut msg = new_msg();

    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);

        let userdata = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if userdata == 1 {
            if let Some((name, pid)) = get_picker_selected_info(lv) {
                result = Some((name, pid));
            } else if !state.selected_name.is_empty() {
                result = Some((state.selected_name.clone(), state.selected_pid));
            }
            break;
        } else if userdata == 2 {
            break;
        }
    }

    KillTimer(hwnd, 999);
    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RemovePropW(hwnd, wstr("Scale").as_ptr());
    RemovePropW(hwnd, wstr("AppPtr").as_ptr());
    RemovePropW(hwnd, wstr("Font").as_ptr());
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font);
    gdi32::DeleteObject(font_bold);
    if let Some(old) = state.selected_icon.take() {
        DestroyIcon(old);
    }
    cache.icons = std::mem::take(&mut state.icon_cache);
    cache.arch = std::mem::take(&mut state.arch_cache);
    gdi32::DeleteObject(state.input_brush);

    result
}

unsafe extern "system" fn picker_dlg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => return 1,
        WM_CTLCOLOREDIT => {
            // Without this the filter box falls back to a white system brush
            let st = GetPropW(hwnd, wstr("PickerState").as_ptr()) as *mut PickerState;
            if st.is_null() {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_INPUT_BG);
            (*st).input_brush as LRESULT
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let mem_dc = gdi32::CreateCompatibleDC(hdc);
            let mem_bm = gdi32::CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
            let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

            let brush = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(mem_dc, &rc, brush);
            let _ = gdi32::DeleteObject(brush);

            let st = GetPropW(hwnd, wstr("PickerState").as_ptr()) as *mut PickerState;
            if !st.is_null() {
                let scale = (*st).scale;
                let dlg_w = s(BASE_PICKER_W, scale);
                // With nothing selected the card has no icon, so its text moves to the left edge
                let has_sel = !(&*st).selected_name.is_empty();
                let text_x = if has_sel { s(72, scale) } else { s(24, scale) };

                // Preview Card Container
                let pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
                let pbrush = gdi32::CreateSolidBrush(C_PANEL);
                let ob = gdi32::SelectObject(mem_dc, pbrush);
                let op = gdi32::SelectObject(mem_dc, pen);
                round_rect(mem_dc, s(16, scale), s(12, scale), dlg_w - s(16, scale), s(82, scale), s(10, scale), s(10, scale));
                gdi32::SelectObject(mem_dc, ob);
                gdi32::SelectObject(mem_dc, op);
                let _ = gdi32::DeleteObject(pen);
                let _ = gdi32::DeleteObject(pbrush);

                // Search field well (the filter edit is placed inside it)
                let well_pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_INPUT_BORDER);
                let well_brush = gdi32::CreateSolidBrush(C_INPUT_BG);
                let ow = gdi32::SelectObject(mem_dc, well_brush);
                let owp = gdi32::SelectObject(mem_dc, well_pen);
                round_rect(mem_dc, s(16, scale), s(90, scale), s(226, scale), s(122, scale), s(R_INPUT, scale), s(R_INPUT, scale));
                gdi32::SelectObject(mem_dc, ow);
                gdi32::SelectObject(mem_dc, owp);
                let _ = gdi32::DeleteObject(well_pen);
                let _ = gdi32::DeleteObject(well_brush);

                // Card Header Label
                let font_name = wstr("Segoe UI");
                let font_small = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
                let of = gdi32::SelectObject(mem_dc, font_small);
                gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);
                gdi32::SetTextColor(mem_dc, C_TEXT_HEAD);
                let mut rc_head = RECT {
                    left: text_x,
                    top: s(16, scale),
                    right: dlg_w - s(24, scale),
                    bottom: s(30, scale),
                };
                let w_h = wstr("SELECTED PROCESS PREVIEW");
                gdi32::DrawTextW(mem_dc, w_h.as_ptr(), -1, &mut rc_head, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                // Icon (large 38x38), only when something is selected
                if let (true, Some(hicon)) = (has_sel, (*st).selected_icon) {
                    DrawIconEx(mem_dc, s(24, scale), s(24, scale), hicon, s(38, scale), s(38, scale), 0, NULL_HANDLE, 0x0003 /* DI_NORMAL */);
                }

                // Process Name
                let font_bold = gdi32::CreateFontW(s(-17, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
                gdi32::SelectObject(mem_dc, font_bold);
                gdi32::SetTextColor(mem_dc, C_TEXT_BODY);
                let mut rc_title = RECT {
                    left: text_x,
                    top: s(32, scale),
                    right: dlg_w - s(24, scale),
                    bottom: s(54, scale),
                };
                let st_ref = &*st;
                let disp_name = if !has_sel {
                    if st_ref.filter.is_empty() { "No Process Selected" } else { "No match" }
                } else {
                    &st_ref.selected_name
                };
                let w_n = wstr(disp_name);
                gdi32::DrawTextW(mem_dc, w_n.as_ptr(), -1, &mut rc_title, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                // Subtext (PID and Arch)
                gdi32::SelectObject(mem_dc, font_small);
                gdi32::SetTextColor(mem_dc, C_TEXT_MUTED);
                let mut rc_sub = RECT {
                    left: text_x,
                    top: s(54, scale),
                    right: dlg_w - s(24, scale),
                    bottom: s(74, scale),
                };
                let arch = *(*st).arch_cache.entry((*st).selected_pid).or_insert_with(|| crate::process::process_arch_quick((*st).selected_pid));
                let arch_str = match arch {
                    Some(crate::process::Arch::X64) => "x64",
                    Some(crate::process::Arch::X86) => "x86",
                    None => "-",
                };
                let sub_info = if has_sel {
                    format!("PID: {} (0x{:X})   |   Architecture: {}", (*st).selected_pid, (*st).selected_pid, arch_str)
                } else if !(&*st).filter.is_empty() {
                    format!("Nothing matches \"{}\". Try another name or PID.", (*st).filter)
                } else {
                    "Select an active process from the list below".to_string()
                };
                let w_s = wstr(&sub_info);
                gdi32::DrawTextW(mem_dc, w_s.as_ptr(), -1, &mut rc_sub, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

                gdi32::SelectObject(mem_dc, of);
                let _ = gdi32::DeleteObject(font_small);
                let _ = gdi32::DeleteObject(font_bold);
            }

            gdi32::BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);
            gdi32::SelectObject(mem_dc, old_bm);
            let _ = gdi32::DeleteObject(mem_bm);
            let _ = gdi32::DeleteDC(mem_dc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, dlg_scale(hwnd));
                return 1;
            } else if dis.CtlType == ODT_LISTVIEW {
                let st = GetPropW(hwnd, wstr("PickerState").as_ptr()) as *mut PickerState;
                if !st.is_null() {
                    draw_picker_row(dis, (*st).himl, (*st).scale);
                }
                return 1;
            }
            0
        }
        WM_TIMER => {
            if wp == 999 {
                let st = GetPropW(hwnd, wstr("PickerState").as_ptr()) as *mut PickerState;
                let lv = GetDlgItem(hwnd, IDC_PROC_LIST);
                if !st.is_null() && !lv.is_null() {
                    refresh_picker_if_changed(lv, &(*st).filter, (*st).windowed_only, &mut *st);
                    load_pending_icons(lv, &mut *st, 8);
                }
            }
            0
        }
        WM_NOTIFY => {
            let nmhdr = &*(lp as *const NMHDR);
            let st = GetPropW(hwnd, wstr("PickerState").as_ptr()) as *mut PickerState;
            if nmhdr.idFrom == IDC_PROC_LIST as usize {
                if nmhdr.code == NM_DBLCLK {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 1);
                } else if nmhdr.code == LVN_COLUMNCLICK {
                    if !st.is_null() {
                        let nmlv = &*(lp as *const NMLISTVIEW);
                        let clicked_col = nmlv.iSubItem as usize;
                        if clicked_col <= 2 {
                            if (*st).sort_col == clicked_col {
                                (*st).sort_asc = !(*st).sort_asc;
                            } else {
                                (*st).sort_col = clicked_col;
                                (*st).sort_asc = true;
                            }
                            (*st).last_procs = None;
                            let lv = GetDlgItem(hwnd, IDC_PROC_LIST);
                            refresh_picker_if_changed(lv, &(*st).filter, (*st).windowed_only, &mut *st);
                            update_picker_preview_card(hwnd, st);
                        }
                    }
                } else {
                    update_picker_preview_card(hwnd, st);
                }
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            let code = hiword(wp as u32);
            let st = GetPropW(hwnd, wstr("PickerState").as_ptr()) as *mut PickerState;
            let lv = GetDlgItem(hwnd, IDC_PROC_LIST);

            if id == IDC_PROC_FILTER && code == 0x0300 {
                if !st.is_null() {
                    let e_filter = GetDlgItem(hwnd, IDC_PROC_FILTER);
                    let mut fbuf = [0u16; 128];
                    let flen = GetWindowTextW(e_filter, fbuf.as_mut_ptr(), 128);
                    (*st).filter = if flen > 0 { from_wsz(fbuf.as_ptr()) } else { String::new() };
                    (*st).last_procs = None;
                    refresh_picker_if_changed(lv, &(*st).filter, (*st).windowed_only, &mut *st);
                    update_picker_preview_card(hwnd, st);
                }
            } else if id == IDC_PROC_WINDOWED {
                if !st.is_null() {
                    let c_win = GetDlgItem(hwnd, IDC_PROC_WINDOWED);
                    (*st).windowed_only = get_checkbox_state(c_win);
                    (*st).last_procs = None;
                    refresh_picker_if_changed(lv, &(*st).filter, (*st).windowed_only, &mut *st);
                    update_picker_preview_card(hwnd, st);
                }
            } else if id == 1 || id == 2 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, id as LONG_PTR);
            }
            0
        }
        WM_CLOSE => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 2);
            0
        }
        WM_NCDESTROY => {
            RemovePropW(hwnd, wstr("PickerState").as_ptr());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ===== Settings Dialog (Fully Scaled & Sleek) =====
unsafe fn paint_dialog_card(hdc: HDC, x: i32, y: i32, w: i32, h: i32, title: &str, scale: u32) {
    let p_pen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
    let p_brush = gdi32::CreateSolidBrush(C_PANEL);
    let old_p = gdi32::SelectObject(hdc, p_pen);
    let old_b = gdi32::SelectObject(hdc, p_brush);
    round_rect(hdc, x, y, x + w, y + h, s(10, scale), s(10, scale));
    gdi32::SelectObject(hdc, old_b);
    gdi32::SelectObject(hdc, old_p);
    let _ = gdi32::DeleteObject(p_pen);
    let _ = gdi32::DeleteObject(p_brush);

    let font_name = wstr("Segoe UI");
    let font_small = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let old_f = gdi32::SelectObject(hdc, font_small);
    gdi32::SetBkMode(hdc, gdi32::TRANSPARENT);
    gdi32::SetTextColor(hdc, C_TEXT_HEAD);

    let mut rc = RECT { left: x + s(12, scale), top: y + s(6, scale), right: x + w - s(12, scale), bottom: y + s(22, scale) };
    let wt = wstr(title);
    gdi32::DrawTextW(hdc, wt.as_ptr(), -1, &mut rc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

    gdi32::SelectObject(hdc, old_f);
    let _ = gdi32::DeleteObject(font_small);
}

unsafe fn show_settings_dialog(parent: HWND, app: &mut App) {
    // Always fit on screen: a too-large saved UI scale must never lock the user out of Settings
    let scale = fit_dialog_scale(resolve_scale(app.settings.ui_scale), BASE_SETTINGS_W, BASE_SETTINGS_H);
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxSettingsDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(settings_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let dlg_w = s(BASE_SETTINGS_W, scale);
    let dlg_h = s(BASE_SETTINGS_H, scale);

    let mut wr = RECT { left: 0, top: 0, right: dlg_w, bottom: dlg_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;

    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let pw = parent_rc.right - parent_rc.left;
    let ph = parent_rc.bottom - parent_rc.top;
    let start_x = parent_rc.left + (pw - win_w) / 2;
    let start_y = parent_rc.top + (ph - win_h) / 2;

    let font_name = wstr("Segoe UI");
    let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let title = wstr(&crate::version::window_title("Settings"));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );

    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);
    SetPropW(hwnd, wstr("AppPtr").as_ptr(), app as *mut App as HANDLE);
    SetPropW(hwnd, wstr("Font").as_ptr(), font as HANDLE);

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let card_w = s(BASE_SETTINGS_W - 32, scale);

    // 1. Options Checkboxes
    let c_auto = mk(hwnd, hinst, WC_BUTTON_S, "Auto-inject when process opens", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(42, scale), card_w - s(24, scale), s(22, scale), IDC_SET_AUTO, font);
    subclass_checkbox(c_auto, app.settings.auto_inject);

    let c_close = mk(hwnd, hinst, WC_BUTTON_S, "Close injector automatically after injection", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(68, scale), card_w - s(24, scale), s(22, scale), IDC_SET_CLOSE, font);
    subclass_checkbox(c_close, app.settings.close_on_inject);

    let c_stealth = mk(hwnd, hinst, WC_BUTTON_S, "Stealth inject (ErasePE + HideModule + hide debugger)", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(94, scale), card_w - s(24, scale), s(22, scale), IDC_SET_STEALTH, font);
    subclass_checkbox(c_stealth, app.settings.stealth_inject);

    let c_erase = mk(hwnd, hinst, WC_BUTTON_S, "Erase PE headers from memory", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(120, scale), card_w - s(24, scale), s(22, scale), IDC_SET_ERASE, font);
    subclass_checkbox(c_erase, app.settings.erase_pe);

    let c_hide = mk(hwnd, hinst, WC_BUTTON_S, "Hide injected module from PEB InLoadOrder list", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(146, scale), card_w - s(24, scale), s(22, scale), IDC_SET_HIDE, font);
    subclass_checkbox(c_hide, app.settings.hide_module);

    let c_hdbg = mk(hwnd, hinst, WC_BUTTON_S, "ThreadHideFromDebugger (Anti-Debug flag)", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(172, scale), card_w - s(24, scale), s(22, scale), IDC_SET_HDBG, font);
    subclass_checkbox(c_hdbg, app.settings.advanced.hide_from_debugger);

    // Auto-inject behaviour
    let c_wait_win = mk(hwnd, hinst, WC_BUTTON_S, "Wait for the target window before injecting", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(242, scale), card_w - s(24, scale), s(22, scale), IDC_SET_WAIT_WINDOW, font);
    subclass_checkbox(c_wait_win, app.settings.auto_wait_window);
    let c_retry = mk(hwnd, hinst, WC_BUTTON_S, "Retry a failed file two more times", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(266, scale), card_w - s(24, scale), s(22, scale), IDC_SET_RETRY, font);
    subclass_checkbox(c_retry, app.settings.auto_retry);
    let c_children = mk(hwnd, hinst, WC_BUTTON_S, "Also inject child processes that have a window", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(290, scale), card_w - s(24, scale), s(22, scale), IDC_SET_CHILDREN, font);
    subclass_checkbox(c_children, app.settings.inject_children);

    let e_child = mk_ex(hwnd, hinst, WC_EDIT_S, &app.settings.auto_child_filter, WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, s(160, scale), s(324, scale), s(220, scale), s(20, scale), IDC_SET_CHILD_FILTER, font);
    uxtheme::set_theme(e_child, "DarkMode_CFD");
    subclass_edit(e_child);
    let e_title = mk_ex(hwnd, hinst, WC_EDIT_S, &app.settings.auto_window_title, WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, s(168, scale), s(364, scale), s(212, scale), s(20, scale), IDC_SET_TITLE_FILTER, font);
    uxtheme::set_theme(e_title, "DarkMode_CFD");
    subclass_edit(e_title);

    // 2. Global timing (centered white text, inside styled rounded container)
    let e_del = mk_ex(hwnd, hinst, WC_EDIT_S, &format!("{}", app.settings.delay), WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_NUMBER_C | 0x0001 /* ES_CENTER */, 0, s(205, scale), s(448, scale), s(80, scale), s(20, scale), IDC_DELAY, font);
    uxtheme::set_theme(e_del, "DarkMode_CFD");
    subclass_edit(e_del);

    let e_del_b = mk_ex(hwnd, hinst, WC_EDIT_S, &format!("{}", app.settings.delay_between), WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_NUMBER_C | 0x0001 /* ES_CENTER */, 0, s(225, scale), s(482, scale), s(80, scale), s(20, scale), IDC_DELAY_BETWEEN, font);
    uxtheme::set_theme(e_del_b, "DarkMode_CFD");
    subclass_edit(e_del_b);

    // 3. Advanced Tools Buttons
    let btn_w = s(170, scale);
    let btn_gap = s(8, scale);
    let start_x = s(16, scale) + (card_w - (btn_w * 2 + btn_gap)) / 2;
    let b_scr = mk(hwnd, hinst, WC_BUTTON_S, "PE Scrambler Settings...", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, start_x, s(554, scale), btn_w, s(32, scale), IDC_SCRAMBLE_BTN, font_bold);
    subclass_button(b_scr, false);

    let b_adv = mk(hwnd, hinst, WC_BUTTON_S, "Advanced Cloaking...", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, start_x + btn_w + btn_gap, s(554, scale), btn_w, s(32, scale), IDC_ADV_BTN, font_bold);
    subclass_button(b_adv, false);

    // 4. Interface UI Scale Dropdown (Custom Owner-Drawn Obsidian)
    let combo_scale = mk_ex(hwnd, hinst, WC_COMBOBOX_S, "", WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST_C | CBS_HASSTRINGS_C, 0, s(132, scale), s(634, scale), card_w - s(144, scale), s(200, scale), IDC_SCALE_COMBO, font);
    SendMessageW(combo_scale, CB_SETDROPPEDWIDTH, s(240, scale) as WPARAM, 0);
    fit_combo_closed_height(combo_scale, scale);
    SendMessageW(combo_scale, CB_SETITEMHEIGHT, 0, s(26, scale) as LPARAM);

    // Percentages are relative to the system DPI (see resolve_scale)
    let scale_options: &[(u32, &str)] = &[
        (0, "Auto (System DPI)"),
        (75, "75%"),
        (80, "80%"),
        (90, "90%"),
        (100, "100% (Standard)"),
        (110, "110%"),
        (125, "125%"),
        (150, "150% (Max)"),
    ];

    let mut active_scale_idx = 0;
    for (i, &(val, text)) in scale_options.iter().enumerate() {
        SendMessageW(combo_scale, CB_ADDSTRING, 0, wstr(text).as_ptr() as LPARAM);
        if val == app.settings.ui_scale {
            active_scale_idx = i;
        }
    }
    SendMessageW(combo_scale, CB_SETCURSEL, active_scale_idx as WPARAM, 0);
    subclass_combobox(combo_scale, scale, C_PANEL, font);

    // 5. Settings scope: one setup for everything, or one per target process
    let c_perproc = mk(hwnd, hinst, WC_BUTTON_S, "Remember a separate setup for each process", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(714, scale), card_w - s(24, scale), s(22, scale), IDC_SET_PERPROC, font);
    subclass_checkbox(c_perproc, app.settings.scope == SettingsScope::PerProcess);

    // Log-only mode: no modal popups, everything goes to the activity log
    let c_logonly = mk(hwnd, hinst, WC_BUTTON_S, "Log only: no popup dialogs (errors go to the log)", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), s(740, scale), card_w - s(24, scale), s(22, scale), IDC_SET_LOGONLY, font);
    subclass_checkbox(c_logonly, app.settings.log_only);

    // 6. Bottom About / Cancel / Save (Save is the rightmost, like the main window's primary action)
    let b_about = mk(hwnd, hinst, WC_BUTTON_S, "About", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(16, scale), s(774, scale), s(88, scale), s(32, scale), IDC_ABOUT, font_bold);
    subclass_button(b_about, false);

    let b_cancel = mk(hwnd, hinst, WC_BUTTON_S, "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_SETTINGS_W - 16 - 120 - 8 - 96, scale), s(774, scale), s(96, scale), s(32, scale), 2, font_bold);
    subclass_button(b_cancel, false);

    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, "Save Settings", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(BASE_SETTINGS_W - 16 - 120, scale), s(774, scale), s(120, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);
    add_debug_controls(hwnd, hinst, scale, font_bold);

    // Shown only now: the window is fully set up (dark title bar, controls, scale), so it
    // never appears white or half-built
    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    focus_first_control(hwnd);

    let mut saved = false;
    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        let userdata = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if userdata != 0 {
            if userdata == 1 {
                app.settings.scope = if get_checkbox_state(c_perproc) {
                    SettingsScope::PerProcess
                } else {
                    SettingsScope::Global
                };
                app.settings.auto_inject = get_checkbox_state(c_auto);
                app.settings.auto_wait_window = get_checkbox_state(c_wait_win);
                app.settings.auto_retry = get_checkbox_state(c_retry);
                app.settings.inject_children = get_checkbox_state(c_children);
                app.settings.auto_child_filter = read_edit_text(e_child);
                app.settings.auto_window_title = read_edit_text(e_title);
                app.settings.close_on_inject = get_checkbox_state(c_close);
                app.settings.log_only = get_checkbox_state(c_logonly);
                LOG_ONLY.store(app.settings.log_only, std::sync::atomic::Ordering::Relaxed);
                app.settings.stealth_inject = get_checkbox_state(c_stealth);
                app.settings.erase_pe = get_checkbox_state(c_erase);
                app.settings.hide_module = get_checkbox_state(c_hide);
                app.settings.advanced.hide_from_debugger = get_checkbox_state(c_hdbg);

                let mut b = [0u16; 16];
                if GetWindowTextW(e_del, b.as_mut_ptr(), 16) > 0 {
                    app.settings.delay = from_wsz(b.as_ptr()).parse().unwrap_or(0);
                }
                if GetWindowTextW(e_del_b, b.as_mut_ptr(), 16) > 0 {
                    app.settings.delay_between = from_wsz(b.as_ptr()).parse().unwrap_or(0);
                }

                // Read UI Scale
                let scale_sel = SendMessageW(combo_scale, CB_GETCURSEL, 0, 0) as usize;
                if scale_sel < scale_options.len() {
                    let new_scale = scale_options[scale_sel].0;
                    app.settings.ui_scale = new_scale;
                }

                saved = true;
            }
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RemovePropW(hwnd, wstr("Scale").as_ptr());
    RemovePropW(hwnd, wstr("AppPtr").as_ptr());
    RemovePropW(hwnd, wstr("Font").as_ptr());
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font);
    gdi32::DeleteObject(font_bold);

    if saved {
        // Keep the live setup as this target's profile, or stop tracking one
        if app.settings.scope == SettingsScope::PerProcess {
            let name = app.settings.process_name.clone();
            if !name.trim().is_empty() {
                app.settings.remember_profile(&name);
                app.active_profile = name;
            }
        } else {
            app.active_profile.clear();
        }

        let mut sh = app.shared.lock().unwrap();
        sh.auto_inject = app.settings.auto_inject;
        sh.close_on_inject = app.settings.close_on_inject;
        sh.auto_wait_window = app.settings.auto_wait_window;
        sh.auto_retry = app.settings.auto_retry;
        sh.inject_children = app.settings.inject_children;
        sh.auto_child_filter = app.settings.auto_child_filter.clone();
        sh.auto_window_title = app.settings.auto_window_title.clone();
        sh.delay = app.settings.delay;
        sh.delay_between = app.settings.delay_between;
        sh.opts = app.settings.inject_options();
        drop(sh);

        apply_scale(parent, app);
        app.settings.save();
    }
}

unsafe extern "system" fn settings_dlg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
    let scale = if scale > 0 { scale } else { 100 };

    match msg {
        WM_ERASEBKGND => return 1,
        WM_CTLCOLOREDIT => {
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_INPUT_BG);
            app_brush(hwnd, |a| a.input_brush)
        }
        0x020A /* WM_MOUSEWHEEL */ => {
            // Route the wheel to the log whenever the cursor is over it, focused or not
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            let panel = GetDlgItem(hwnd, IDC_LOG_PANEL);
            if !panel.is_null() {
                let mut prc = RECT::default();
                GetWindowRect(panel, &mut prc);
                if pt.x >= prc.left && pt.x < prc.right && pt.y >= prc.top && pt.y < prc.bottom {
                    SendMessageW(panel, msg, wp, lp);
                    return 0;
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_PANEL);
            app_brush(hwnd, |a| a.panel_brush)
        }
        WM_CTLCOLORLISTBOX => {
            let lbox = lp as HWND;
            if !lbox.is_null() {
                let font = GetPropW(hwnd, wstr("Font").as_ptr()) as HFONT;
                subclass_lbox(lbox, scale, font);
            }
            let hdc = wp as HDC;
            gdi32::SetTextColor(hdc, C_TEXT_BODY);
            gdi32::SetBkColor(hdc, C_INPUT_BG);
            app_brush(hwnd, |a| a.input_brush)
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, scale);
                return 1;
            } else if dis.CtlType == ODT_COMBOBOX {
                let font = GetPropW(hwnd, wstr("Font").as_ptr()) as HFONT;
                draw_owner_combobox(dis, scale, font);
                return 1;
            }
            0
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let mem_dc = gdi32::CreateCompatibleDC(hdc);
            let mem_bm = gdi32::CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
            let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

            let brush = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(mem_dc, &rc, brush);
            let _ = gdi32::DeleteObject(brush);

            let w = s(BASE_SETTINGS_W - 32, scale);

            paint_dialog_card(mem_dc, s(16, scale), s(14, scale), w, s(192, scale), "CLOAKING & EVASION OPTIONS", scale);
            paint_dialog_card(mem_dc, s(16, scale), s(214, scale), w, s(190, scale), "AUTO-INJECT", scale);
            paint_dialog_card(mem_dc, s(16, scale), s(412, scale), w, s(106, scale), "GLOBAL TIMING & DELAY", scale);

            // In the global timing card: rounded input containers with a 1px border
            let ipen = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
            let ibrush = gdi32::CreateSolidBrush(C_INPUT_BG);
            let o_ip = gdi32::SelectObject(mem_dc, ipen);
            let o_ib = gdi32::SelectObject(mem_dc, ibrush);
            round_rect(mem_dc, s(203, scale), s(442, scale), s(287, scale), s(474, scale), s(R_INPUT, scale), s(R_INPUT, scale));
            round_rect(mem_dc, s(223, scale), s(476, scale), s(307, scale), s(508, scale), s(R_INPUT, scale), s(R_INPUT, scale));
            gdi32::SelectObject(mem_dc, o_ib);
            gdi32::SelectObject(mem_dc, o_ip);
            let _ = gdi32::DeleteObject(ipen);
            let _ = gdi32::DeleteObject(ibrush);

            let font_name = wstr("Segoe UI");
            let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let old_f = gdi32::SelectObject(mem_dc, font);
            gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);
            gdi32::SetTextColor(mem_dc, C_TEXT_HEAD);

            let mut rc_l1 = RECT { left: s(28, scale), top: s(442, scale), right: s(252, scale), bottom: s(474, scale) };
            let wl1 = wstr("Global start delay (ms):");
            gdi32::DrawTextW(mem_dc, wl1.as_ptr(), -1, &mut rc_l1, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            let mut rc_l2 = RECT { left: s(28, scale), top: s(476, scale), right: s(252, scale), bottom: s(508, scale) };
            let wl2 = wstr("Global between DLLs (ms):");
            gdi32::DrawTextW(mem_dc, wl2.as_ptr(), -1, &mut rc_l2, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            // Auto-inject filters: rounded input containers and labels
            let ipen2 = gdi32::CreatePen(gdi32::PS_SOLID, 1, C_PANEL_BORDER);
            let ibrush2 = gdi32::CreateSolidBrush(C_INPUT_BG);
            let o_ip2 = gdi32::SelectObject(mem_dc, ipen2);
            let o_ib2 = gdi32::SelectObject(mem_dc, ibrush2);
            round_rect(mem_dc, s(158, scale), s(318, scale), s(382, scale), s(350, scale), s(R_INPUT, scale), s(R_INPUT, scale));
            round_rect(mem_dc, s(166, scale), s(358, scale), s(382, scale), s(390, scale), s(R_INPUT, scale), s(R_INPUT, scale));
            gdi32::SelectObject(mem_dc, o_ib2);
            gdi32::SelectObject(mem_dc, o_ip2);
            let _ = gdi32::DeleteObject(ipen2);
            let _ = gdi32::DeleteObject(ibrush2);

            let mut rc_cf = RECT { left: s(28, scale), top: s(318, scale), right: s(240, scale), bottom: s(350, scale) };
            let w_cf = wstr("Child name filter:");
            gdi32::DrawTextW(mem_dc, w_cf.as_ptr(), -1, &mut rc_cf, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
            let mut rc_tf = RECT { left: s(28, scale), top: s(358, scale), right: s(240, scale), bottom: s(390, scale) };
            let w_tf = wstr("Window title filter:");
            gdi32::DrawTextW(mem_dc, w_tf.as_ptr(), -1, &mut rc_tf, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            // Card 3: TOOLS
            paint_dialog_card(mem_dc, s(16, scale), s(526, scale), w, s(72, scale), "ADVANCED PE TOOLS", scale);

            // Card 4: INTERFACE & DISPLAY
            paint_dialog_card(mem_dc, s(16, scale), s(606, scale), w, s(72, scale), "INTERFACE & DISPLAY", scale);

            let mut rc_sc = RECT { left: s(28, scale), top: s(634, scale), right: s(128, scale), bottom: s(666, scale) };
            let w_sc = wstr("UI Zoom Scale:");
            gdi32::DrawTextW(mem_dc, w_sc.as_ptr(), -1, &mut rc_sc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            // Card 5: SETTINGS SCOPE
            paint_dialog_card(mem_dc, s(16, scale), s(686, scale), w, s(80, scale), "SETTINGS SCOPE", scale);
            // The hint gets its own smaller font and a full line height, so descenders fit
            let font_small = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            gdi32::SelectObject(mem_dc, font_small);
            gdi32::SetTextColor(mem_dc, C_TEXT_MUTED);
            let mut rc_scope = RECT {
                left: s(28, scale),
                top: s(738, scale),
                right: s(16, scale) + w - s(12, scale),
                bottom: s(762, scale),
            };
            let w_scope = wstr("One setup for everything, or one for each target.");
            gdi32::DrawTextW(mem_dc, w_scope.as_ptr(), -1, &mut rc_scope, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
            gdi32::SelectObject(mem_dc, font);
            let _ = gdi32::DeleteObject(font_small);

            gdi32::SelectObject(mem_dc, old_f);
            let _ = gdi32::DeleteObject(font);

            gdi32::BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);
            gdi32::SelectObject(mem_dc, old_bm);
            let _ = gdi32::DeleteObject(mem_bm);
            let _ = gdi32::DeleteDC(mem_dc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            let code = hiword(wp as u32);
            let app_ptr = GetPropW(hwnd, wstr("AppPtr").as_ptr()) as *mut App;

            if id == IDC_SCALE_COMBO && (code == 1 /* CBN_SELCHANGE */ || code == 8 /* CBN_CLOSEUP */) {
                let cb = GetDlgItem(hwnd, id);
                InvalidateRect(cb, std::ptr::null(), 0);
            } else if id == IDC_SCRAMBLE_BTN {
                if !app_ptr.is_null() {
                    scramble_options_dialog(hwnd, &mut (*app_ptr).settings.scramble, scale);
                }
            } else if id == IDC_ADV_BTN {
                if !app_ptr.is_null() {
                    advanced_options_dialog(hwnd, &mut (*app_ptr).settings.advanced, scale);
                }
            } else if id == IDC_ABOUT {
                show_about_dialog(hwnd, scale);
            } else if id == IDC_DEBUG_POPUPS {
                show_popup_samples(hwnd, scale);
            } else if id == 1 || id == 2 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, id as LONG_PTR);
            }
            0
        }
        WM_CLOSE => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 2);
            0
        }
        WM_NCDESTROY => {
            RemovePropW(hwnd, wstr("Scale").as_ptr());
            RemovePropW(hwnd, wstr("AppPtr").as_ptr());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn scramble_options_dialog(parent: HWND, sc: &mut ScrambleOptions, scale: u32) {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxScrambleDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(generic_modal_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let dlg_w = s(420, scale);
    let dlg_h = s(296, scale);

    let mut wr = RECT { left: 0, top: 0, right: dlg_w, bottom: dlg_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;

    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let pw = parent_rc.right - parent_rc.left;
    let ph = parent_rc.bottom - parent_rc.top;
    let start_x = parent_rc.left + (pw - win_w) / 2;
    let start_y = parent_rc.top + (ph - win_h) / 2;

    let font_name = wstr("Segoe UI");
    let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let title = wstr(&crate::version::window_title("PE Scrambler"));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );

    // Kept alive for the whole dialog: the property holds a pointer to this buffer
    let card_title = wstr("PE HEADER & CODE SCRAMBLING");
    SetPropW(hwnd, wstr("CardTitle").as_ptr(), card_title.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let items = [
        ("Scramble PE header fields", sc.scramble_header_fields),
        ("Remove debug data / CV directories", sc.remove_debug_data),
        ("Remove useless / junk data", sc.remove_useless_data),
        ("Modify assembly prologue signatures", sc.modify_assembly_code),
        ("Modify import table structure", sc.modify_import_table),
        ("Move relocation table", sc.move_relocation_table),
        ("Insert decoy extra sections", sc.insert_extra_sections),
    ];

    let mut cbs = Vec::new();
    let mut cur_y = s(42, scale);
    for (text, initial) in items {
        let cb = mk(hwnd, hinst, WC_BUTTON_S, text, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), cur_y, dlg_w - s(56, scale), s(22, scale), 400 + cbs.len() as i32, font);
        subclass_checkbox(cb, initial);
        cbs.push(cb);
        cur_y += s(26, scale);
    }

    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, "Apply", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, dlg_w - s(194, scale), dlg_h - s(48, scale), s(88, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);

    let b_can = mk(hwnd, hinst, WC_BUTTON_S, "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, dlg_w - s(96, scale), dlg_h - s(48, scale), s(80, scale), s(32, scale), 2, font_bold);
    subclass_button(b_can, false);

    // Shown only now: the window is fully set up (dark title bar, controls, scale), so it
    // never appears white or half-built
    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    focus_first_control(hwnd);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        let userdata = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if userdata != 0 {
            if userdata == 1 {
                sc.scramble_header_fields = get_checkbox_state(cbs[0]);
                sc.remove_debug_data = get_checkbox_state(cbs[1]);
                sc.remove_useless_data = get_checkbox_state(cbs[2]);
                sc.modify_assembly_code = get_checkbox_state(cbs[3]);
                sc.modify_import_table = get_checkbox_state(cbs[4]);
                sc.move_relocation_table = get_checkbox_state(cbs[5]);
                sc.insert_extra_sections = get_checkbox_state(cbs[6]);
            }
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RemovePropW(hwnd, wstr("Scale").as_ptr());
    RemovePropW(hwnd, wstr("AppPtr").as_ptr());
    RemovePropW(hwnd, wstr("Font").as_ptr());
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font);
    gdi32::DeleteObject(font_bold);
}

unsafe fn advanced_options_dialog(parent: HWND, adv: &mut AdvancedOptions, scale: u32) {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxAdvDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(generic_modal_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let dlg_w = s(390, scale);
    let dlg_h = s(224, scale);

    let mut wr = RECT { left: 0, top: 0, right: dlg_w, bottom: dlg_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;

    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let pw = parent_rc.right - parent_rc.left;
    let ph = parent_rc.bottom - parent_rc.top;
    let start_x = parent_rc.left + (pw - win_w) / 2;
    let start_y = parent_rc.top + (ph - win_h) / 2;

    let font_name = wstr("Segoe UI");
    let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let title = wstr(&crate::version::window_title("Advanced Cloaking"));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );

    // Kept alive for the whole dialog: the property holds a pointer to this buffer
    let card_title = wstr("ADVANCED CLOAKING & HOOKING");
    SetPropW(hwnd, wstr("CardTitle").as_ptr(), card_title.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let items = [
        ("Zero the Load Config (SEH / CFG metadata)", adv.disable_seh_validation),
        ("Drop the exception directory (.pdata)", adv.disable_exception_support),
        ("ThreadHideFromDebugger (Anti-Debug flag)", adv.hide_from_debugger),
        ("Resolve imports from the target's export tables", adv.manual_resolve_imports),
    ];

    let mut cbs = Vec::new();
    let mut cur_y = s(42, scale);
    for (text, initial) in items {
        let cb = mk(hwnd, hinst, WC_BUTTON_S, text, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(28, scale), cur_y, dlg_w - s(56, scale), s(22, scale), 500 + cbs.len() as i32, font);
        subclass_checkbox(cb, initial);
        cbs.push(cb);
        cur_y += s(28, scale);
    }

    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, "Apply", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, dlg_w - s(194, scale), dlg_h - s(48, scale), s(88, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);

    let b_can = mk(hwnd, hinst, WC_BUTTON_S, "Cancel", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, dlg_w - s(96, scale), dlg_h - s(48, scale), s(80, scale), s(32, scale), 2, font_bold);
    subclass_button(b_can, false);

    // Shown only now: the window is fully set up (dark title bar, controls, scale), so it
    // never appears white or half-built
    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    focus_first_control(hwnd);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        let userdata = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if userdata != 0 {
            if userdata == 1 {
                adv.disable_seh_validation = get_checkbox_state(cbs[0]);
                adv.disable_exception_support = get_checkbox_state(cbs[1]);
                adv.hide_from_debugger = get_checkbox_state(cbs[2]);
                adv.manual_resolve_imports = get_checkbox_state(cbs[3]);
            }
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RemovePropW(hwnd, wstr("Scale").as_ptr());
    RemovePropW(hwnd, wstr("AppPtr").as_ptr());
    RemovePropW(hwnd, wstr("Font").as_ptr());
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font);
    gdi32::DeleteObject(font_bold);
}

// Hidden by default: set BLACKBOX_DEBUG_POPUPS=1 to get a "Test popups" button that previews the
// popup styles. Normal runs never show it.
unsafe fn add_debug_controls(hwnd: HWND, hinst: HINSTANCE, scale: u32, font: HFONT) {
    if std::env::var("BLACKBOX_DEBUG_POPUPS").is_err() {
        return;
    }
    let b = mk(hwnd, hinst, WC_BUTTON_S, "Test popups", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, s(116, scale), s(774, scale), s(120, scale), s(32, scale), IDC_DEBUG_POPUPS, font);
    subclass_button(b, false);
}

unsafe fn show_popup_samples(parent: HWND, scale: u32) {
    show_message(
        parent, scale, MsgKind::Warning, "Warning", "No items are checked.",
        "Tick at least one file in the list to inject it.", "E1003 NO_ITEMS",
        vec!["Items in list: 2, checked: 0".to_string()],
    );
    show_message(
        parent, scale, MsgKind::Error, "Error",
        "Process 'some_long_process_name.exe' is not running and could not be found in the process list.",
        "Start the process, or pick it again with Select Process.", "E1002 PROCESS_NOT_RUNNING",
        vec![
            "Target name: some_long_process_name.exe".to_string(),
            "Target PID: not resolved".to_string(),
            "Mode: Standard (CreateRemoteThread + LoadLibrary)".to_string(),
            "Items in list: 2, checked: 2".to_string(),
        ],
    );
    show_message(parent, scale, MsgKind::Success, "Success", "Injected 2 DLLs.", "", "", Vec::new());
}

// Popup button IDs. OK is 1 and Cancel is 2, as in the other dialogs.
const IDC_MSG_COPY: i32 = 3;

// Kinds of popup. The kind sets the icon colour and its mark.
#[derive(Clone, Copy)]
enum MsgKind {
    Success,
    Warning,
    Error,
}

fn msg_accent(kind: MsgKind) -> COLORREF {
    match kind {
        MsgKind::Success => 0x0080_DE4A, // #4ade80
        MsgKind::Warning => 0x0037_B5F0, // #f0b537
        MsgKind::Error => 0x004D_48E5,   // #e5484d
    }
}

fn msg_kind_from(v: usize) -> MsgKind {
    match v {
        0 => MsgKind::Success,
        1 => MsgKind::Warning,
        _ => MsgKind::Error,
    }
}

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(hWndNewOwner: HWND) -> BOOL;
    fn EmptyClipboard() -> BOOL;
    fn SetClipboardData(uFormat: u32, hMem: HANDLE) -> HANDLE;
    fn CloseClipboard() -> BOOL;
    fn GetKeyState(nVirtKey: i32) -> i16;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> HANDLE;
    fn GlobalLock(hMem: HANDLE) -> *mut c_void_raw;
    fn GlobalUnlock(hMem: HANDLE) -> BOOL;
}

const CF_UNICODETEXT_C: u32 = 13;
const GMEM_MOVEABLE_C: u32 = 0x0002;

// Puts UTF-16 text (ending in a 0) on the clipboard. Returns false when the clipboard is not available.
unsafe fn copy_text_to_clipboard(owner: HWND, text: &[u16]) -> bool {
    if OpenClipboard(owner) == 0 {
        return false;
    }
    EmptyClipboard();
    let mem = GlobalAlloc(GMEM_MOVEABLE_C, text.len() * 2);
    if mem.is_null() {
        CloseClipboard();
        return false;
    }
    let dst = GlobalLock(mem) as *mut u16;
    std::ptr::copy_nonoverlapping(text.as_ptr(), dst, text.len());
    GlobalUnlock(mem);
    // Once SetClipboardData succeeds, the clipboard owns the memory
    SetClipboardData(CF_UNICODETEXT_C, mem);
    CloseClipboard();
    true
}

// Puts the popup's report on the clipboard and marks the copy button as done
unsafe fn popup_copy(dlg: HWND) {
    let ptr = GetPropW(dlg, wstr("MsgReport").as_ptr()) as *const u16;
    if ptr.is_null() {
        return;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let report = std::slice::from_raw_parts(ptr, len + 1);
    if copy_text_to_clipboard(dlg, report) {
        let b = GetDlgItem(dlg, IDC_MSG_COPY);
        if !b.is_null() {
            SetWindowTextW(b, wstr("Copied").as_ptr());
        }
    }
}

// Current time in UTC, as "2026-11-01 00:40:12 UTC", for the copied report
fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from a day count (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60
    )
}

// Lines that describe the run: target, PID, mode, and how many items are in the list and checked
fn run_context(app: &App) -> Vec<String> {
    let sh = app.shared.lock().unwrap();
    let mode = Settings::METHODS.get(app.settings.method as usize).copied().unwrap_or("unknown");
    let name = if sh.process_name.is_empty() { "(none)" } else { sh.process_name.as_str() };
    let pid = if sh.target_pid > 0 { sh.target_pid.to_string() } else { "not resolved".to_string() };
    vec![
        format!("Target name: {}", name),
        format!("Target PID: {}", pid),
        format!("Mode: {}", mode),
        format!("Items in list: {}, checked: {}", sh.modules.len(), sh.modules.iter().filter(|m| m.enabled).count()),
    ]
}

// Coloured circle with a mark in it: a cross for errors, an exclamation mark for warnings, a tick for success.
// Drawn anti-aliased, like the other icons in the app.
unsafe fn draw_msg_icon(hdc: HDC, x: i32, y: i32, d: i32, kind: MsgKind) {
    let wu = d.max(1) as usize;
    let f = d as f32;
    let c = f / 2.0;
    let mut px = vec![colorref_to_dib(C_BG); wu * wu];
    draw_aa_stroke(&mut px, wu, wu, (c, c), (c, c), c - 0.5, msg_accent(kind));
    let ink = match kind {
        MsgKind::Warning => 0x001E_1E1E,
        _ => 0x00FF_FFFF,
    };
    let w = (f * 0.05).max(1.2);
    match kind {
        MsgKind::Error => {
            draw_aa_stroke(&mut px, wu, wu, (f * 0.34, f * 0.34), (f * 0.66, f * 0.66), w, ink);
            draw_aa_stroke(&mut px, wu, wu, (f * 0.66, f * 0.34), (f * 0.34, f * 0.66), w, ink);
        }
        MsgKind::Warning => {
            draw_aa_stroke(&mut px, wu, wu, (f * 0.5, f * 0.24), (f * 0.5, f * 0.58), w, ink);
            draw_aa_stroke(&mut px, wu, wu, (f * 0.5, f * 0.72), (f * 0.5, f * 0.72), w, ink);
        }
        MsgKind::Success => {
            draw_aa_stroke(&mut px, wu, wu, (f * 0.28, f * 0.52), (f * 0.44, f * 0.68), w, ink);
            draw_aa_stroke(&mut px, wu, wu, (f * 0.44, f * 0.68), (f * 0.72, f * 0.36), w, ink);
        }
    }
    blit_dib(hdc, x, y, wu, wu, &px);
}

// Themed popup that replaces MessageBoxW. On the left, a coloured icon. On the right, the heading, what happened,
// and what to do next. Errors also get a "Copy error code" button. The copied report has the code, the run
// details and the time. Ctrl+C copies the report too.
unsafe fn show_message(
    parent: HWND, scale: u32, kind: MsgKind, title: &str, text: &str, hint: &str, code: &str, details: Vec<String>,
) {
    if LOG_ONLY.load(std::sync::atomic::Ordering::Relaxed) { return; }
    let hinst = get_module_handle();
    let cls = wstr("AppMsgDlg_520");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(msg_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let font_name = wstr("Segoe UI");
    let font_body = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    // Layout in pixels: the icon on the left, the text column beside it, the buttons along the bottom
    let pad = s(20, scale);
    let icon_d = s(40, scale);
    let text_x = pad + icon_d + s(16, scale);
    let client_w = s(BASE_MSG_W, scale);
    let text_w = client_w - text_x - pad;
    let body = if hint.is_empty() { text.to_string() } else { format!("{}\r\n\r\n{}", text, hint) };
    let wbody = wstr(&body);
    let mut measure = RECT { left: 0, top: 0, right: text_w, bottom: 0 };
    let mdc = GetDC(parent);
    let old_mf = gdi32::SelectObject(mdc, font_body);
    gdi32::DrawTextW(mdc, wbody.as_ptr(), -1, &mut measure, DT_LEFT_FLAG | DT_WORDBREAK_FLAG | DT_NOPREFIX_FLAG | DT_CALCRECT_FLAG);
    gdi32::SelectObject(mdc, old_mf);
    ReleaseDC(parent, mdc);
    let body_top = pad + s(28, scale);
    // The error code sits under the body text on a line of its own
    let code_h = if code.is_empty() { 0 } else { s(10, scale) + s(20, scale) };
    let content_h = (body_top - pad + measure.bottom + code_h).max(icon_d);
    let btn_top = pad + content_h + s(22, scale);
    let client_h = btn_top + s(32, scale) + pad;

    let mut wr = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;


    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let start_x = parent_rc.left + ((parent_rc.right - parent_rc.left) - win_w) / 2;
    let start_y = parent_rc.top + ((parent_rc.bottom - parent_rc.top) - win_h) / 2;

    // The window caption is the app's name. The heading inside names the kind.
    let wcaption = wstr(&crate::version::window_title(title));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), wcaption.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );
    if hwnd.is_null() {
        eprintln!("[!] CreateWindowExW failed: {}", crate::util::last_error("CreateWindowExW"));
        return;
    }

    // The report: code, title, message, the run details, the time and the app version
    let mut lines: Vec<String> = Vec::new();
    if !code.is_empty() {
        lines.push(format!("Error code: {}", code));
    }
    lines.push(format!("Title: {}", title));
    lines.push(format!("Message: {}", text));
    lines.extend(details);
    lines.push(format!("Time: {}", utc_stamp()));
    lines.push(format!("Application version: {}", crate::version::VERSION));
    let wreport = wstr(&lines.join("\r\n"));
    let wtitle = wstr(title);
    let wcode = wstr(&if code.is_empty() { String::new() } else { format!("Error code: {}", code) });

    // Everything the paint and the copy read. These buffers stay alive until the loop below ends.
    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);
    SetPropW(hwnd, wstr("MsgKind").as_ptr(), kind as usize as HANDLE);
    SetPropW(hwnd, wstr("MsgTitle").as_ptr(), wtitle.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgText").as_ptr(), wbody.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgCode").as_ptr(), wcode.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgReport").as_ptr(), wreport.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgBtnTop").as_ptr(), btn_top as usize as HANDLE);
    SetPropW(hwnd, wstr("MsgBodyH").as_ptr(), (measure.bottom.max(0) as usize) as HANDLE);

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    // OK is created first, so it has the focus and Enter closes the popup
    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, "OK", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, client_w - pad - s(90, scale), btn_top, s(90, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);
    if matches!(kind, MsgKind::Error) {
        let b_copy = mk(hwnd, hinst, WC_BUTTON_S, "Copy error code", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, client_w - pad - s(90, scale) - s(10, scale) - s(160, scale), btn_top, s(160, scale), s(32, scale), IDC_MSG_COPY, font_bold);
        subclass_button(b_copy, false);
    }

    // Shown only once it is fully built, like the other dialogs
    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    focus_first_control(hwnd);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        // Ctrl+C copies the report, the same as the copy button
        if msg.message == WM_KEYDOWN && msg.wParam == 0x43 && (GetKeyState(0x11) as u16 & 0x8000) != 0 {
            popup_copy(hwnd);
            continue;
        }
        dispatch_dialog_message(hwnd, &msg);
        if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0 {
            break;
        }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    for key in ["Scale", "MsgKind", "MsgTitle", "MsgText", "MsgCode", "MsgReport", "MsgBtnTop", "MsgBodyH"] {
        RemovePropW(hwnd, wstr(key).as_ptr());
    }
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font_body);
    gdi32::DeleteObject(font_bold);
}

unsafe fn show_confirm_dialog(
    parent: HWND,
    scale: u32,
    title: &str,
    text: &str,
    hint: &str,
    ok_text: &str,
    cancel_text: &str,
) -> bool {
    if LOG_ONLY.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    let hinst = get_module_handle();
    let cls = wstr("AppMsgDlg_520");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(msg_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let font_name = wstr("Segoe UI");
    let font_body = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let pad = s(20, scale);
    let icon_d = s(40, scale);
    let text_x = pad + icon_d + s(16, scale);
    let client_w = s(490, scale);
    let text_w = client_w - text_x - pad;
    let body = if hint.is_empty() { text.to_string() } else { format!("{}\r\n\r\n{}", text, hint) };
    let wbody = wstr(&body);
    let mut measure = RECT { left: 0, top: 0, right: text_w, bottom: 0 };
    let mdc = GetDC(parent);
    let old_mf = gdi32::SelectObject(mdc, font_body);
    gdi32::DrawTextW(mdc, wbody.as_ptr(), -1, &mut measure, DT_LEFT_FLAG | DT_WORDBREAK_FLAG | DT_NOPREFIX_FLAG | DT_CALCRECT_FLAG);
    gdi32::SelectObject(mdc, old_mf);
    ReleaseDC(parent, mdc);
    let body_top = pad + s(28, scale);
    let content_h = (body_top - pad + measure.bottom).max(icon_d);
    let btn_top = pad + content_h + s(22, scale);
    let client_h = btn_top + s(32, scale) + pad;

    let mut wr = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;


    let mut parent_rc = RECT::default();
    if !parent.is_null() {
        GetWindowRect(parent, &mut parent_rc);
    } else {
        parent_rc.right = crate::win32::user32::GetSystemMetrics(0);
        parent_rc.bottom = crate::win32::user32::GetSystemMetrics(1);
    }
    let start_x = parent_rc.left + ((parent_rc.right - parent_rc.left) - win_w) / 2;
    let start_y = parent_rc.top + ((parent_rc.bottom - parent_rc.top) - win_h) / 2;

    let wcaption = wstr(&crate::version::window_title(title));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), wcaption.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );
    if hwnd.is_null() {
        eprintln!("[!] CreateWindowExW failed: {}", crate::util::last_error("CreateWindowExW"));
        return false;
    }

    let wtitle = wstr(title);
    let wcode = wstr("");
    let wreport = wstr("");

    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);
    SetPropW(hwnd, wstr("MsgKind").as_ptr(), MsgKind::Warning as usize as HANDLE);
    SetPropW(hwnd, wstr("MsgTitle").as_ptr(), wtitle.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgText").as_ptr(), wbody.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgCode").as_ptr(), wcode.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgReport").as_ptr(), wreport.as_ptr() as HANDLE);
    SetPropW(hwnd, wstr("MsgBtnTop").as_ptr(), btn_top as usize as HANDLE);
    SetPropW(hwnd, wstr("MsgBodyH").as_ptr(), (measure.bottom.max(0) as usize) as HANDLE);

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let btn_w = s(130, scale);
    let btn_h = s(32, scale);
    let gap = s(10, scale);

    // Cancel button (secondary)
    let b_cancel = mk(hwnd, hinst, WC_BUTTON_S, cancel_text, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, client_w - pad - btn_w, btn_top, btn_w, btn_h, 2, font_bold);
    subclass_button(b_cancel, false);

    // Install / OK button (primary CTA)
    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, ok_text, WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, client_w - pad - btn_w * 2 - gap, btn_top, btn_w, btn_h, 1, font_bold);
    subclass_button(b_ok, true);

    ShowWindow(hwnd, SW_SHOW);
    if !parent.is_null() {
        EnableWindow(parent, 0);
    }
    SetForegroundWindow(hwnd);
    SetFocus(b_ok);

    let mut msg = new_msg();
    let mut result_code = 2; // Default to cancel
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        let userdata = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if userdata != 0 {
            result_code = userdata;
            break;
        }
    }

    if !parent.is_null() {
        EnableWindow(parent, 1);
        SetForegroundWindow(parent);
    }
    for key in ["Scale", "MsgKind", "MsgTitle", "MsgText", "MsgCode", "MsgReport", "MsgBtnTop", "MsgBodyH"] {
        RemovePropW(hwnd, wstr(key).as_ptr());
    }
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font_body);
    gdi32::DeleteObject(font_bold);

    result_code == 1
}


unsafe extern "system" fn msg_dlg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };
            let kind = msg_kind_from(GetPropW(hwnd, wstr("MsgKind").as_ptr()) as usize);
            let title_ptr = GetPropW(hwnd, wstr("MsgTitle").as_ptr()) as *const u16;
            let text_ptr = GetPropW(hwnd, wstr("MsgText").as_ptr()) as *const u16;
            let code_ptr = GetPropW(hwnd, wstr("MsgCode").as_ptr()) as *const u16;

            let mem_dc = gdi32::CreateCompatibleDC(hdc);
            let mem_bm = gdi32::CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
            let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

            let brush = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(mem_dc, &rc, brush);
            let _ = gdi32::DeleteObject(brush);

            let pad = s(20, scale);
            let icon_d = s(40, scale);
            let text_x = pad + icon_d + s(16, scale);
            draw_msg_icon(mem_dc, pad, pad, icon_d, kind);

            let font_name = wstr("Segoe UI");
            let font_body = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let font_small = gdi32::CreateFontW(s(-13, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);

            // Heading, beside the icon
            let of = gdi32::SelectObject(mem_dc, font_bold);
            gdi32::SetTextColor(mem_dc, C_TEXT_BODY);
            let mut rc_t = RECT { left: text_x, top: pad, right: rc.right - pad, bottom: pad + s(22, scale) };
            if !title_ptr.is_null() {
                gdi32::DrawTextW(mem_dc, title_ptr, -1, &mut rc_t, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
            }

            // What happened, and what to do next, wrapped to the text column. Stop above the
            // buttons so a long message can never paint over them.
            gdi32::SelectObject(mem_dc, font_body);
            gdi32::SetTextColor(mem_dc, C_TEXT_BODY);
            let btn_top = GetPropW(hwnd, wstr("MsgBtnTop").as_ptr()) as usize as i32;
            let body_bottom = (btn_top - s(10, scale)).max(pad + s(28, scale));
            let mut rc_x = RECT { left: text_x, top: pad + s(28, scale), right: rc.right - pad, bottom: body_bottom };
            if !text_ptr.is_null() {
                gdi32::DrawTextW(mem_dc, text_ptr, -1, &mut rc_x, DT_LEFT_FLAG | DT_WORDBREAK_FLAG | DT_NOPREFIX_FLAG);
            }

            // Error code, on its own line under the body text
            if !code_ptr.is_null() && *code_ptr != 0 {
                let body_h = GetPropW(hwnd, wstr("MsgBodyH").as_ptr()) as usize as i32;
                gdi32::SelectObject(mem_dc, font_small);
                gdi32::SetTextColor(mem_dc, C_TEXT_MUTED);
                let top = pad + s(28, scale) + body_h + s(10, scale);
                let mut rc_c = RECT { left: text_x, top, right: rc.right - pad, bottom: top + s(20, scale) };
                gdi32::DrawTextW(mem_dc, code_ptr, -1, &mut rc_c, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);
            }

            gdi32::SelectObject(mem_dc, of);
            let _ = gdi32::DeleteObject(font_body);
            let _ = gdi32::DeleteObject(font_bold);
            let _ = gdi32::DeleteObject(font_small);

            gdi32::BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);
            gdi32::SelectObject(mem_dc, old_bm);
            let _ = gdi32::DeleteObject(mem_bm);
            let _ = gdi32::DeleteDC(mem_dc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, dlg_scale(hwnd));
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if id == 1 || id == 2 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, id as LONG_PTR);
            } else if id == IDC_MSG_COPY {
                popup_copy(hwnd);
            }
            0
        }
        WM_CLOSE => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 2);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn show_about_dialog(parent: HWND, scale: u32) {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxAboutDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(about_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let dlg_w = s(BASE_ABOUT_W, scale);
    let dlg_h = s(BASE_ABOUT_H, scale);

    let mut wr = RECT { left: 0, top: 0, right: dlg_w, bottom: dlg_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;

    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let pw = parent_rc.right - parent_rc.left;
    let ph = parent_rc.bottom - parent_rc.top;
    let start_x = parent_rc.left + (pw - win_w) / 2;
    let start_y = parent_rc.top + (ph - win_h) / 2;

    let font_name = wstr("Segoe UI");
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let title = wstr(&crate::version::window_title("About"));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );

    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);


    let b_ok = mk(hwnd, hinst, WC_BUTTON_S, "OK", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW, dlg_w - s(16, scale) - s(90, scale), dlg_h - s(48, scale), s(90, scale), s(32, scale), 1, font_bold);
    subclass_button(b_ok, true);

    // Shown only now: the window is fully set up (dark title bar, controls, scale), so it
    // never appears white or half-built
    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    focus_first_control(hwnd);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0 { break; }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RemovePropW(hwnd, wstr("Scale").as_ptr());
    RemovePropW(hwnd, wstr("AppPtr").as_ptr());
    RemovePropW(hwnd, wstr("Font").as_ptr());
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font_bold);
}

fn about_link_rects(rc: &RECT, scale: u32) -> [(RECT, &'static str, &'static str); 3] {
    [
        (RECT { left: s(24, scale), top: s(156, scale), right: rc.right - s(24, scale), bottom: s(180, scale) }, "Developer: Plisskien", DEV_URL),
        (RECT { left: s(24, scale), top: s(182, scale), right: rc.right - s(24, scale), bottom: s(206, scale) }, "Download binary: hackvshack.net", THREAD_URL),
        (RECT { left: s(24, scale), top: s(208, scale), right: rc.right - s(24, scale), bottom: s(232, scale) }, "Source code: GitHub", GITHUB_URL),
    ]
}

unsafe fn about_link_at(hwnd: HWND, scale: u32, x: i32, y: i32) -> Option<&'static str> {
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    for (r, _label, url) in about_link_rects(&rc, scale) {
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return Some(url);
        }
    }
    None
}
unsafe extern "system" fn about_dlg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let mem_dc = gdi32::CreateCompatibleDC(hdc);
            let mem_bm = gdi32::CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
            let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

            let brush = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(mem_dc, &rc, brush);
            let _ = gdi32::DeleteObject(brush);

            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };

            paint_dialog_card(mem_dc, s(16, scale), s(14, scale), rc.right - s(32, scale), rc.bottom - s(76, scale), "ABOUT", scale);

            let font_name = wstr("Segoe UI");
            let font_title = gdi32::CreateFontW(s(-22, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let font = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

            gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);

            let of = gdi32::SelectObject(mem_dc, font_title);
            gdi32::SetTextColor(mem_dc, C_WHITE_CTA_BG);
            let mut rc_t = RECT { left: s(24, scale), top: s(42, scale), right: rc.right - s(24, scale), bottom: s(70, scale) };
            let wt = wstr("BlackBox Injector");
            gdi32::DrawTextW(mem_dc, wt.as_ptr(), -1, &mut rc_t, DT_CENTER_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            gdi32::SelectObject(mem_dc, font);
            let mut rc_v = RECT { left: s(24, scale), top: s(72, scale), right: rc.right - s(24, scale), bottom: s(92, scale) };
            let found_ver = crate::update::get_found();
            let ver_text = match &found_ver {
                Some(nv) => format!("Version {}  •  Update available: v{}", crate::version::VERSION, nv),
                None => format!("Version {}", crate::version::VERSION),
            };
            if found_ver.is_some() {
                gdi32::SetTextColor(mem_dc, C_WHITE_CTA_BG);
            } else {
                gdi32::SetTextColor(mem_dc, C_TEXT_MUTED);
            }
            let wv = wstr(&ver_text);
            gdi32::DrawTextW(mem_dc, wv.as_ptr(), -1, &mut rc_v, DT_CENTER_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            gdi32::SetTextColor(mem_dc, C_TEXT_BODY);
            let mut rc_about = RECT { left: s(28, scale), top: s(98, scale), right: rc.right - s(28, scale), bottom: s(150, scale) };
            let wa = wstr("Native Windows DLL injector with ordered per-file injection, six methods, x86/x64 targets and stealth options.");
            gdi32::DrawTextW(mem_dc, wa.as_ptr(), -1, &mut rc_about, DT_CENTER_FLAG | 0x00000010 /* DT_WORDBREAK */ | DT_NOPREFIX_FLAG);

            // Clickable project links: bold and underlined
            gdi32::SelectObject(mem_dc, font_bold);
            gdi32::SetTextColor(mem_dc, C_WHITE_CTA_BG);
            for (rect, label, _url) in about_link_rects(&rc, scale) {
                let mut lr = rect;
                let wtl = wstr(label);
                gdi32::DrawTextW(
                    mem_dc,
                    wtl.as_ptr(),
                    -1,
                    &mut lr,
                    DT_CENTER_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x0004 /* DT_UNDERLINE */ | DT_NOPREFIX_FLAG,
                );
            }

            gdi32::SelectObject(mem_dc, of);
            let _ = gdi32::DeleteObject(font_title);
            let _ = gdi32::DeleteObject(font);
            let _ = gdi32::DeleteObject(font_bold);

            gdi32::BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);
            gdi32::SelectObject(mem_dc, old_bm);
            let _ = gdi32::DeleteObject(mem_bm);
            let _ = gdi32::DeleteDC(mem_dc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_SETCURSOR => {
            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            ScreenToClient(hwnd, &mut pt);
            if about_link_at(hwnd, scale, pt.x, pt.y).is_some() {
                SetCursor(LoadCursorW(NULL_HANDLE, 32649usize as *const u16));
                1
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_LBUTTONUP => {
            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            ScreenToClient(hwnd, &mut pt);
            if let Some(url) = about_link_at(hwnd, scale, pt.x, pt.y) {
                let verb = wstr("open");
                let target = wstr(url);
                let _ = crate::win32::shell32::ShellExecuteW(
                    hwnd,
                    verb.as_ptr(),
                    target.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    crate::win32::shell32::SW_SHOWNORMAL,
                );
            }
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, dlg_scale(hwnd));
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if id == 1 || id == 2 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, id as LONG_PTR);
            }
            0
        }
        WM_CLOSE => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 2);
            0
        }
        WM_NCDESTROY => {
            RemovePropW(hwnd, wstr("Scale").as_ptr());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

const BASE_UPDATE_W: i32 = 490;
const BASE_UPDATE_H: i32 = 226;

fn update_link_rects(text_x: i32, client_w: i32, scale: u32) -> [(RECT, &'static str, &'static str); 2] {
    let right = client_w - s(20, scale);
    let pad = s(20, scale);
    [
        (
            RECT { left: text_x, top: pad + s(74, scale), right, bottom: pad + s(96, scale) },
            "Download binary: hackvshack.net",
            THREAD_URL,
        ),
        (
            RECT { left: text_x, top: pad + s(100, scale), right, bottom: pad + s(122, scale) },
            "Source code: GitHub",
            GITHUB_URL,
        ),
    ]
}

unsafe fn update_link_at(hwnd: HWND, scale: u32, x: i32, y: i32) -> Option<&'static str> {
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let pad = s(20, scale);
    let icon_d = s(40, scale);
    let text_x = pad + icon_d + s(16, scale);
    for (r, _label, url) in update_link_rects(text_x, rc.right, scale) {
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
            return Some(url);
        }
    }
    None
}

unsafe extern "system" fn update_dlg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let mem_dc = gdi32::CreateCompatibleDC(hdc);
            let mem_bm = gdi32::CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
            let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

            let brush = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(mem_dc, &rc, brush);
            let _ = gdi32::DeleteObject(brush);

            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };

            let pad = s(20, scale);
            let icon_d = s(40, scale);
            let text_x = pad + icon_d + s(16, scale);

            // Green success circle with checkmark on the left
            draw_msg_icon(mem_dc, pad, pad, icon_d, MsgKind::Success);

            let font_name = wstr("Segoe UI");
            let font_title = gdi32::CreateFontW(s(-18, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let font_sub = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let font_desc = gdi32::CreateFontW(s(-14, scale), 0, 0, 0, gdi32::FW_NORMAL, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());
            let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

            gdi32::SetBkMode(mem_dc, gdi32::TRANSPARENT);

            // Title
            let of = gdi32::SelectObject(mem_dc, font_title);
            gdi32::SetTextColor(mem_dc, C_TEXT_BODY);
            let mut rc_t = RECT { left: text_x, top: pad, right: rc.right - pad, bottom: pad + s(22, scale) };
            let wt = wstr("Update Available");
            gdi32::DrawTextW(mem_dc, wt.as_ptr(), -1, &mut rc_t, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            // Subtitle with version in green
            gdi32::SelectObject(mem_dc, font_sub);
            gdi32::SetTextColor(mem_dc, msg_accent(MsgKind::Success));
            let mut rc_v = RECT { left: text_x, top: pad + s(24, scale), right: rc.right - pad, bottom: pad + s(44, scale) };
            let ver_prop = GetPropW(hwnd, wstr("NewVersion").as_ptr()) as *const u16;
            let ver_str = if !ver_prop.is_null() { from_wsz(ver_prop) } else { String::new() };
            let wv = wstr(&format!("Version {} is available", ver_str));
            gdi32::DrawTextW(mem_dc, wv.as_ptr(), -1, &mut rc_v, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            // Description
            gdi32::SelectObject(mem_dc, font_desc);
            gdi32::SetTextColor(mem_dc, C_TEXT_MUTED);
            let mut rc_desc = RECT { left: text_x, top: pad + s(48, scale), right: rc.right - pad, bottom: pad + s(68, scale) };
            let wd = wstr("Download the latest binary or inspect the source repository:");
            gdi32::DrawTextW(mem_dc, wd.as_ptr(), -1, &mut rc_desc, DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | DT_NOPREFIX_FLAG);

            // Clickable links: bold and underlined
            gdi32::SelectObject(mem_dc, font_bold);
            gdi32::SetTextColor(mem_dc, C_WHITE_CTA_BG);
            for (rect, label, _url) in update_link_rects(text_x, rc.right, scale) {
                let mut lr = rect;
                let wtl = wstr(label);
                gdi32::DrawTextW(
                    mem_dc,
                    wtl.as_ptr(),
                    -1,
                    &mut lr,
                    DT_LEFT_FLAG | DT_VCENTER_FLAG | DT_SINGLELINE_FLAG | 0x0004 /* DT_UNDERLINE */ | DT_NOPREFIX_FLAG,
                );
            }

            gdi32::SelectObject(mem_dc, of);
            let _ = gdi32::DeleteObject(font_title);
            let _ = gdi32::DeleteObject(font_sub);
            let _ = gdi32::DeleteObject(font_desc);
            let _ = gdi32::DeleteObject(font_bold);

            gdi32::BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);
            gdi32::SelectObject(mem_dc, old_bm);
            let _ = gdi32::DeleteObject(mem_bm);
            let _ = gdi32::DeleteDC(mem_dc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_SETCURSOR => {
            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            ScreenToClient(hwnd, &mut pt);
            if update_link_at(hwnd, scale, pt.x, pt.y).is_some() {
                SetCursor(LoadCursorW(NULL_HANDLE, 32649usize as *const u16));
                1
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_LBUTTONUP => {
            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            ScreenToClient(hwnd, &mut pt);
            if let Some(url) = update_link_at(hwnd, scale, pt.x, pt.y) {
                let verb = wstr("open");
                let target = wstr(url);
                let _ = crate::win32::shell32::ShellExecuteW(
                    hwnd,
                    verb.as_ptr(),
                    target.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    crate::win32::shell32::SW_SHOWNORMAL,
                );
            }
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, dlg_scale(hwnd));
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if id == 1 || id == 2 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, id as LONG_PTR);
            }
            0
        }
        WM_CLOSE => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 2);
            0
        }
        WM_NCDESTROY => {
            RemovePropW(hwnd, wstr("Scale").as_ptr());
            RemovePropW(hwnd, wstr("NewVersion").as_ptr());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn show_update_dialog(parent: HWND, scale: u32, new_version: &str) {
    let hinst = get_module_handle();
    let cls = wstr("BlackBoxUpdateDlg");
    let mut wc = WNDCLASSEXW::default();
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(update_dlg_proc);
    wc.hInstance = hinst;
    wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
    wc.lpszClassName = cls.as_ptr();
    wc.hbrBackground = class_bg_brush();
    RegisterClassExW(&wc);

    let dlg_w = s(BASE_UPDATE_W, scale);
    let dlg_h = s(BASE_UPDATE_H, scale);

    let mut wr = RECT { left: 0, top: 0, right: dlg_w, bottom: dlg_h };
    AdjustWindowRect(&mut wr, WS_CAPTION | WS_SYSMENU, 0);
    let win_w = wr.right - wr.left;
    let win_h = wr.bottom - wr.top;

    let mut parent_rc = RECT::default();
    GetWindowRect(parent, &mut parent_rc);
    let pw = parent_rc.right - parent_rc.left;
    let ph = parent_rc.bottom - parent_rc.top;
    let start_x = parent_rc.left + (pw - win_w) / 2;
    let start_y = parent_rc.top + (ph - win_h) / 2;

    let font_name = wstr("Segoe UI");
    let font_bold = gdi32::CreateFontW(s(-15, scale), 0, 0, 0, gdi32::FW_BOLD, 0, 0, 0, gdi32::DEFAULT_CHARSET, 0, 0, gdi32::CLEARTYPE_QUALITY, 0, font_name.as_ptr());

    let title = wstr(&crate::version::window_title("Update Available"));
    let hwnd = CreateWindowExW(
        0, cls.as_ptr(), title.as_ptr(), WS_CAPTION | WS_SYSMENU | 0x02000000 /* WS_CLIPCHILDREN */,
        start_x, start_y, win_w, win_h, parent, NULL_HANDLE, hinst, std::ptr::null_mut()
    );

    let wver = wstr(new_version);
    SetPropW(hwnd, wstr("Scale").as_ptr(), scale as HANDLE);
    SetPropW(hwnd, wstr("NewVersion").as_ptr(), wver.as_ptr() as HANDLE);

    dwmapi::dark_title_bar(hwnd);
    uxtheme::allow_dark_mode_for_window(hwnd);
    apply_app_icons(hwnd);

    let pad = s(20, scale);
    let b_ok = mk(
        hwnd,
        hinst,
        WC_BUTTON_S,
        "OK",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        dlg_w - pad - s(90, scale),
        s(166, scale),
        s(90, scale),
        s(32, scale),
        1,
        font_bold,
    );
    subclass_button(b_ok, true);

    ShowWindow(hwnd, SW_SHOW);
    EnableWindow(parent, 0);
    SetForegroundWindow(hwnd);
    focus_first_control(hwnd);

    let mut msg = new_msg();
    while IsWindow(hwnd) != 0 && GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
        dispatch_dialog_message(hwnd, &msg);
        if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0 { break; }
    }

    EnableWindow(parent, 1);
    SetForegroundWindow(parent);
    RemovePropW(hwnd, wstr("Scale").as_ptr());
    RemovePropW(hwnd, wstr("NewVersion").as_ptr());
    DestroyWindow(hwnd);
    gdi32::DeleteObject(font_bold);
}

unsafe extern "system" fn generic_modal_dlg_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);

            let mem_dc = gdi32::CreateCompatibleDC(hdc);
            let mem_bm = gdi32::CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
            let old_bm = gdi32::SelectObject(mem_dc, mem_bm);

            let brush = gdi32::CreateSolidBrush(C_BG);
            gdi32::FillRect(mem_dc, &rc, brush);
            let _ = gdi32::DeleteObject(brush);

            let title_ptr = GetPropW(hwnd, wstr("CardTitle").as_ptr()) as *const u16;
            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };

            if !title_ptr.is_null() {
                let card_title = from_wsz(title_ptr);
                paint_dialog_card(mem_dc, s(16, scale), s(14, scale), rc.right - s(32, scale), rc.bottom - s(76, scale), &card_title, scale);
            }

            gdi32::BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem_dc, 0, 0, SRCCOPY);
            gdi32::SelectObject(mem_dc, old_bm);
            let _ = gdi32::DeleteObject(mem_bm);
            let _ = gdi32::DeleteDC(mem_dc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_SETCURSOR => {
            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            ScreenToClient(hwnd, &mut pt);
            if about_link_at(hwnd, scale, pt.x, pt.y).is_some() {
                SetCursor(LoadCursorW(NULL_HANDLE, 32649usize as *const u16));
                1
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_LBUTTONUP => {
            let scale = GetPropW(hwnd, wstr("Scale").as_ptr()) as u32;
            let scale = if scale > 0 { scale } else { 100 };
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            ScreenToClient(hwnd, &mut pt);
            if let Some(url) = about_link_at(hwnd, scale, pt.x, pt.y) {
                let verb = wstr("open");
                let target = wstr(url);
                let _ = crate::win32::shell32::ShellExecuteW(
                    hwnd,
                    verb.as_ptr(),
                    target.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    crate::win32::shell32::SW_SHOWNORMAL,
                );
            }
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lp as *const DRAWITEMSTRUCT);
            if dis.CtlType == ODT_BUTTON {
                draw_owner_button(dis, dlg_scale(hwnd));
                return 1;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp as u32) as i32;
            if id == 1 || id == 2 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, id as LONG_PTR);
            }
            0
        }
        WM_CLOSE => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 2);
            0
        }
        WM_NCDESTROY => {
            RemovePropW(hwnd, wstr("CardTitle").as_ptr());
            RemovePropW(hwnd, wstr("Scale").as_ptr());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ===== Entry Point =====
pub fn run() -> i32 {
    unsafe {
        // Must run before any window or DPI query. Without it Windows bitmap-stretches
        // the whole UI on scaled displays, which is what made everything look soft.
        SetProcessDpiAwarenessContext(DPI_AWARENESS_PER_MONITOR_V2);
        let _ = crate::process::enable_debug_privilege();
        let hinst = get_module_handle();
        let cls = wstr("BlackBoxMainWindow");

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.style = 0;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.hCursor = LoadCursorW(NULL_HANDLE, (32512 as usize) as *const u16);
        wc.lpszClassName = cls.as_ptr();
        wc.hbrBackground = class_bg_brush();

        if RegisterClassExW(&wc) == 0 {
            eprintln!("{}", last_error("RegisterClassExW"));
            return 1;
        }

        let mut settings = Settings::load();
        // Emergency/test override for the zoom: BLACKBOX_UI_SCALE=75..150 (0 = auto)
        if let Ok(raw) = std::env::var("BLACKBOX_UI_SCALE") {
            if let Ok(v) = raw.trim().parse::<u32>() {
                settings.ui_scale = if v == 0 { 0 } else { v.clamp(75, 150) };
            }
        }
        // In per-process mode the saved setup for the current target replaces the global one
        let startup_name = settings.process_name.clone();
        if settings.scope == SettingsScope::PerProcess && !startup_name.is_empty() {
            settings.apply_profile(&startup_name);
        }
        // Drop saved entries whose file no longer exists, so the list only shows usable DLLs.
        let modules_before = settings.modules.len();
        settings.modules.retain(|m| std::path::Path::new(&m.path).is_file());
        if settings.modules.len() != modules_before {
            settings.save();
        }
        let active_profile = settings.process_name.clone();
        let scale = resolve_scale(settings.ui_scale);

        let mut rc = RECT {
            left: 0,
            top: 0,
            right: s(BASE_CLIENT_W, scale),
            bottom: s(BASE_CLIENT_H, scale),
        };
        AdjustWindowRect(&mut rc, WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX, 0);
        let win_w = rc.right - rc.left;
        let win_h = rc.bottom - rc.top;

        let screen_w = GetSystemMetrics(0);
        let screen_h = GetSystemMetrics(1);
        let start_x = (screen_w - win_w) / 2;
        let start_y = (screen_h - win_h) / 2;

        let shared = Arc::new(Mutex::new(Shared {
            hwnd: 0,
            target_pid: 0,
            process_name: settings.process_name.clone(),
            delay: settings.delay,
            delay_between: settings.delay_between,
            auto_inject: settings.auto_inject,
            close_on_inject: settings.close_on_inject,
            auto_wait_window: settings.auto_wait_window,
            auto_retry: settings.auto_retry,
            inject_children: settings.inject_children,
            auto_child_filter: settings.auto_child_filter.clone(),
            auto_window_title: settings.auto_window_title.clone(),
            modules: settings.modules.clone(),
            opts: settings.inject_options(),
        }));

        let title = wstr(&crate::version::window_title("Main"));
        let hwnd = CreateWindowExW(
            0,
            cls.as_ptr(),
            title.as_ptr(),
            WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | 0x02000000 /* WS_CLIPCHILDREN */,
            start_x,
            start_y,
            win_w,
            win_h,
            NULL_HANDLE,
            NULL_HANDLE,
            hinst,
            std::ptr::null_mut(),
        );

        if hwnd.is_null() {
            eprintln!("{}", last_error("CreateWindowExW"));
            return 1;
        }

        shared.lock().unwrap().hwnd = hwnd as usize;

        dwmapi::dark_title_bar(hwnd);
        uxtheme::allow_dark_mode_for_window(hwnd);
        apply_app_icons(hwnd);

        let app = Box::new(App {
            shared: Arc::clone(&shared),
            settings,
            hfont: NULL_HANDLE,
            hfont_bold: NULL_HANDLE,
            hfont_small: NULL_HANDLE,
            hfont_italic: NULL_HANDLE,
            hfont_btn: NULL_HANDLE,
            hfont_title: NULL_HANDLE,
            bg_brush: gdi32::CreateSolidBrush(C_BG),
            panel_brush: gdi32::CreateSolidBrush(C_PANEL),
            input_brush: gdi32::CreateSolidBrush(C_INPUT_BG),
            injecting: false,
            proc_title: "Waiting for process...".to_string(),
            proc_sub: "PID: N/A".to_string(),
            proc_icon: None,
            arch_cache: HashMap::new(),
            picker_cache: PickerCache::new(),
            active_profile,
            log: Vec::new(),
            log_scroll: 0,
            log_total: 0,
            log_visible: 0,
            log_wrapped: Vec::new(),
            log_sel_anchor: None,
            log_sel_end: None,
            log_selecting: false,
            log_drag: false,
            log_drag_offset: 0,
        });

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(app) as LONG_PTR);

        // Spawn auto-inject worker thread
        let worker_shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            crate::worker::auto_inject_loop(worker_shared);
        });

        // Create and layout controls
        let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
        LOG_ONLY.store((*app_ptr).settings.log_only, std::sync::atomic::Ordering::Relaxed);
        init_app_fonts(&mut *app_ptr, scale);
        create_main_controls(hwnd, &mut *app_ptr);
        apply_scale(hwnd, &mut *app_ptr);
        update_process_info(hwnd, &mut *app_ptr);
        SetTimer(hwnd, TARGET_REFRESH_TIMER_ID, TARGET_REFRESH_TIMER_MS, None);

        let admin = crate::process::is_elevated();
        push_log(&mut *app_ptr, if admin {
            "[+] Administrator mode: SeDebugPrivilege enabled"
        } else {
            "[!] Unelevated: limited process access"
        });
        push_log(&mut *app_ptr, "[*] Ready. Select target and add DLLs.");

        DragAcceptFiles(hwnd, 1);

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        crate::update::check_async(hwnd);

        let mut msg = new_msg();

        while GetMessageW(&mut msg, NULL_HANDLE, 0, 0) > 0 {
            dispatch_dialog_message(hwnd, &msg);
        }

        msg.wParam as i32
    }
}

pub fn run_gui() -> i32 {
    run()
}


