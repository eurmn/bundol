// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::{engine::general_purpose, Engine as _};
use lazy_static::lazy_static;
use rand;
use reqwest::Url;
use serde_json::Value;
use std::fs::File;
use std::io::prelude::*;
use std::net::TcpStream;
use std::ops::Index;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::Manager;
use tauri::Window;
use tungstenite::http::Request;
use tungstenite::protocol::WebSocketConfig;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct LcuSubscribePayload {
    eventy_type: String,
    event: String,
}

lazy_static! {
    static ref IS_CONNECTED_TO_LCU: AtomicBool = AtomicBool::new(false);
    static ref LOCKFILE: Arc<Mutex<String>> = Arc::new(Mutex::new("".to_string()));
    static ref SUMMONER_NAME: Arc<Mutex<String>> = Arc::new(Mutex::new("".to_string()));
    static ref ENCODED_PASSWORD: Arc<Mutex<String>> = Arc::new(Mutex::new("".to_string()));
    static ref LCU_PORT: AtomicU32 = AtomicU32::new(0);
}

#[tauri::command]
fn is_connected_to_lcu() -> bool {
    return IS_CONNECTED_TO_LCU.load(std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
fn lcu_summoner_name() -> String {
    return SUMMONER_NAME.lock().unwrap().clone();
}

#[tauri::command]
fn create_lobby() {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let port = LCU_PORT.load(std::sync::atomic::Ordering::Relaxed);
    let enc = ENCODED_PASSWORD.lock().unwrap().clone();

    let res = client
        .post(format!("https://127.0.0.1:{}{}", port, "/lol-lobby/v2/lobby").as_str())
        .header("Authorization", format!("Basic {}", enc).as_str())
        .json(&serde_json::json!({
            "queueId": 430,
        }))
        .send();

    if res.is_ok() {
        let res = res.unwrap();

        if res.status().is_success() {
            println!("Lobby created");
        } else {
            println!("Failed to create lobby: {}", res.status());
        }
    } else {
        println!("Failed to create lobby: {}", res.err().unwrap());
    }
}

#[tauri::command]
fn get_current_lobby() -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let port = LCU_PORT.load(std::sync::atomic::Ordering::Relaxed);
    let enc = ENCODED_PASSWORD.lock().unwrap().clone();

    let res = client
        .get(format!("https://127.0.0.1:{}{}", port, "/lol-lobby/v2/lobby").as_str())
        .header("Authorization", format!("Basic {}", enc).as_str())
        .send();

    if res.is_ok() {
        let res = res.unwrap();

        if res.status().is_success() {
            let res = res.json::<Value>().unwrap();
            let members = res.get("members").unwrap();

            return Some(members.to_string());
        } else {
            return None;
        }
    } else {
        return None;
    }
}

#[tauri::command]
fn select_champion(id: i64) {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let port = LCU_PORT.load(std::sync::atomic::Ordering::Relaxed);
    let enc = ENCODED_PASSWORD.lock().unwrap().clone();

    let res = client
        .patch(
            format!(
                "https://127.0.0.1:{}{}",
                port, "/lol-champ-select/v1/session/actions/1"
            )
            .as_str(),
        )
        .header("Authorization", format!("Basic {}", enc).as_str())
        .json(&serde_json::json!({
            "championId": id,
        }))
        .send();

    if res.is_ok() {
        let res = res.unwrap();

        if res.status().is_success() {
            println!("Champion selected");
        } else {
            println!("Failed to select champion: {}", res.status());
        }
    } else {
        println!("Failed to select champion: {}", res.err().unwrap());
    }
}

#[tauri::command]
fn get_pickable_champions() -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let port = LCU_PORT.load(std::sync::atomic::Ordering::Relaxed);
    let enc = ENCODED_PASSWORD.lock().unwrap().clone();

    let res = client
        .get(
            format!(
                "https://127.0.0.1:{}{}",
                port, "/lol-champ-select/v1/pickable-champion-ids"
            )
            .as_str(),
        )
        .header("Authorization", format!("Basic {}", enc).as_str())
        .send();

    if res.is_ok() {
        let res = res.unwrap();

        if res.status().is_success() {
            let res = res.json::<Value>().unwrap();

            return Some(res.to_string());
        } else {
            println!("{}", res.status());
            return None;
        }
    } else {
        println!("not ok");
        return None;
    }
}

