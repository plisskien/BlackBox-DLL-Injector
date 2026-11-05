//! user32 declarations (GUI + window picker + drag&drop support bits).
#![allow(non_snake_case, dead_code)]

use super::*;

pub const WM_APP: u32 = 0x8000;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_QUIT: u32 = 0x0012;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_ERASEBKGND: u32 = 0x0014;
pub const WM_SIZE: u32 = 0x0005;
pub const WM_COMMAND: u32 = 0x0111;
pub const WM_NOTIFY: u32 = 0x004E;
pub const WM_CTLCOLORSTATIC: u32 = 0x0138;
pub const WM_CTLCOLOREDIT: u32 = 0x0133;
pub const WM_CTLCOLORLISTBOX: u32 = 0x0134;
pub const WM_CTLCOLORSCROLLBAR: u32 = 0x0137;
pub const WM_DROPFILES: u32 = 0x0233;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_MOUSELEAVE: u32 = 0x02A3;
pub const WM_SETCURSOR: u32 = 0x0020;
pub const WM_CREATE: u32 = 0x0001;
pub const WM_INITDIALOG: u32 = 0x0110;
pub const WM_HSCROLL: u32 = 0x0114;
pub const WM_VSCROLL: u32 = 0x0115;
pub const WM_GETMINMAXINFO: u32 = 0x0024;
pub const WM_DRAWITEM: u32 = 0x002B;
pub const WM_SETICON: u32 = 0x0080;

pub const IMAGE_ICON: u32 = 1;
pub const LR_LOADFROMFILE: u32 = 0x0010;
pub const LR_DEFAULTSIZE: u32 = 0x0040;
pub const PM_REMOVE: u32 = 0x0001;
pub const ASFW_ANY: u32 = 0xFFFF_FFFF;

pub const SB_HORZ: i32 = 0;
pub const SBS_VERT: u32 = 0x0001;
pub const SB_CTL: i32 = 2;
pub const SB_VERT: i32 = 1;
pub const SB_LINEUP: u32 = 0;
pub const SB_LINEDOWN: u32 = 1;
pub const SB_PAGEUP: u32 = 2;
pub const SB_PAGEDOWN: u32 = 3;
pub const SB_THUMBPOSITION: u32 = 4;
pub const SB_THUMBTRACK: u32 = 5;
pub const SB_TOP: u32 = 6;
pub const SB_BOTTOM: u32 = 7;
pub const SIF_RANGE: u32 = 0x0001;
pub const SIF_PAGE: u32 = 0x0002;
pub const SIF_POS: u32 = 0x0004;

