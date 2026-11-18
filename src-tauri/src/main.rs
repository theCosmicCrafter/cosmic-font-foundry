#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Elevated worker mode (for system font operations) executes without opening a GUI window.
    if let Some(code) = cosmic_fonts_lib::elevated_mode() {
        std::process::exit(code);
    }
    cosmic_fonts_lib::run()
}
