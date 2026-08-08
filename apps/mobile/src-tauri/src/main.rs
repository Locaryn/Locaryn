// Desktop entry point. Android goes through the mobile_entry_point in lib.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    locaryn_mobile_lib::run()
}
