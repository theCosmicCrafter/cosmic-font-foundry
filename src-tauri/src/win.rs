//! Windows OS integration: GDI session font loading/unloading, WM_FONTCHANGE broadcast,
//! MoveFileExW reboot scheduling, UAC self-elevation, TrustedInstaller ownership checks,
//! and native Win32 RegisterHotKey global shortcut listener.

use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    Graphics::Gdi::{AddFontResourceExW, RemoveFontResourceExW, FR_PRIVATE},
    Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT},
    System::Threading::{GetCurrentThreadId, GetExitCodeProcess, WaitForSingleObject, INFINITE},
    UI::{
        Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
        },
        Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
        WindowsAndMessaging::{
            GetMessageW, PostThreadMessageW, SendMessageTimeoutW, HWND_BROADCAST, MSG,
            SMTO_ABORTIFHUNG, SW_HIDE, WM_FONTCHANGE, WM_HOTKEY, WM_QUIT,
        },
    },
};

/// Resolves a Windows `FOLDERID_*` path via `SHGetKnownFolderPath`.
pub fn known_folder(id: &windows_sys::core::GUID) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{System::Com::CoTaskMemFree, UI::Shell::SHGetKnownFolderPath};
    let mut ptr: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath allocates the string; we copy it and always free with CoTaskMemFree.
    unsafe {
        let hr = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut ptr);
        let path = if hr == 0 && !ptr.is_null() {
            let mut n = 0;
            while *ptr.add(n) != 0 {
                n += 1;
            }
            Some(std::ffi::OsString::from_wide(std::slice::from_raw_parts(ptr, n)).into())
        } else {
            None
        };
        CoTaskMemFree(ptr as *const _);
        path
    }
}

/// SID of `NT SERVICE\TrustedInstaller`, owner of inbox Windows system fonts.
const SID_TRUSTED_INSTALLER: &str =
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";

/// Returns whether a font file is owned by `TrustedInstaller` (memoized per session).
pub fn is_trusted_installer(path: &Path) -> Option<bool> {
    use std::{collections::HashMap, sync::Mutex};
    use windows_sys::Win32::{
        Foundation::{LocalFree, ERROR_SUCCESS},
        Security::{
            Authorization::{ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT},
            OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        },
    };
    static SEEN: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
    let key = path.to_string_lossy().to_lowercase();
    if let Some(&v) = SEEN.lock().ok()?.get_or_insert_with(HashMap::new).get(&key) {
        return Some(v);
    }
    let wide_path = wide(path);
    let mut owner = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: wide_path is NUL-terminated; Windows allocates sd which is freed via LocalFree.
    let result = unsafe {
        let r = GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sd,
        );
        if r != ERROR_SUCCESS || owner.is_null() {
            if !sd.is_null() {
                LocalFree(sd as _);
            }
            return None;
        }
        let mut text: *mut u16 = std::ptr::null_mut();
        let ok = ConvertSidToStringSidW(owner, &mut text) != 0 && !text.is_null();
        let is_ti = ok && {
            let mut n = 0;
            while *text.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(text, n)) == SID_TRUSTED_INSTALLER
        };
        if !text.is_null() {
            LocalFree(text as _);
        }
        LocalFree(sd as _);
        is_ti
    };
    if let Ok(mut map) = SEEN.lock() {
        map.get_or_insert_with(HashMap::new).insert(key, result);
    }
    Some(result)
}

/// Opens a web URL in the default browser via `ShellExecuteW`.
pub fn open_url(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let (verb, target) = (wide("open"), wide(url));
    // SAFETY: NUL-terminated buffers live across the call; no parent window handle.
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if r as isize > 32 {
        Ok(())
    } else {
        Err(format!("unable to open {url} ({})", r as isize))
    }
}

pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Loads a font into the current Windows session (`private` restricts visibility to this process).
pub fn load_font(path: &Path, private: bool) -> bool {
    let w = wide(path);
    unsafe {
        AddFontResourceExW(
            w.as_ptr(),
            if private { FR_PRIVATE } else { 0 },
            std::ptr::null(),
        ) > 0
    }
}

/// Unloads a font from the Windows session, draining Windows' internal load reference count.
pub fn unload_font(path: &Path, private: bool) {
    let w = wide(path);
    for _ in 0..16 {
        if unsafe {
            RemoveFontResourceExW(
                w.as_ptr(),
                if private { FR_PRIVATE } else { 0 },
                std::ptr::null(),
            )
        } == 0
        {
            break;
        }
    }
}

/// Broadcasts `WM_FONTCHANGE` to top-level windows with a 1s timeout per window so hung apps cannot stall the UI.
pub fn broadcast_font_change() {
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_FONTCHANGE,
            0,
            0,
            SMTO_ABORTIFHUNG,
            1000,
            std::ptr::null_mut(),
        );
    }
}