// Owner-draw items
pub const ODA_DRAWENTIRE: u32 = 0x0001;
pub const ODS_SELECTED: u32 = 0x0001;
pub const ODS_DISABLED: u32 = 0x0004;
pub const BS_OWNERDRAW: u32 = 0x000B;
pub const ODT_BUTTON: u32 = 4;
pub const ODT_LISTVIEW: u32 = 102;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DRAWITEMSTRUCT {
    pub CtlType: u32,
    pub CtlID: u32,
    pub itemID: u32,
    pub itemAction: u32,
    pub itemState: u32,
    pub hwndItem: HWND,
    pub hDC: HDC,
    pub rcItem: RECT,
    pub itemData: usize,
}
impl Default for DRAWITEMSTRUCT {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

// Owner-draw size request: the owner sets itemHeight for each owner-drawn row
pub const WM_MEASUREITEM: u32 = 0x002C;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MEASUREITEMSTRUCT {
    pub CtlType: u32,
    pub CtlID: u32,
    pub itemID: u32,
    pub itemWidth: u32,
    pub itemHeight: u32,
    pub itemData: usize,
}

// Mouse tracking (hover)
pub const TME_LEAVE: u32 = 0x0002;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TRACKMOUSEEVENT {
    pub cbSize: u32,
    pub dwFlags: u32,
    pub hwndTrack: HWND,
    pub dwHoverTime: u32,
}
impl Default for TRACKMOUSEEVENT {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

pub const WM_KEYDOWN: u32 = 0x0100;

// ComboBox notifications
pub const CBN_DROPDOWN: u32 = 7;
pub const CBN_SELCHANGE: u32 = 1;

// Button/checkbox
pub const BN_CLICKED: u32 = 0;

// Edit
pub const EN_CHANGE: u32 = 0x0300;

pub const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
pub const WS_MINIMIZEBOX: u32 = 0x0002_0000;
pub const WS_VISIBLE: u32 = 0x1000_0000;
pub const WS_CHILD: u32 = 0x4000_0000;
pub const WS_TABSTOP: u32 = 0x0001_0000;
pub const WS_VSCROLL: u32 = 0x0020_0000;
pub const WS_HSCROLL: u32 = 0x0010_0000;
pub const WS_BORDER: u32 = 0x0080_0000;
pub const WS_EX_CLIENTEDGE: i32 = 0x0000_0200;
pub const WS_EX_TOPMOST: i32 = 0x0000_0008;
pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_CAPTION: u32 = 0x00C0_0000;
pub const WS_SYSMENU: u32 = 0x0008_0000;
pub const WS_DISABLED: u32 = 0x0800_0000;
pub const WS_GROUP: u32 = 0x0002_0000;

pub const BS_PUSHBUTTON: u32 = 0x0000_0000;
pub const BS_DEFPUSHBUTTON: u32 = 0x0000_0001;
pub const BS_CHECKBOX: u32 = 0x0000_0002;
pub const BS_AUTOCHECKBOX: u32 = 0x0000_0003;
pub const BS_GROUPBOX: u32 = 0x0000_0007;

pub const CBS_DROPDOWNLIST: u32 = 0x0003;
pub const CBS_SORT: u32 = 0x0100;
pub const CBS_HASSTRINGS: u32 = 0x0200;
pub const CBS_NOINTEGRALHEIGHT: u32 = 0x0400;

pub const ES_MULTILINE: u32 = 0x0004;
pub const ES_READONLY: u32 = 0x0800;
pub const ES_AUTOVSCROLL: u32 = 0x0040;
pub const ES_NUMBER: u32 = 0x2000;
pub const ES_LEFT: u32 = 0x0000;

pub const SS_LEFT: u32 = 0x0000;
pub const SS_NOTIFY: u32 = 0x0100;
pub const SS_WHITEFRAME: u32 = 0x0005;

pub const LVS_REPORT: u32 = 0x0001;
pub const LVS_SINGLESEL: u32 = 0x0004;
pub const LVS_SHOWSELALWAYS: u32 = 0x0008;
pub const LVS_NOCOLUMNHEADER: u32 = 0x4000;
pub const LVS_OWNERDRAWFIXED: u32 = 0x0400;
pub const LVS_EX_CHECKBOXES: u32 = 0x0000_0004;
pub const LVS_EX_FULLROWSELECT: u32 = 0x0000_0020;
pub const LVS_EX_DOUBLEBUFFER: u32 = 0x0001_0000;

pub const LVM_FIRST: u32 = 0x1000;
pub const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 97;
pub const LVM_INSERTITEMW: u32 = LVM_FIRST + 77;
pub const LVM_SETITEMW: u32 = LVM_FIRST + 76;
pub const LVM_GETITEMW: u32 = LVM_FIRST + 75;
pub const LVM_DELETEITEM: u32 = LVM_FIRST + 8;
pub const LVM_DELETEALLITEMS: u32 = LVM_FIRST + 9;
pub const LVM_GETITEMCOUNT: u32 = LVM_FIRST + 4;
pub const LVM_GETITEMTEXTW: u32 = LVM_FIRST + 115;
pub const LVM_GETNEXTITEM: u32 = LVM_FIRST + 12;
pub const LVM_GETITEMSTATE: u32 = LVM_FIRST + 44;
pub const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = LVM_FIRST + 54;
pub const LVM_SETITEMSTATE: u32 = LVM_FIRST + 45;
pub const LVM_GETCOLUMNW: u32 = LVM_FIRST + 95;
pub const LVM_SETCOLUMNW: u32 = LVM_FIRST + 96;

pub const LVCF_TEXT: u32 = 0x0004;
pub const LVCF_WIDTH: u32 = 0x0002;
pub const LVCF_FMT: u32 = 0x0001;
pub const LVCF_SUBITEM: u32 = 0x0008;

pub const LVIF_TEXT: u32 = 0x0001;
pub const LVIF_PARAM: u32 = 0x0004;
pub const LVIS_SELECTED: u32 = 0x0002;
pub const LVIS_STATEIMAGEMASK: u32 = 0xF000;
pub const LVNI_SELECTED: u32 = 0x0002;
pub const LVNI_ALL: u32 = 0x0000;

pub const SW_SHOW: i32 = 5;
pub const SW_HIDE: i32 = 0;

pub const IDC_CROSS: *const u16 = 32515isize as *const u16;
pub const IDC_ARROW: *const u16 = 32512isize as *const u16;
pub const IDC_HAND: *const u16 = 32649isize as *const u16;

pub const NULL_BRUSH: i32 = 5;
pub const DEFAULT_GUI_FONT: i32 = 17;

pub const GA_ROOT: u32 = 2;

pub const SM_CXSCREEN: u32 = 0;
pub const SM_CYSCREEN: u32 = 1;

pub const COLOR_WINDOW: i32 = 5;
pub const COLOR_BTNFACE: i32 = 15;

pub const DRAGQUERY_NUMFILES: u32 = 0xFFFF_FFFF;

pub const GWL_STYLE: i32 = -16;

pub const BM_GETCHECK: u32 = 0x00F0;
pub const BM_SETCHECK: u32 = 0x00F1;
pub const BST_CHECKED: LONG = 1;
pub const BST_UNCHECKED: LONG = 0;

pub const CB_ADDSTRING: u32 = 0x0143;
pub const CB_RESETCONTENT: u32 = 0x014B;
pub const CB_GETCURSEL: u32 = 0x0147;
pub const CB_SETCURSEL: u32 = 0x014E;
pub const CB_GETLBTEXT: u32 = 0x0148;
pub const CB_GETLBTEXTLEN: u32 = 0x0149;
pub const CB_SETITEMDATA: u32 = 0x0151;
pub const CB_GETITEMDATA: u32 = 0x0150;
pub const CB_SETCUEBANNER: u32 = 0x0300 + 11;

pub const WM_SETFONT: u32 = 0x0030;
pub const WM_GETFONT: u32 = 0x0031;

pub const EM_SETLIMITTEXT: u32 = 0x00C5;
pub const EM_REPLACESEL: u32 = 0x00C2;
pub const EM_SETSEL: u32 = 0x00B1;

pub const UDM_SETRANGE: u32 = 0x0464;
pub const UDM_GETPOS32: u32 = 0x0472;
pub const UDM_SETPOS32: u32 = 0x0471;
pub const UDM_SETBUDDY: u32 = 0x0469;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct POINT {
    pub x: LONG,
    pub y: LONG,
}
impl Default for POINT {
    fn default() -> Self {
        POINT { x: 0, y: 0 }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
}
impl Default for MSG {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RECT {
    pub left: LONG,
    pub top: LONG,
    pub right: LONG,
    pub bottom: LONG,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PAINTSTRUCT {
    pub hdc: HDC,
    pub fErase: BOOL,
    pub rcPaint: RECT,
    pub _rest: [u8; 64],
}
impl Default for PAINTSTRUCT {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MINMAXINFO {
    pub ptReserved: POINT,
    pub ptMaxSize: POINT,
    pub ptMaxPosition: POINT,
    pub ptMinTrackSize: POINT,
    pub ptMaxTrackSize: POINT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HICON,
}
impl Default for WNDCLASSEXW {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

pub type WNDPROC = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NMCUSTOMDRAW {
    pub hdr: NMHDR,
    pub dwDrawStage: u32,
    pub hdc: HDC,
    pub rc: RECT,
    pub dwItemSpec: usize,
    pub uItemState: u32,
    pub lItemlParam: LPARAM,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NMHDR {
    pub hwndFrom: HWND,
    pub idFrom: usize,
    pub code: u32,
}
impl Default for NMHDR {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

pub const NM_CUSTOMDRAW: u32 = 0xFFFF_FF04u32;
pub const CDDS_PREPAINT: u32 = 0x0000_0001;
pub const CDDS_ITEMPREPAINT: u32 = 0x0001_0001;
pub const CDRF_DODEFAULT: LRESULT = 0;
pub const CDRF_NOTIFYITEMDRAW: LRESULT = 0x20;

pub const NM_CLICK: u32 = 0xFFFF_FFFE;
pub const NM_DBLCLK: u32 = 0xFFFF_FFFD;
pub const NM_RCLICK: u32 = 0xFFFF_FFFB;
pub const LVN_KEYDOWN: u32 = 0xFFFF_FECA;
pub const LVN_COLUMNCLICK: u32 = 0xFFFF_FF94;
pub const VK_DELETE: u32 = 0x2E;

pub const TPM_LEFTALIGN: u32 = 0x0000;
pub const TPM_TOPALIGN: u32 = 0x0000;
pub const TPM_RETURNCMD: u32 = 0x0100;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;

pub const MF_STRING: u32 = 0x0000;
pub const MF_CHECKED: u32 = 0x0008;
pub const MF_UNCHECKED: u32 = 0x0000;
pub const MF_SEPARATOR: u32 = 0x0800;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NMITEMACTIVATE {
    pub hdr: NMHDR,
    pub iItem: i32,
    pub iSubItem: i32,
    pub uNewState: u32,
    pub uOldState: u32,
    pub uChanged: u32,
    pub ptAction: POINT,
    pub lParam: LPARAM,
    pub uKeyFlags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NMLISTVIEW {
    pub hdr: NMHDR,
    pub iItem: i32,
    pub iSubItem: i32,
    pub uNewState: u32,
    pub uOldState: u32,
    pub uChanged: u32,
    pub ptAction: POINT,
    pub lParam: LPARAM,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LVCOLUMNW {
    pub mask: u32,
    pub fmt: i32,
    pub cx: i32,
    pub pszText: *mut u16,
    pub cchTextMax: i32,
    pub iSubItem: i32,
    pub _rest: [u8; 24],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LVITEMW {
    pub mask: u32,
    pub iItem: i32,
    pub iSubItem: i32,
    pub state: u32,
    pub stateMask: u32,
    pub pszText: *mut u16,
    pub cchTextMax: i32,
    pub iImage: i32,
    pub lParam: LPARAM,
    pub _rest: [u8; 24],
}
impl Default for LVITEMW {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TRACKEVENT {} // placeholder (unused)

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    pub fn TrackMouseEvent(tme: *mut TRACKMOUSEEVENT) -> BOOL;
    pub fn GetClassNameW(hwnd: HWND, name: *mut u16, max: i32) -> i32;
    pub fn AdjustWindowRect(rect: *mut RECT, style: u32, menu: BOOL) -> BOOL;
    pub fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        x: i32,
        y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: HWND,
        hMenu: HMENU,
        hInstance: HINSTANCE,
        lpParam: *mut c_void_raw,
    ) -> HWND;
    pub fn DefWindowProcW(hwnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn ShowWindow(hwnd: HWND, nCmdShow: i32) -> BOOL;
    pub fn GetMessageW(lpMsg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    pub fn PeekMessageW(lpMsg: *mut MSG, hwnd: HWND, wMsgFilterMin: u32, wMsgFilterMax: u32, wRemoveMsg: u32) -> BOOL;
    pub fn AllowSetForegroundWindow(dwProcessId: u32) -> BOOL;
    pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> LRESULT;
    pub fn PostMessageW(hwnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM) -> BOOL;
    pub fn SendMessageW(hwnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn SendDlgItemMessageW(hwnd: HWND, id: i32, msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn DestroyWindow(hwnd: HWND) -> BOOL;
    pub fn PostQuitMessage(nExitCode: i32);
    pub fn SetWindowTextW(hwnd: HWND, text: *const u16) -> BOOL;
    pub fn GetWindowTextW(hwnd: HWND, text: *mut u16, max: i32) -> i32;
    pub fn GetWindowTextLengthW(hwnd: HWND) -> i32;
    pub fn GetWindowThreadProcessId(hwnd: HWND, pid: *mut u32) -> u32;
    pub fn WindowFromPoint(pt: POINT) -> HWND;
    pub fn GetAncestor(hwnd: HWND, flags: u32) -> HWND;
    pub fn IsWindow(hwnd: HWND) -> BOOL;
    pub fn IsWindowVisible(hwnd: HWND) -> BOOL;
    pub fn EnumWindows(cb: Option<unsafe extern "system" fn(HWND, LPARAM) -> BOOL>, lparam: LPARAM) -> BOOL;
    pub fn GetWindowRect(hwnd: HWND, rect: *mut RECT) -> BOOL;
    pub fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> BOOL;
    pub fn ScreenToClient(hwnd: HWND, pt: *mut POINT) -> BOOL;
    pub fn ClientToScreen(hwnd: HWND, pt: *mut POINT) -> BOOL;
    pub fn BeginPaint(hwnd: HWND, ps: *mut PAINTSTRUCT) -> HDC;
    pub fn EndPaint(hwnd: HWND, ps: *const PAINTSTRUCT) -> BOOL;
    pub fn GetDC(hwnd: HWND) -> HDC;
    pub fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
    pub fn SetCapture(hwnd: HWND) -> HWND;
    pub fn ReleaseCapture() -> BOOL;
    pub fn GetCursorPos(pt: *mut POINT) -> BOOL;
    pub fn SetCursor(cursor: HCURSOR) -> HCURSOR;
    pub fn LoadCursorW(hInstance: HINSTANCE, lpCursorName: *const u16) -> HCURSOR;
    pub fn LoadIconW(hInstance: HINSTANCE, lpIconName: *const u16) -> HICON;
    pub fn LoadImageW(
        hInstance: HINSTANCE,
        name: *const u16,
        kind: u32,
        cx: i32,
        cy: i32,
        load: u32,
    ) -> HANDLE;
    pub fn SetScrollInfo(hwnd: HWND, bar: i32, si: *const SCROLLINFO, redraw: BOOL) -> i32;
    pub fn GetScrollInfo(hwnd: HWND, bar: i32, si: *mut SCROLLINFO) -> BOOL;
    pub fn ShowScrollBar(hwnd: HWND, bar: i32, show: BOOL) -> BOOL;
    pub fn MessageBoxW(hwnd: HWND, text: *const u16, caption: *const u16, kind: u32) -> i32;
    pub fn SetWindowPos(
        hwnd: HWND,
        after: HWND,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        flags: u32,
    ) -> BOOL;
    pub fn MoveWindow(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, repaint: BOOL) -> BOOL;
    pub fn EnableWindow(hwnd: HWND, enable: BOOL) -> BOOL;
    pub fn IsWindowEnabled(hwnd: HWND) -> BOOL;
    pub fn GetSysColor(index: i32) -> COLORREF;
    pub fn InvalidateRect(hwnd: HWND, rect: *const RECT, erase: BOOL) -> BOOL;
    pub fn UpdateWindow(hwnd: HWND) -> BOOL;
    pub fn GetDlgItem(hwnd: HWND, id: i32) -> HWND;
    pub fn SetFocus(hwnd: HWND) -> HWND;
    pub fn GetFocus() -> HWND;
    pub fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> LONG_PTR;
    pub fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: LONG_PTR) -> LONG_PTR;
    pub fn GetParent(hwnd: HWND) -> HWND;
    pub fn DragAcceptFiles(hwnd: HWND, accept: BOOL);
    pub fn GetMessagePos() -> u32;
    pub fn ValidateRect(hwnd: HWND, rect: *const RECT) -> BOOL;
    pub fn GetForegroundWindow() -> HWND;
    pub fn SetForegroundWindow(hwnd: HWND) -> BOOL;
    pub fn SetActiveWindow(hwnd: HWND) -> HWND;
    pub fn BringWindowToTop(hwnd: HWND) -> BOOL;
    pub fn IsIconic(hwnd: HWND) -> BOOL;
    pub fn AttachThreadInput(idAttach: u32, idAttachTo: u32, attach: BOOL) -> BOOL;
    pub fn CallWindowProcW(
        lpPrevWndFunc: isize,
        hWnd: HWND,
        Msg: u32,
        wParam: WPARAM,
        lParam: LPARAM,
    ) -> LRESULT;
    pub fn GetDlgCtrlID(hWnd: HWND) -> i32;
    pub fn GetSystemMetrics(nIndex: i32) -> i32;
    pub fn DialogBoxIndirectParamW(
        hInstance: HINSTANCE,
        hTemplate: *const c_void_raw,
        parent: HWND,
        proc_: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
        init: LPARAM,
    ) -> i32;
    pub fn EndDialog(hwnd: HWND, result: i32) -> BOOL;
    pub fn CheckDlgButton(hwnd: HWND, id: i32, check: u32) -> BOOL;
    pub fn IsDlgButtonChecked(hwnd: HWND, id: i32) -> u32;
    pub fn SetDlgItemTextW(hwnd: HWND, id: i32, text: *const u16) -> BOOL;
    pub fn GetDlgItemTextW(hwnd: HWND, id: i32, text: *mut u16, max: i32) -> u32;
    pub fn GetDlgItemInt(hwnd: HWND, id: i32, translated: *mut BOOL, signed_: BOOL) -> u32;
    pub fn SetDlgItemInt(hwnd: HWND, id: i32, value: u32, signed_: BOOL) -> BOOL;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn DestroyMenu(hMenu: HMENU) -> BOOL;
    pub fn AppendMenuW(hMenu: HMENU, uFlags: u32, uIDNewItem: usize, lpNewItem: *const u16) -> BOOL;
    pub fn TrackPopupMenu(hMenu: HMENU, uFlags: u32, x: i32, y: i32, nReserved: i32, hWnd: HWND, prcRect: *const RECT) -> BOOL;
    pub fn RedrawWindow(hwnd: HWND, lprcUpdate: *const RECT, hrgnUpdate: HANDLE, flags: u32) -> BOOL;
}

pub const MB_OK: u32 = 0x0;
pub const MB_OKCANCEL: u32 = 0x1;
pub const MB_YESNO: u32 = 0x4;
pub const MB_ICONWARNING: u32 = 0x30;
pub const MB_ICONERROR: u32 = 0x10;
pub const MB_ICONINFORMATION: u32 = 0x40;
pub const MB_ICONQUESTION: u32 = 0x20;
pub const MB_YES: i32 = 6;
pub const MB_OK_RET: i32 = 1;

pub const SWP_NOMOVE: u32 = 0x0002;
pub const SWP_NOSIZE: u32 = 0x0001;
pub const SWP_NOZORDER: u32 = 0x0004;
pub const HWND_TOP: HWND = 0isize as HWND;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CREATESTRUCTW {
    _opaque: [u8; 96],
}

pub const GWL_EXSTYLE: i32 = -20;
pub const GWLP_WNDPROC: i32 = -4;
pub const GWL_WNDPROC: i32 = -4;
pub const GWLP_USERDATA: i32 = -21;
pub const WM_NCDESTROY: u32 = 0x0082;
pub const LVM_SETBKCOLOR: u32 = 0x1001;
pub const LVM_SETTEXTCOLOR: u32 = 0x1024;
pub const LVM_SETTEXTBKCOLOR: u32 = 0x1026;
pub const CB_GETCOUNT: u32 = 0x0146;
pub const EM_SCROLLCARET: u32 = 0x00B7;
