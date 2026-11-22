//! One picture painted on a window with GDI, before there is a graphics device to draw it with.
//! Native pointers never escape these calls, and every handle made here is released here.

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_pels_per_meter: i32,
    y_pels_per_meter: i32,
    clr_used: u32,
    clr_important: u32,
}

type Handle = *mut core::ffi::c_void;

/// `BI_RGB`, `DIB_RGB_COLORS`, `SRCCOPY` and `HALFTONE`.
const BI_RGB: u32 = 0;
const DIB_RGB_COLORS: u32 = 0;
const SRCCOPY: u32 = 0x00CC_0020;
const HALFTONE: i32 = 4;

#[link(name = "user32")]
extern "system" {
    fn GetClientRect(window: Handle, rect: *mut Rect) -> i32;
    fn GetDC(window: Handle) -> Handle;
    fn ReleaseDC(window: Handle, dc: Handle) -> i32;
    fn FillRect(dc: Handle, rect: *const Rect, brush: Handle) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(colour: u32) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
    fn SetStretchBltMode(dc: Handle, mode: i32) -> i32;
    fn SetBrushOrgEx(dc: Handle, x: i32, y: i32, old: *mut core::ffi::c_void) -> i32;
    #[allow(clippy::too_many_arguments)]
    fn StretchDIBits(
        dc: Handle,
        x_dest: i32,
        y_dest: i32,
        dest_width: i32,
        dest_height: i32,
        x_src: i32,
        y_src: i32,
        src_width: i32,
        src_height: i32,
        bits: *const core::ffi::c_void,
        info: *const BitmapInfoHeader,
        usage: u32,
        rop: u32,
    ) -> i32;
}

fn client_size(window: isize) -> Option<(u32, u32)> {
    if window == 0 {
        return None;
    }
    let mut r = Rect::default();
    // SAFETY: the system validates the window handle; `r` is a writable rectangle of the native
    // layout and is not retained.
    if unsafe { GetClientRect(window as Handle, &mut r) } == 0 {
        return None;
    }
    Some((
        u32::try_from(r.right - r.left).ok()?,
        u32::try_from(r.bottom - r.top).ok()?,
    ))
}

pub fn paint(window: isize, picture: &crate::Picture<'_>, round: (u8, u8, u8)) -> bool {
    let Some((cw, ch)) = client_size(window) else {
        return false;
    };
    let at = crate::contained((picture.width, picture.height), (cw, ch));
    let (Ok(w), Ok(h), Ok(cw), Ok(ch)) = (
        i32::try_from(picture.width),
        i32::try_from(picture.height),
        i32::try_from(cw),
        i32::try_from(ch),
    ) else {
        return false;
    };
    let rows = usize::try_from(picture.width).unwrap_or(0) * 4;
    if w == 0 || h == 0 || picture.bgra.len() < rows * usize::try_from(h).unwrap_or(0) {
        return false;
    }
    let info = BitmapInfoHeader {
        size: 40,
        width: w,
        // Negative: the rows run top to bottom, as the picture's do.
        height: -h,
        planes: 1,
        bit_count: 32,
        compression: BI_RGB,
        size_image: 0,
        x_pels_per_meter: 0,
        y_pels_per_meter: 0,
        clr_used: 0,
        clr_important: 0,
    };
    let colour = u32::from(round.0) | u32::from(round.1) << 8 | u32::from(round.2) << 16;
    let hwnd = window as Handle;
    // SAFETY: the system validates the window handle. The device context and the brush are made
    // and released within this block; `info` and the picture's rows are read for the length of
    // the copy only, the rows checked above to hold `height` rows of `width` four-byte texels.
    unsafe {
        let dc = GetDC(hwnd);
        if dc.is_null() {
            return false;
        }
        let brush = CreateSolidBrush(colour);
        if !brush.is_null() {
            let all = Rect {
                left: 0,
                top: 0,
                right: cw,
                bottom: ch,
            };
            FillRect(dc, &all, brush);
            DeleteObject(brush);
        }
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, core::ptr::null_mut());
        let copied = StretchDIBits(
            dc,
            at.0,
            at.1,
            at.2,
            at.3,
            0,
            0,
            w,
            h,
            picture.bgra.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        ReleaseDC(hwnd, dc);
        copied != 0
    }
}
