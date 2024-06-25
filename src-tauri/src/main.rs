// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::{CustomMenuItem, Manager, SystemTray, SystemTrayMenu, SystemTrayMenuItem};
use tokio::sync::{broadcast, Mutex};
use tracing::info;
use monitor::core::{manager, Scheduler, Task};

use monitor::utils;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}


#[tokio::main]
async fn main() {
    let _guard = utils::logger::init("./".to_string()).unwrap();

    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let show = CustomMenuItem::new("quit".to_string(), "Show");
    let hide = CustomMenuItem::new("hide".to_string(), "Hide");
    let tray_menu = SystemTrayMenu::new()
        .add_item(quit)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(show)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(hide);
    let tray = SystemTray::new().with_menu(tray_menu);

    let (tx, rx) = broadcast::channel(1);
    let mut m = Arc::new(Mutex::new(manager::Manager::new(rx)));
    info!("Init Screen Monitor...");
    let task = Task::default();
    let _ = m.lock().await.start(task.clone()).await;

    let mm = Arc::clone(&m);
    tokio::spawn(async move {
        mm.lock().await.monitor().await;
    });

    tauri::Builder::default()
        .setup(|_app| {

            Ok(())
        })
        .system_tray(tray)
        .on_window_event(|event| match event.event() {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                event.window().hide().unwrap();
                api.prevent_close();
            }
            _ => {}
        })
        .on_system_tray_event(move |app, event| match event {
            tauri::SystemTrayEvent::MenuItemClick { id, .. } => {
                if id == "quit" {
                    let val = task.clone();
                    let m = Arc::clone(&m);
                    tokio::spawn(async move {
                        m.lock().await.stop(val).await;
                    });
                    app.exit(0);
                } else if id == "hide" {
                    app.get_window("main").unwrap().hide().unwrap();
                } else if id == "show" { // show
                    app.get_window("main").unwrap().show().unwrap();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![greet])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app_handle, event| match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                api.prevent_exit();
            }
            _ => {}
        });
}
