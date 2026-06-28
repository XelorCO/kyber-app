#[cfg(windows)]
pub mod windows_ui {
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use std::ptr;

    // Détection basique: On écoute les changements de focus via SetWinEventHook
    // En vrai prod avec UIAutomation, on implémenterait IUIAutomationFocusChangedEventHandler
    // Pour ce proto "sans fioritures", on fait un hook système d'événements.

    unsafe extern "system" fn win_event_proc(
        _h_win_event_hook: HWINEVENTHOOK,
        event: u32,
        _hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _id_event_thread: u32,
        _dwms_event_time: u32,
    ) {
        if event == EVENT_OBJECT_FOCUS {
            // Un nouvel élément a le focus.
            // On peut interroger UIAutomation pour vérifier si c'est un champ password.
            // Pour le proto, on affiche juste l'ID ou on log l'event.
            // On pourrait faire un CoCreateInstance pour IUIAutomation et ElementFromHandle.
            // println!("Focus changed to HWND: {:?}", hwnd);
        }
    }

    pub fn start_scanner() {
        unsafe {
            // Init COM en MTA
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            // Création de l'instance CUIAutomation
            let uia: Result<IUIAutomation, _> = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER);
            
            match uia {
                Ok(_automation) => {
                    println!("[*] Scanner UIAutomation initialisé. En attente de détection...");
                    // On place le hook système pour détecter les focus.
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
                        eprintln!("[-] Erreur SetWinEventHook");
                        return;
                    }

                    // Boucle de messages Windows classique (bloquante)
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, HWND::default(), 0, 0).into() {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                Err(e) => {
                    eprintln!("[-] Echec initialisation UIAutomation: {:?}", e);
                }
            }
        }
    }
}

#[cfg(not(windows))]
pub mod dummy_scanner {
    pub fn start_scanner() {
        println!("[!] Le scanner AT-SPI/AX API n'est pas encore implémenté pour ce proto sur cet OS.");
    }
}
