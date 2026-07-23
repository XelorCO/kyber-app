use tauri::AppHandle;
use std::sync::OnceLock;

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

#[derive(serde::Serialize, Clone)]
pub struct ScanPayload {
    pub context: String,
}

/// Point d'entrée unique — dispatche vers l'implémentation de la plateforme.
pub fn start_scanner(app_handle: AppHandle) {
    let _ = APP_HANDLE.set(app_handle.clone());

    #[cfg(target_os = "windows")]
    windows_impl::start(app_handle);

    #[cfg(target_os = "macos")]
    macos_impl::start(app_handle);

    #[cfg(target_os = "linux")]
    linux_impl::start(app_handle);

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    log::warn!("[SCANNER] Plateforme non supportée — scanner désactivé.");
}

// ─────────────────────────────────────────────────────────────
//  WINDOWS  —  SetWinEventHook + UIAutomation
// ─────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{APP_HANDLE, ScanPayload};
    use tauri::Emitter;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation,
        SetWinEventHook,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoCreateInstance, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMessageW, TranslateMessage, DispatchMessageW, MSG,
        GetForegroundWindow, GetWindowTextW, GetWindowTextLengthW,
        EVENT_OBJECT_FOCUS, WINEVENT_OUTOFCONTEXT,
    };
    use windows::Win32::Foundation::{HMODULE, HWND};

    unsafe extern "system" fn win_event_proc(
        _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
        event: u32,
        _hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _id_event_thread: u32,
        _dwms_event_time: u32,
    ) {
        if event != EVENT_OBJECT_FOCUS {
            return;
        }

        if let Some(app) = APP_HANDLE.get() {
            let uia_result: Result<IUIAutomation, _> =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER);

            if let Ok(uia) = uia_result {
                if let Ok(element) = uia.GetFocusedElement() {
                    let is_pw = element
                        .CurrentIsPassword()
                        .unwrap_or(windows::Win32::Foundation::BOOL(0));

                    let mut by_name = false;
                    if let Ok(name) = element.CurrentName() {
                        let n = name.to_string().to_lowercase();
                        if n.contains("password") || n.contains("mot de passe") || n.contains("passwort") {
                            by_name = true;
                        }
                    }

                    if is_pw.as_bool() || by_name {
                        let fg = GetForegroundWindow();
                        let len = GetWindowTextLengthW(fg);
                        let mut buf = vec![0u16; (len + 1) as usize];
                        GetWindowTextW(fg, &mut buf);
                        let title = String::from_utf16_lossy(&buf)
                            .trim_matches('\0')
                            .to_string();

                        // Ne pas déclencher le scanner sur la fenêtre Kyber elle-même
                        if title.to_lowercase().contains("kyber") {
                            return;
                        }

                        log::info!("[SCANNER] Windows — champ password détecté dans '{}'", title);
                        let _ = app.emit("scanner-detected", ScanPayload { context: title });
                    }
                }
            }
        }
    }

    pub fn start(app_handle: tauri::AppHandle) {
        let _ = APP_HANDLE.set(app_handle);
        std::thread::spawn(|| unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let hook = SetWinEventHook(
                EVENT_OBJECT_FOCUS,
                EVENT_OBJECT_FOCUS,
                HMODULE::default(),
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );

            if hook.is_invalid() {
                log::error!("[SCANNER] SetWinEventHook échoué");
                return;
            }
            log::info!("[SCANNER] Hook Windows actif, boucle de messages démarrée.");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND::default(), 0, 0).into() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────
//  macOS  —  AXUIElement polling (300 ms)
// ─────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
mod macos_impl {
    use super::ScanPayload;
    use tauri::Emitter;
    use std::ffi::c_void;
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation::base::{CFTypeRef, TCFType};

    // AXError = 0 → kAXErrorSuccess
    type AXUIElementRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn CFRelease(cf: CFTypeRef);
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
    }

    /// Retourne le rôle AX de l'élément focalisé, ou None.
    unsafe fn focused_element_role() -> Option<String> {
        let sys = AXUIElementCreateSystemWide();
        if sys.is_null() {
            return None;
        }

        let attr_focused = CFString::new("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(
            sys,
            attr_focused.as_concrete_TypeRef(),
            &mut focused,
        );
        CFRelease(sys as CFTypeRef);

        if err != 0 || focused.is_null() {
            return None;
        }

        let attr_role = CFString::new("AXRole");
        let mut role_ref: CFTypeRef = std::ptr::null();
        let err2 = AXUIElementCopyAttributeValue(
            focused as AXUIElementRef,
            attr_role.as_concrete_TypeRef(),
            &mut role_ref,
        );
        CFRelease(focused);

        if err2 != 0 || role_ref.is_null() {
            return None;
        }

        // Vérifie que c'est bien un CFString avant de caster
        if CFGetTypeID(role_ref) != CFStringGetTypeID() {
            CFRelease(role_ref);
            return None;
        }

        let role = CFString::wrap_under_create_rule(role_ref as CFStringRef).to_string();
        Some(role)
    }

    /// Nom de l'app frontmost via NSWorkspace.
    fn frontmost_app_name() -> String {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::MainThreadMarker;
        unsafe {
            // NSWorkspace doit être appelé depuis le main thread si possible,
            // mais sharedWorkspace est thread-safe en lecture.
            let ws = NSWorkspace::sharedWorkspace();
            if let Some(app) = ws.frontmostApplication() {
                if let Some(name) = app.localizedName() {
                    return name.to_string();
                }
            }
        }
        "Unknown".to_string()
    }

    pub fn start(app_handle: tauri::AppHandle) {
        std::thread::spawn(move || {
            let mut last_was_password = false;
            loop {
                let role = unsafe { focused_element_role() };
                match role.as_deref() {
                    Some("AXSecureTextField") => {
                        if !last_was_password {
                            last_was_password = true;
                            let context = frontmost_app_name();
                            // Ne pas déclencher sur la fenêtre Kyber elle-même
                            if !context.to_lowercase().contains("kyber") {
                                log::info!("[SCANNER] macOS — champ password détecté dans '{}'", context);
                                let _ = app_handle.emit("scanner-detected", ScanPayload { context });
                            }
                        }
                    }
                    _ => {
                        last_was_password = false;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────
//  Linux  —  AT-SPI via atspi (async / tokio)
// ─────────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
mod linux_impl {
    use super::ScanPayload;
    use tauri::Emitter;

    pub fn start(app_handle: tauri::AppHandle) {
        tauri::async_runtime::spawn(async move {
            match run_atspi_scanner(app_handle).await {
                Ok(_) => {}
                Err(e) => log::error!("[SCANNER] AT-SPI erreur: {}", e),
            }
        });
    }

    async fn run_atspi_scanner(
        app_handle: tauri::AppHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use atspi::{AccessibilityConnection, Role};
        use atspi::events::object::StateChangedEvent;
        use atspi::proxy::accessible::AccessibleProxy;
        use futures_util::StreamExt;

        let atspi = AccessibilityConnection::open().await?;
        atspi.register_event::<StateChangedEvent>().await?;

        let mut stream = atspi.event_stream();
        let mut last_was_password = false;

        while let Some(Ok(event)) = stream.next().await {
            if let Ok(ev) = StateChangedEvent::try_from(event) {
                // On ne s'intéresse qu'aux gains de focus
                if ev.state().to_string() != "focused" || ev.enabled() == 0 {
                    last_was_password = false;
                    continue;
                }

                // Récupère le proxy AT-SPI de l'élément focalisé
                let proxy_result = AccessibleProxy::builder(atspi.connection())
                    .destination(ev.sender().as_str())?
                    .path(ev.path())?
                    .build()
                    .await;

                let Ok(proxy) = proxy_result else {
                    continue;
                };

                // Vérifie le rôle : PasswordText = champ password natif
                let is_pw = match proxy.get_role().await {
                    Ok(role) => role == Role::PasswordText,
                    Err(_) => false,
                };

                if is_pw && !last_was_password {
                    last_was_password = true;

                    // Remonte jusqu'au nœud Application pour lire le vrai nom de l'app.
                    // La hiérarchie AT-SPI est : Application → Window → ... → Element
                    let app_name = get_app_name_from_proxy(&proxy, atspi.connection()).await;

                    // Ne pas déclencher sur la fenêtre Kyber elle-même
                    if !app_name.to_lowercase().contains("kyber") {
                        log::info!("[SCANNER] Linux — champ password détecté dans '{}'", app_name);
                        let _ = app_handle.emit("scanner-detected", ScanPayload { context: app_name });
                    }
                } else if !is_pw {
                    last_was_password = false;
                }
            }
        }

        Ok(())
    }

    /// Remonte la hiérarchie AT-SPI jusqu'au nœud de rôle `Application`
    /// et retourne son nom accessible. Fallback sur "Unknown".
    async fn get_app_name_from_proxy(
        proxy: &atspi::proxy::accessible::AccessibleProxy<'_>,
        conn: &zbus::Connection,
    ) -> String {
        use atspi::Role;
        use atspi::proxy::accessible::AccessibleProxy;

        // Démarre depuis le proxy de l'élément focalisé
        let mut dest = proxy.inner().destination().to_string();
        let mut path = proxy.inner().path().to_string();

        // Remonte la hiérarchie jusqu'à trouver le nœud Application (max 10 niveaux)
        for _ in 0..10 {
            let p = match AccessibleProxy::builder(conn)
                .destination(dest.as_str())
                .ok()
                .and_then(|b| b.path(path.as_str()).ok())
            {
                Some(b) => match b.build().await {
                    Ok(p) => p,
                    Err(_) => break,
                },
                None => break,
            };

            match p.get_role().await {
                Ok(Role::Application) => {
                    // Nœud Application trouvé — lire le nom
                    return p.name().await.unwrap_or_else(|_| "Unknown".to_string());
                }
                Ok(_) => {}
                Err(_) => break,
            }

            // Remonte au parent
            match p.get_parent().await {
                Ok((parent_dest, parent_path)) => {
                    dest = parent_dest.to_string();
                    path = parent_path.to_string();
                }
                Err(_) => break,
            }
        }

        // Fallback : nom direct de l'élément
        proxy.name().await.unwrap_or_else(|_| "Unknown".to_string())
    }
}

