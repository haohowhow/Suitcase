#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args_os().any(|argument| argument == "--unregister-sync-roots") {
        std::process::exit(
            if koofr_gui_lib::cleanup_native_sync_roots_for_uninstall() {
                0
            } else {
                1
            },
        );
    }
    koofr_gui_lib::run();
}
