//! System tray icon, custom frameless tray popup (`menu`), and Spotlight-style Quick-Switcher (`quick`).

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use windows_sys::Win32::{
    Foundation::POINT,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
        },
        WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, IsWindowVisible},
    },
};

use crate::{err, init_script};

const MENU_WIDTH: f64 = 280.0;
const MENU_HEIGHT: f64 = 356.0;
const QUICK_WIDTH: f64 = 580.0;
const QUICK_HEIGHT: f64 = 452.0;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct PinnedCollection {
    pub id: String,
    pub name: String,
    pub fonts: Vec<String>,
    pub all_active: bool,
}

#[derive(Clone, Serialize, Default)]
pub struct TrayStatus {
    pub status: String,
    pub folders: u32,
    pub theme: String,
    pub language: String,
    pub shortcut: String,
    pub pinned: Vec<PinnedCollection>,
}

#[derive(Default)]
pub struct TrayState(Mutex<TrayStatus>);

pub fn create(app: &AppHandle, theme: &str, language: &str) -> tauri::Result<()> {
    app.manage(TrayState(Mutex::new(TrayStatus {
        theme: theme.into(),
        language: language.into(),
        shortcut: "Ctrl+Shift+F".into(),
        ..Default::default()
    })));

    let menu_init = json!({ "settings": { "theme": theme, "language": language }, "mode": "menu" });
    let menu = WebviewWindowBuilder::new(app, "menu", WebviewUrl::default())
        .title("Cosmic Font Foundry Menu")
        .inner_size(MENU_WIDTH, MENU_HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .focusable(false)
        .zoom_hotkeys_enabled(false)
        .initialization_script(init_script(&menu_init))
        .build()?;

    let menu_clone = menu.clone();
    menu.on_window_event(move |ev| {
        if let WindowEvent::CloseRequested { api, .. } = ev {
            api.prevent_close();
            let _ = menu_clone.hide();
        }
    });

    let quick_init =
        json!({ "settings": { "theme": theme, "language": language }, "mode": "quick" });
    let quick = WebviewWindowBuilder::new(app, "quick", WebviewUrl::default())
        .title("Cosmic Quick-Switcher")
        .inner_size(QUICK_WIDTH, QUICK_HEIGHT)
        .center()
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .zoom_hotkeys_enabled(false)
        .initialization_script(init_script(&quick_init))
        .build()?;

    let quick_clone = quick.clone();
    quick.on_window_event(move |ev| match ev {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = quick_clone.hide();
        }
        WindowEvent::Focused(false) => {
            let _ = quick_clone.hide();
        }
        _ => {}
    });

    let mut builder = TrayIconBuilder::with_id("cosmic-font-foundry")
        .tooltip("Cosmic Font Foundry")
        .on_tray_icon_event(|icon, ev| {
            if let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = ev
            {
                match button {
                    MouseButton::Left => show_main(icon.app_handle()),
                    MouseButton::Right => open_menu(icon.app_handle(), position),
                    _ => {}
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn open_menu(app: &AppHandle, cursor: PhysicalPosition<f64>) {
    let Some(menu) = app.get_webview_window("menu") else {
        return;
    };
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let (w, h) = (MENU_WIDTH * scale, MENU_HEIGHT * scale);
    let (left, top) = (area.position.x as f64, area.position.y as f64);
    let (right, bottom) = (left + area.size.width as f64, top + area.size.height as f64);
    let x = (cursor.x - w).clamp(left, right - w);
    let y = (cursor.y - h).clamp(top, bottom - h);
    let _ = menu.set_position(PhysicalPosition::new(x, y));
    let _ = menu.emit("menu-opened", ());
    let _ = menu.show();
    watch_menu_dismiss(menu, (x, y, w, h));
}

static MENU_SEQ: AtomicU64 = AtomicU64::new(0);

fn is_key_down(vk: u16) -> bool {
    (unsafe { GetAsyncKeyState(vk as i32) } as u16) & 0x8000 != 0
}

fn any_mouse_down() -> bool {
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
        .into_iter()
        .any(is_key_down)
}

fn watch_menu_dismiss(menu: WebviewWindow, (x, y, w, h): (f64, f64, f64, f64)) {
    let seq = MENU_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let prev_fg = unsafe { GetForegroundWindow() } as isize;
    thread::spawn(move || {
        let alive = || MENU_SEQ.load(Ordering::SeqCst) == seq && menu.is_visible().unwrap_or(false);
        let own_hwnd = menu.hwnd().map_or(0, |hwnd| hwnd.0 as isize);
        let focus_lost = || {
            let now = unsafe { GetForegroundWindow() } as isize;
            let visible = prev_fg == 0 || unsafe { IsWindowVisible(prev_fg as _) } != 0;
            !visible || (now != prev_fg && now != own_hwnd && now != 0)
        };
        while any_mouse_down() {
            if !alive() {
                return;
            }
            thread::sleep(Duration::from_millis(15));
        }
        while alive() {
            thread::sleep(Duration::from_millis(20));
            if is_key_down(VK_ESCAPE) || focus_lost() {
                let _ = menu.hide();
                return;
            }
            if !any_mouse_down() {
                continue;
            }
            let mut p = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut p) };
            let (px, py) = (p.x as f64, p.y as f64);
            if px < x || px >= x + w || py < y || py >= y + h {
                let _ = menu.hide();
                return;
            }
            while any_mouse_down() && alive() {
                thread::sleep(Duration::from_millis(15));
            }
        }
    });
}

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn toggle_quick_switcher(app: &AppHandle) {
    let Some(quick) = app.get_webview_window("quick") else {
        return;
    };
    if quick.is_visible().unwrap_or(false) {
        let _ = quick.hide();
        return;
    }
    let _ = quick.center();
    let _ = quick.emit("quick-opened", ());
    let _ = quick.show();
    let _ = quick.set_focus();
}

#[tauri::command]
pub fn update_tray(
    app: AppHandle,
    status: String,
    folders: u32,
    theme: String,
    language: String,
    shortcut: String,
    pinned: Vec<PinnedCollection>,
) {
    let next = TrayStatus {
        status,
        folders,
        theme,
        language,
        shortcut,
        pinned,
    };
    if let Some(state) = app.try_state::<TrayState>() {
        *state.0.lock().unwrap() = next.clone();
    }
    let _ = app.emit_to("menu", "tray-status", &next);
    let _ = app.emit_to("quick", "tray-status", &next);
}

#[tauri::command]
pub fn get_tray_status(app: AppHandle) -> TrayStatus {
    app.try_state::<TrayState>()
        .map(|s| s.0.lock().unwrap().clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn tray_action(app: AppHandle, action: String) -> Result<(), String> {
    if let Some(m) = app.get_webview_window("menu") {
        let _ = m.hide();
    }
    match action.as_str() {
        "open" => show_main(&app),
        "quick" => toggle_quick_switcher(&app),
        "exit" => app.exit(0),
        "close" => {}
        "search" | "discover" | "settings" => {
            show_main(&app);
            app.emit_to("main", "tray-command", action).map_err(err)?;
        }
        other => return Err(format!("unknown tray action: {other}")),
    }
    Ok(())
}