/// Schedules a file delete/move on next reboot when Windows has a font locked in use.
pub fn move_on_reboot(src: &Path, dst: Option<&Path>) -> bool {
    let s = wide(src);
    let d = dst.map(wide);
    unsafe {
        MoveFileExW(
            s.as_ptr(),
            d.as_ref().map_or(std::ptr::null(), |x| x.as_ptr()),
            MOVEFILE_DELAY_UNTIL_REBOOT,
        ) != 0
    }
}

/// Relaunches this executable elevated via UAC and waits for its exit code (`None` if cancelled).
pub fn run_elevated(args: &str) -> Option<u32> {
    let exe = std::env::current_exe().ok()?;
    let verb = wide("runas");
    let file = wide(&exe);
    let params = wide(args);
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE;
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return None;
        }
        let mut code = 1u32;
        if WaitForSingleObject(info.hProcess, INFINITE) == WAIT_OBJECT_0 {
            GetExitCodeProcess(info.hProcess, &mut code);
        }
        CloseHandle(info.hProcess);
        Some(code)
    }
}

// ---- Native Win32 Global Hotkey Listener (Quick-Switcher) ----

static HOTKEY_THREAD_ID: AtomicU32 = AtomicU32::new(0);

pub fn parse_shortcut(spec: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut vk = None;
    for raw in spec.split('+') {
        let part = raw.trim().to_ascii_uppercase();
        match part.as_str() {
            "" => {}
            "CTRL" | "CONTROL" => mods |= MOD_CONTROL,
            "SHIFT" => mods |= MOD_SHIFT,
            "ALT" => mods |= MOD_ALT,
            "SPACE" => vk = Some(0x20),
            s if s.len() == 1 => {
                let b = s.as_bytes()[0];
                if b.is_ascii_alphanumeric() {
                    vk = Some(b as u32);
                }
            }
            s if s.starts_with('F') => {
                if let Ok(n) = s[1..].parse::<u32>() {
                    if (1..=12).contains(&n) {
                        vk = Some(0x70 + (n - 1));
                    }
                }
            }
            _ => return None,
        }
    }
    let key = vk?;
    if mods == 0 {
        return None;
    }
    Some((mods, key))
}

pub fn stop_hotkey_listener() {
    let prev = HOTKEY_THREAD_ID.swap(0, Ordering::SeqCst);
    if prev != 0 {
        unsafe {
            PostThreadMessageW(prev, WM_QUIT, 0, 0);
        }
    }
}

pub fn start_hotkey_listener(app: tauri::AppHandle, shortcut: &str) -> Result<(), String> {
    stop_hotkey_listener();
    if shortcut.trim().is_empty() || shortcut.eq_ignore_ascii_case("off") {
        return Ok(());
    }
    let Some((mods, vk)) = parse_shortcut(shortcut) else {
        return Err(format!("unsupported global shortcut: {shortcut}"));
    };

    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    std::thread::spawn(move || {
        // SAFETY: Registers a thread-scoped global hotkey (hwnd = null) and pumps Win32 messages until WM_QUIT.
        unsafe {
            let tid = GetCurrentThreadId();
            if RegisterHotKey(std::ptr::null_mut(), 1, mods | MOD_NOREPEAT, vk) == 0 {
                let _ = tx.send(Err("hotkey is already in use by another application".into()));
                return;
            }
            HOTKEY_THREAD_ID.store(tid, Ordering::SeqCst);
            let _ = tx.send(Ok(()));

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY && msg.wParam == 1 {
                    crate::tray::toggle_quick_switcher(&app);
                }
            }
            UnregisterHotKey(std::ptr::null_mut(), 1);
            let _ = HOTKEY_THREAD_ID.compare_exchange(tid, 0, Ordering::SeqCst, Ordering::SeqCst);
        }
    });

    rx.recv()
        .unwrap_or_else(|_| Err("hotkey thread exited unexpectedly".into()))
}

/// Sends a file to the Windows Recycle Bin (`FO_DELETE` + `FOF_ALLOWUNDO`) instead of permanently deleting it.
pub fn recycle_file(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE,
        SHFILEOPSTRUCTW,
    };

    if !path.exists() {
        return Err("file does not exist".into());
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0u16, 0u16]).collect();
    let mut op = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: wide.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    let rc = unsafe { SHFileOperationW(&mut op) };
    if rc != 0 || op.fAnyOperationsAborted != 0 {
        return Err(format!("Recycle Bin operation failed (code {rc})"));
    }
    Ok(())
}
