//! Native WebView2 handling for browser refresh accelerators.
//!
//! The renderer cannot reliably intercept these keys because WebView2 handles
//! browser accelerators before dispatching keyboard events to the page. Keep
//! this policy deliberately small so editing, zoom, and navigation shortcuts
//! continue to use WebView2's normal behavior.

use std::sync::{Arc, Mutex};

use tauri::Runtime;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, ContextMenuRequestedEventHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU,
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
        COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN, ICoreWebView2_11,
        ICoreWebView2ContextMenuItemCollection, ICoreWebView2Controller,
    },
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::core::{Interface, PWSTR};

const VK_R: u32 = 0x52;
const VK_F5: u32 = 0x74;
const VK_BROWSER_REFRESH: u32 = 0xA8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Modifiers {
    control: bool,
    shift: bool,
    alt: bool,
    windows: bool,
}

/// Returns whether WebView2 should consume this accelerator as a refresh.
///
/// This remains independent from the OS keyboard state so the policy can be
/// tested without a window. Browser refresh is blocked for F5 and Ctrl+F5,
/// and for Ctrl+R / Ctrl+Shift+R. AltGr and other platform combinations keep
/// their normal behavior.
fn blocks_reload_accelerator(
    virtual_key: u32,
    key_event_kind: COREWEBVIEW2_KEY_EVENT_KIND,
    modifiers: Modifiers,
) -> bool {
    if !matches!(
        key_event_kind,
        COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN | COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
    ) || modifiers.alt
        || modifiers.windows
    {
        return false;
    }

    match virtual_key {
        VK_F5 | VK_BROWSER_REFRESH => true,
        VK_R => modifiers.control,
        _ => false,
    }
}

fn is_reload_context_menu_item(name: &str) -> bool {
    name == "reload"
}

fn remove_reload_context_menu_items(
    items: &ICoreWebView2ContextMenuItemCollection,
) -> Result<(), windows::core::Error> {
    let mut count = 0;
    unsafe { items.Count(&mut count)? };

    for index in (0..count).rev() {
        let item = unsafe { items.GetValueAtIndex(index)? };
        let mut raw_name = PWSTR::null();
        unsafe { item.Name(&mut raw_name)? };
        let name = webview2_com::take_pwstr(raw_name);
        if is_reload_context_menu_item(&name) {
            unsafe { items.RemoveValueAtIndex(index)? };
            continue;
        }

        let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
        unsafe { item.Kind(&mut kind)? };
        if kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU {
            let children = unsafe { item.Children()? };
            remove_reload_context_menu_items(&children)?;
        }
    }

    Ok(())
}

fn key_is_down(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    // The high bit is the documented GetKeyState down flag. The callback is
    // delivered on WebView2's UI thread, so this is the state for this event.
    unsafe { GetKeyState(key.0 as i32) < 0 }
}

fn current_modifiers() -> Modifiers {
    Modifiers {
        control: key_is_down(VK_CONTROL),
        shift: key_is_down(VK_SHIFT),
        alt: key_is_down(VK_MENU),
        windows: key_is_down(VK_LWIN) || key_is_down(VK_RWIN),
    }
}

fn install_controller(controller: &ICoreWebView2Controller) -> Result<(), windows::core::Error> {
    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else {
            return Ok(());
        };

        let mut key_event_kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
        let mut virtual_key = 0;
        unsafe {
            args.KeyEventKind(&mut key_event_kind)?;
            args.VirtualKey(&mut virtual_key)?;
        }

        if blocks_reload_accelerator(virtual_key, key_event_kind, current_modifiers()) {
            unsafe { args.SetHandled(true)? };
        }

        Ok(())
    }));

    let mut accelerator_token = 0;
    unsafe { controller.add_AcceleratorKeyPressed(&handler, &mut accelerator_token)? };

    let webview = unsafe { controller.CoreWebView2()? };
    let webview_11: ICoreWebView2_11 = webview.cast()?;
    let context_menu_handler = ContextMenuRequestedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let items = match unsafe { args.MenuItems() } {
            Ok(items) => items,
            Err(error) => {
                // A partially filtered menu could otherwise leave the browser
                // reload command available. Hide the menu if enumeration fails.
                eprintln!("failed to inspect WebView2 context menu: {error}");
                unsafe { args.SetHandled(true)? };
                return Ok(());
            }
        };
        if let Err(error) = remove_reload_context_menu_items(&items) {
            eprintln!("failed to filter WebView2 reload context menu item: {error}");
            unsafe { args.SetHandled(true)? };
        }
        Ok(())
    }));
    let mut context_menu_token = 0;
    unsafe { webview_11.add_ContextMenuRequested(&context_menu_handler, &mut context_menu_token)? };
    Ok(())
}