fn find_league_lockfile() -> String {
    let mut file: File = loop {
        let file = File::open("C:/Riot Games/League of Legends/lockfile");

        if file.is_ok() {
            println!("Found lockfile");
            break file.unwrap();
        }

        thread::sleep(std::time::Duration::from_secs(1));
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    return contents;
}

fn start_league_watcher(port: u32, password: &str, window: &Window) {
    let enc = general_purpose::STANDARD.encode(format!("riot:{}", password).as_bytes());

    ENCODED_PASSWORD.lock().unwrap().clear();
    ENCODED_PASSWORD.lock().unwrap().push_str(&enc);

    let r: [u8; 16] = rand::random();
    let key = general_purpose::STANDARD.encode(&r);
    let url = Url::parse(("wss://".to_owned() + format!("127.0.0.1:{}/", port).as_str()).as_str())
        .unwrap();

    let req = Request::builder()
        .header("Authorization", format!("Basic {}", enc).as_str())
        .method("GET")
        .header("Host", url.host_str().unwrap())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", key)
        .uri(url.as_str())
        .body(())
        .unwrap();

    let connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let addr = format!("{}:{}", url.host_str().unwrap(), port);

    let stream = loop {
        let con = TcpStream::connect(addr.to_owned());

        if con.is_ok() {
            break con.unwrap();
        }

        thread::sleep(std::time::Duration::from_secs(1));
    };

    let (mut socket, _) = tungstenite::client_tls_with_config(
        req,
        stream,
        Some(WebSocketConfig::default()),
        Some(tungstenite::Connector::NativeTls(connector)),
    )
    .unwrap();

    println!("Connected to LCU");
    IS_CONNECTED_TO_LCU.store(true, std::sync::atomic::Ordering::Relaxed);

    window.emit("lcu-connected", Option::<()>::None).unwrap();

    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    loop {
        let res = client
            .get(
                format!(
                    "https://127.0.0.1:{}/lol-summoner/v1/current-summoner",
                    port
                )
                .as_str(),
            )
            .header("Authorization", format!("Basic {}", enc).as_str())
            .send();

        if res.is_ok() {
            let res = res.unwrap();

            if res.status().is_success() {
                let res = res
                    .json::<Value>()
                    .unwrap()
                    .get("displayName")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned();

                SUMMONER_NAME.lock().unwrap().clear();
                SUMMONER_NAME.lock().unwrap().push_str(&res);
                window.emit("lcu_summoner_name", Some(res)).unwrap();
                break;
            } else {
                println!("Failed to get summoner name: {}", res.status());
                thread::sleep(std::time::Duration::from_secs(1));

                if res.status() != 404 {
                    break;
                }
            }
        }
    }

    thread::sleep(std::time::Duration::from_secs(1));

    let sub_events = vec![
        "OnJsonApiEvent_lol-lobby_v2_lobby",
        "OnJsonApiEvent_lol-summoner_v1_current-summoner",
        "OnJsonApiEvent_lol-champ-select_v1_pickable-champion-ids",
        "OnJsonApiEvent_lol-champ-select-legacy_v1_current-champion",
    ];

    for event in sub_events {
        socket
            .send(tungstenite::Message::Text(
                format!("[5, \"{}\"]", event).to_string(),
            ))
            .unwrap();
    }

    loop {
        let read = socket.read();

        if read.is_ok() {
            let msg = read.unwrap();

            // println!("Message: {}", msg.to_string());

            let desserialized = serde_json::from_str::<Vec<Value>>(&msg.to_string());

            if desserialized.is_err() {
                continue;
            }

            let desserialized = desserialized.unwrap();

            if desserialized.index(0).as_u64().unwrap() != 8 {
                continue;
            }

            if desserialized.index(2).get("uri").unwrap().as_str().unwrap()
                == "/lol-champ-select/v1/pickable-champion-ids"
                && desserialized
                    .index(2)
                    .get("eventType")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    != "Delete"
            {
                // CALL PatchLolChampSelectV1SessionActionsById
                let pickable_champions = desserialized
                    .index(2)
                    .get("data")
                    .unwrap()
                    .as_array()
                    .unwrap();

                let random_champ = pickable_champions
                    [rand::random::<usize>() % pickable_champions.len()]
                .as_i64()
                .unwrap();

                let r: [u8; 16] = rand::random();
                let key = general_purpose::STANDARD.encode(&r);

                // pick champion
                socket
                    .send(tungstenite::Message::Text(
                        format!("[2, \"{}\", \"PATCH /lol-champ-select/v1/session/actions/1\", {{ \"championId\": {}, \"completed\": true }}]",
                        key,
                        random_champ.to_string()
                    )
                    ))
                    .unwrap();

                println!("pickable");
            }

            window.emit("lcu-message", Some(msg.to_string())).unwrap();

            if msg.is_close() {
                break;
            }

            continue;
        };

        break;
    }

    window.emit("lcu-disconnected", Option::<()>::None).unwrap();

    IS_CONNECTED_TO_LCU.store(false, std::sync::atomic::Ordering::Relaxed);
    SUMMONER_NAME.lock().unwrap().clear();
}

fn main() {
    tauri::Builder::default()
        .setup(move |app| {
            let window = app.get_window("main").unwrap();

            #[cfg(dev)]
            window.open_devtools();

            tauri::async_runtime::spawn(async move {
                loop {
                    println!("Starting league watcher");

                    let lockfile = find_league_lockfile();

                    LOCKFILE.lock().unwrap().clear();
                    LOCKFILE.lock().unwrap().push_str(&lockfile);

                    let mut split = lockfile.split(":");

                    let _ = split.next();
                    let _ = split.next();

                    let port = split.next().unwrap().parse::<u32>().unwrap();
                    LCU_PORT.store(port, std::sync::atomic::Ordering::Relaxed);

                    let password = split.next().unwrap();

                    start_league_watcher(port, password, &window);
                    println!("Connection closed");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            is_connected_to_lcu,
            lcu_summoner_name,
            create_lobby,
            get_current_lobby,
            get_pickable_champions,
            select_champion
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
