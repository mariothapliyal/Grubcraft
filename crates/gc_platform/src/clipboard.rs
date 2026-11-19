//! The system clipboard, for pasting a join code into the game and copying
//! one out of it. Text only.
//!
//! The Mac uses `pbpaste` and `pbcopy`, which every Mac has. Windows calls
//! the clipboard functions in `user32` directly. Linux tries `wl-paste` and
//! `wl-copy` (Wayland), then `xclip` and `xsel` (X11); with none of them
//! installed, the clipboard just reads as empty.

/// The clipboard's text, or `None` when it holds none (or can't be read).
pub fn get() -> Option<String> {
    imp::get().filter(|t| !t.is_empty())
}

/// Put `text` on the clipboard. False when that failed.
pub fn set(text: &str) -> bool {
    imp::set(text)
}

#[cfg(not(target_os = "windows"))]
mod tool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// What `program args..` prints, when it runs and succeeds.
    pub fn read(program: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Run `program args..` with `text` on its input. True when it ran and
    /// succeeded.
    pub fn write(program: &str, args: &[&str], text: &str) -> bool {
        let Ok(mut child) = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let wrote = child
            .stdin
            .take()
            .is_some_and(|mut input| input.write_all(text.as_bytes()).is_ok());
        // The input closes above, so the tool can finish.
        child.wait().is_ok_and(|s| s.success()) && wrote
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::tool;

    pub fn get() -> Option<String> {
        tool::read("pbpaste", &[])
    }

    pub fn set(text: &str) -> bool {
        tool::write("pbcopy", &[], text)
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::ffi::c_void;

    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(owner: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> isize;
        fn SetClipboardData(format: u32, memory: isize) -> isize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalFree(memory: isize) -> isize;
        fn GlobalLock(memory: isize) -> *mut c_void;
        fn GlobalUnlock(memory: isize) -> i32;
        fn GlobalSize(memory: isize) -> usize;
    }

    /// The clipboard, open until this is dropped.
    struct Open;

    impl Open {
        fn new() -> Option<Open> {
            // SAFETY: no window owns it; closed again on drop.
            (unsafe { OpenClipboard(0) } != 0).then_some(Open)
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: opened in `Open::new`.
            unsafe { CloseClipboard() };
        }
    }

    pub fn get() -> Option<String> {
        let _open = Open::new()?;
        // SAFETY: the clipboard is open. The handle belongs to the
        // clipboard; it is only locked, read within its size, and unlocked.
        unsafe {
            let memory = GetClipboardData(CF_UNICODETEXT);
            if memory == 0 {
                return None;
            }
            let units = GlobalSize(memory) / 2;
            let start = GlobalLock(memory) as *const u16;
            if start.is_null() {
                return None;
            }
            let all = std::slice::from_raw_parts(start, units);
            let end = all.iter().position(|&u| u == 0).unwrap_or(units);
            let text = String::from_utf16_lossy(&all[..end]);
            GlobalUnlock(memory);
            Some(text)
        }
    }

    pub fn set(text: &str) -> bool {
        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let bytes = wide.len() * 2;
        // SAFETY: the block is allocated with room for `wide`, filled while
        // locked, then handed to the clipboard, which owns it from then on;
        // it is freed here only when the clipboard didn't take it.
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if memory == 0 {
                return false;
            }
            let start = GlobalLock(memory) as *mut u16;
            if start.is_null() {
                GlobalFree(memory);
                return false;
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), start, wide.len());
            GlobalUnlock(memory);
            let Some(_open) = Open::new() else {
                GlobalFree(memory);
                return false;
            };
            EmptyClipboard();
            if SetClipboardData(CF_UNICODETEXT, memory) == 0 {
                GlobalFree(memory);
                return false;
            }
            true
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::tool;

    pub fn get() -> Option<String> {
        tool::read("wl-paste", &["--no-newline"])
            .or_else(|| tool::read("xclip", &["-selection", "clipboard", "-o"]))
            .or_else(|| tool::read("xsel", &["--clipboard", "--output"]))
    }

    pub fn set(text: &str) -> bool {
        tool::write("wl-copy", &[], text)
            || tool::write("xclip", &["-selection", "clipboard"], text)
            || tool::write("xsel", &["--clipboard", "--input"], text)
    }
}