/// Install the refresh policy before the first renderer interaction.
///
/// Tauri's callback does not carry a return value, so retain the registration
/// outcome and turn callback failure into a setup error. A missing callback is
/// also an error: startup must never silently run without the native guard.
pub(crate) fn install<R: Runtime>(window: &tauri::WebviewWindow<R>) -> tauri::Result<()> {
    let outcome = Arc::new(Mutex::new(None));
    let callback_outcome = Arc::clone(&outcome);

    window.with_webview(move |webview| {
        let result = install_controller(&webview.controller())
            .map_err(|error| format!("registering WebView2 refresh accelerator guard: {error}"));
        *callback_outcome
            .lock()
            .expect("refresh accelerator setup mutex poisoned") = Some(result);
    })?;

    let result = outcome
        .lock()
        .expect("refresh accelerator setup mutex poisoned")
        .take()
        .ok_or_else(|| {
            std::io::Error::other("WebView2 refresh accelerator setup callback did not run")
        })?;
    result.map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VK_0: u32 = 0x30;
    const VK_L: u32 = 0x4C;
    const VK_OEM_PLUS: u32 = 0xBB;
    const VK_OEM_MINUS: u32 = 0xBD;

    const KEY_DOWN: COREWEBVIEW2_KEY_EVENT_KIND = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
    const KEY_UP: COREWEBVIEW2_KEY_EVENT_KIND =
        webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_KEY_EVENT_KIND_KEY_UP;

    fn modifiers(control: bool, shift: bool, alt: bool, windows: bool) -> Modifiers {
        Modifiers {
            control,
            shift,
            alt,
            windows,
        }
    }

    #[test]
    fn blocks_only_reload_accelerators() {
        assert!(blocks_reload_accelerator(
            VK_R,
            KEY_DOWN,
            modifiers(true, false, false, false)
        ));
        assert!(blocks_reload_accelerator(
            VK_R,
            KEY_DOWN,
            modifiers(true, true, false, false)
        ));
        assert!(blocks_reload_accelerator(
            VK_F5,
            KEY_DOWN,
            Modifiers::default()
        ));
        assert!(blocks_reload_accelerator(
            VK_F5,
            KEY_DOWN,
            modifiers(true, false, false, false)
        ));
        assert!(blocks_reload_accelerator(
            VK_BROWSER_REFRESH,
            KEY_DOWN,
            Modifiers::default()
        ));
    }

    #[test]
    fn preserves_edit_zoom_navigation_and_other_key_variants() {
        assert!(!blocks_reload_accelerator(
            VK_R,
            KEY_DOWN,
            modifiers(false, false, false, false)
        ));
        assert!(!blocks_reload_accelerator(
            VK_R,
            KEY_DOWN,
            modifiers(true, false, true, false)
        ));
        assert!(blocks_reload_accelerator(
            VK_F5,
            KEY_DOWN,
            modifiers(false, true, false, false)
        ));
        assert!(blocks_reload_accelerator(
            VK_F5,
            KEY_DOWN,
            modifiers(true, true, false, false)
        ));
        for virtual_key in [VK_OEM_PLUS, VK_OEM_MINUS, VK_0, VK_L] {
            assert!(!blocks_reload_accelerator(
                virtual_key,
                KEY_DOWN,
                modifiers(true, false, false, false)
            ));
        }
        assert!(!blocks_reload_accelerator(
            VK_R,
            KEY_UP,
            modifiers(true, false, false, false)
        ));
        assert!(!blocks_reload_accelerator(
            VK_R,
            KEY_DOWN,
            modifiers(true, false, false, true)
        ));
    }

    #[test]
    fn handles_system_key_down_for_the_same_policy() {
        assert!(blocks_reload_accelerator(
            VK_R,
            COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
            modifiers(true, true, false, false)
        ));
    }

    #[test]
    fn context_menu_filter_uses_unlocalized_reload_name_only() {
        assert!(is_reload_context_menu_item("reload"));
        assert!(!is_reload_context_menu_item("Reload"));
        assert!(!is_reload_context_menu_item("copy"));
        assert!(!is_reload_context_menu_item("paste"));
        assert!(!is_reload_context_menu_item("refresh"));
    }
}
