//! Shelf: files dragged into the notch are saved as paths (not copied),
//! and can later be dragged out to chat, email, or other folders.

use crate::with_app;
use tiny_skia as sk;
use windows::core::{implement, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub struct Item {
    pub path: String,
    pub name: String,
    pub thumb: Option<sk::Pixmap>,
}

impl Item {
    pub fn new(path: String, px: u32) -> Item {
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let thumb = thumbnail(&path, px);
        Item { path, name, thumb }
    }
}

/// Windows Explorer thumbnail: image/PDF preview or application icon.
fn thumbnail(path: &str, px: u32) -> Option<sk::Pixmap> {
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let size = SIZE { cx: px as i32, cy: px as i32 };
        let bmp = factory.GetImage(size, SIIGBF_RESIZETOFIT).ok()?;
        let pixmap = bitmap_to_pixmap(bmp);
        let _ = DeleteObject(bmp);
        pixmap
    }
}

unsafe fn bitmap_to_pixmap(bmp: HBITMAP) -> Option<sk::Pixmap> {
    let mut info = BITMAP::default();
    if GetObjectW(bmp, std::mem::size_of::<BITMAP>() as i32, Some(&mut info as *mut _ as *mut _)) == 0 {
        return None;
    }
    let (w, h) = (info.bmWidth as u32, info.bmHeight.unsigned_abs());
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut data = vec![0u8; (w * h * 4) as usize];
    let dc = CreateCompatibleDC(None);
    let lines = GetDIBits(dc, bmp, 0, h, Some(data.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
    let _ = DeleteDC(dc);
    if lines == 0 {
        return None;
    }

    // Photos usually have no alpha (all 0) = fully opaque. Icons have alpha, sometimes not
    // premultiplied; tiny-skia requires premultiplied alpha.
    let no_alpha = data.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
    let straight = !no_alpha && data.as_chunks::<4>().0.iter().any(|p| p[0].max(p[1]).max(p[2]) > p[3]);
    for p in data.as_chunks_mut::<4>().0 {
        p.swap(0, 2); // BGRA -> RGBA
        if no_alpha {
            p[3] = 255;
        } else if straight {
            let a = p[3] as u16;
            for c in &mut p[..3] {
                *c = (*c as u16 * a / 255) as u8;
            }
        }
    }
    sk::Pixmap::from_vec(data, sk::IntSize::from_wh(w, h)?)
}

pub fn open(path: &str) {
    unsafe {
        ShellExecuteW(None, windows::core::w!("open"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Drags files out of the notch. Blocks until files are dropped (OLE loop).
pub fn drag_out(hwnd: HWND, paths: &[String]) {
    unsafe {
        let pidls: Vec<*mut ITEMIDLIST> =
            paths.iter().map(|p| ILCreateFromPathW(&HSTRING::from(p.as_str()))).filter(|p| !p.is_null()).collect();
        if pidls.is_empty() {
            return;
        }
        let list: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| *p as *const _).collect();
        if let Ok(items) = SHCreateShellItemArrayFromIDLists(&list) {
            if let Ok(data) = items.BindToHandler::<_, IDataObject>(None, &BHID_DataObject) {
                let _ = SHDoDragDrop(hwnd, &data, None, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK);
            }
        }
        for p in pidls {
            ILFree(Some(p));
        }
    }
}

// ---- Receiving files dragged into the notch --------------------------------------------

#[implement(IDropTarget)]
struct DropTarget;

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        data: Option<&IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let has_files = data.is_some_and(|d| unsafe { d.QueryGetData(&hdrop_format()) } == S_OK);
        unsafe { *effect = if has_files { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if has_files {
            with_app(|a| a.on_drag_enter());
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let accepting = with_app(|a| a.drag_in).unwrap_or(false);
        unsafe { *effect = if accepting { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        with_app(|a| a.on_drag_leave());
        Ok(())
    }

    fn Drop(&self, data: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let paths = data.map(dropped_paths).unwrap_or_default();
        unsafe { *effect = if paths.is_empty() { DROPEFFECT_NONE } else { DROPEFFECT_COPY } };
        with_app(|a| a.on_drop(paths));
        Ok(())
    }
}

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

fn dropped_paths(data: &IDataObject) -> Vec<String> {
    let mut paths = Vec::new();
    unsafe {
        let Ok(mut medium) = data.GetData(&hdrop_format()) else { return paths };
        let drop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        for i in 0..count {
            let mut buf = vec![0u16; DragQueryFileW(drop, i, None) as usize + 1];
            let n = DragQueryFileW(drop, i, Some(&mut buf));
            paths.push(String::from_utf16_lossy(&buf[..n as usize]));
        }
        ReleaseStgMedium(&mut medium);
    }
    paths
}

pub fn register(hwnd: HWND) {
    unsafe {
        let _ = OleInitialize(None);
        let target: IDropTarget = DropTarget.into();
        let _ = RegisterDragDrop(hwnd, &target);
    }
}
