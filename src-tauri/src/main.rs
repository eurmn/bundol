// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::{engine::general_purpose, Engine as _};
use lazy_static::lazy_static;
use rand;
use reqwest::Url;
use serde_json::{json, Value};
use std::fs::File;
use std::io::prelude::*;
use std::net::TcpStream;
use std::ops::Index;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{
    CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
    Window,
};
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
    static ref SUMMONER_ID: Arc<Mutex<String>> = Arc::new(Mutex::new("".to_string()));
    static ref ENCODED_PASSWORD: Arc<Mutex<String>> = Arc::new(Mutex::new("".to_string()));
    static ref LCU_PORT: AtomicU32 = AtomicU32::new(0);
    static ref IS_USER_READY: AtomicBool = AtomicBool::new(true);
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
                port, "/lol-champ-select-legacy/v1/pickable-champion-ids"
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
            return None;
        }
    } else {
        return None;
    }
}

#[tauri::command]
fn ready(ready: bool) {
    IS_USER_READY.store(ready, std::sync::atomic::Ordering::Relaxed);
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
    // get https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/summoner-spells.json
    let spells = reqwest::blocking::get("https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/summoner-spells.json").unwrap().json::<Value>().unwrap();
    let spells = spells.as_array().unwrap();

    let spell_ids = spells
        .iter()
        .filter(|s| {
            s.get("gameModes")
                .unwrap()
                .as_array()
                .unwrap()
                .contains(&Value::String("CLASSIC".to_string()))
        })
        .map(|s| s.get("id").unwrap().as_i64().unwrap())
        .collect::<Vec<i64>>();

    let enc = general_purpose::STANDARD.encode(format!("riot:{}", password).as_bytes());

    ENCODED_PASSWORD.lock().unwrap().clear();
    ENCODED_PASSWORD.lock().unwrap().push_str(&enc);

    let mut socket = {
        let r: [u8; 16] = rand::random();
        let key = general_purpose::STANDARD.encode(&r);

        let url =
            Url::parse(("wss://".to_owned() + format!("127.0.0.1:{}/", port).as_str()).as_str())
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

        let (socket, _) = tungstenite::client_tls_with_config(
            req,
            stream,
            Some(WebSocketConfig::default()),
            Some(tungstenite::Connector::NativeTls(connector)),
        )
        .unwrap();

        socket
    };

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
                let res = res.json::<Value>().unwrap();

                let summoner_name = res.get("displayName").unwrap().as_str().unwrap().to_owned();
                let summoner_id = res.get("summonerId").unwrap().to_string().to_owned();

                SUMMONER_NAME.lock().unwrap().clear();
                SUMMONER_NAME.lock().unwrap().push_str(&summoner_name);

                SUMMONER_ID.lock().unwrap().clear();
                SUMMONER_ID.lock().unwrap().push_str(&summoner_id);

                window
                    .emit("lcu_summoner_name", Some(summoner_name))
                    .unwrap();
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
        "OnJsonApiEvent_lol-champ-select-legacy_v1_session",
        "OnJsonApiEvent_lol-champ-select_v1_session"
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

            if IS_USER_READY.load(std::sync::atomic::Ordering::Relaxed) {
                let desserialized = serde_json::from_str::<Vec<Value>>(&msg.to_string());

                if desserialized.is_err() {
                    continue;
                }

                let desserialized = desserialized.unwrap();

                if desserialized.index(0).as_u64().unwrap() != 8 {
                    continue;
                }

                if desserialized.index(2).get("uri").unwrap().as_str().unwrap()
                    == "/lol-champ-select-legacy/v1/session"
                    && desserialized
                        .index(2)
                        .get("eventType")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        == "Create"
                {
                    // [SELECT RANDOM SPELLS]

                    let available_spells = {
                        socket
                            .send(tungstenite::Message::Text(format!(
                                "[2, \"bundolrequest\", \"GET /lol-collections/v1/inventories/{}/spells\"]",
                                
                                SUMMONER_ID.lock().unwrap().clone()
                            )))
                            .unwrap();

                        let spells = loop {
                            let read = socket.read();
                            let msg = read.unwrap();

                            let desserialized =
                                serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

                            if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                                break desserialized
                                    .index(2)
                                    .get("spells")
                                    .unwrap()
                                    .as_array()
                                    .unwrap()
                                    .clone();
                            }
                        };

                        let mut available_spells = Vec::new();

                        for spell in spells {
                            let spell_id = spell.as_i64().unwrap();

                            if spell_ids.contains(&spell_id) {
                                available_spells.push(spell_id);
                            }
                        }

                        spell_ids
                            .iter()
                            .filter(|s| available_spells.contains(s))
                            .collect::<Vec<&i64>>()
                    };

                    let random_spells = {
                        let mut random_spells = Vec::new();

                        loop {
                            let spell =
                                available_spells[rand::random::<usize>() % available_spells.len()];

                            if !random_spells.contains(&spell) {
                                random_spells.push(spell);
                            }

                            if random_spells.len() == 2 {
                                break random_spells;
                            }
                        }
                    };


                    socket
                        .send(tungstenite::Message::Text(
                            format!("[2, \"bundolrequest\", \"PATCH /lol-champ-select-legacy/v1/session/my-selection\", {{ \"spell1Id\": {}, \"spell2Id\": {} }}]",
                            
                            random_spells[0].to_string(),
                            random_spells[1].to_string()
                        )
                        ))
                        .unwrap();

                    loop {
                        let read = socket.read();
                        let msg = read.unwrap();

                        let desserialized =
                            serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

                        if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                            break;
                        }
                    }

                    // [SELECT RANDOM RUNES]

                    socket
                        .send(tungstenite::Message::Text(
                            format!("[2, \"bundolrequest\", \"GET /lol-perks/v1/styles\"]").to_string(),
                        ))
                        .unwrap();

                    let runes = {
                        let mut selected_perks: Vec<i64> = vec![];

                        let styles = loop {
                            let read = socket.read();
                            let msg = read.unwrap();

                            let desserialized =
                                serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

                            if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                                println!("{:?}", desserialized);
                                break desserialized.index(2).clone();
                            }
                        };

                        // select random index of styles (for the main rune)
                        let random_style_index =
                            rand::random::<usize>() % styles.as_array().unwrap().len();
                        let primary_style = styles.index(random_style_index);

                        // go through each slot (except the last 3) and select a random perk and push it to selected_perks
                        let slots = primary_style.get("slots").unwrap().as_array().unwrap();

                        for slot in slots.iter().take(slots.len() - 3) {
                            let perks = slot.get("perks").unwrap().as_array().unwrap();

                            let random_perk_index = rand::random::<usize>() % perks.len();
                            let perk_id = perks.index(random_perk_index);

                            selected_perks.push(perk_id.as_i64().unwrap());
                        }

                        let allowed_secondary_styles = primary_style
                            .get("allowedSubStyles")
                            .unwrap()
                            .as_array()
                            .unwrap();
                        let random_secondary_style_index: usize =
                            rand::random::<usize>() % allowed_secondary_styles.len();
                        let secondary_style_id =
                            allowed_secondary_styles.index(random_secondary_style_index);

                        let secondary_style = styles
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|s| {
                                s.get("id").unwrap().as_i64().unwrap()
                                    == secondary_style_id.as_i64().unwrap()
                            })
                            .unwrap();

                        // go through each slot that has "type" != "kKeyStone" and select a random perk and push it to selected_perks
                        // there are 3 slots that have "type" == "kMixedRegularSplashable", however, only two of them will be used
                        // so one random slot will be ignored
                        let slots = secondary_style.get("slots").unwrap().as_array().unwrap();

                        let mut sub_slots_selected = 0;
                        for slot in slots {
                            if slot.get("type").unwrap().as_str().unwrap() == "kKeyStone" {
                                continue;
                            }

                            if slot.get("type").unwrap().as_str().unwrap()
                                == "kMixedRegularSplashable"
                            {
                                if sub_slots_selected == 2 {
                                    continue;
                                }

                                if rand::random::<bool>() {
                                    sub_slots_selected += 1;
                                    continue;
                                }
                            }

                            let perks = slot.get("perks").unwrap().as_array().unwrap();

                            let random_perk_index = rand::random::<usize>() % perks.len();
                            let perk_id = perks.index(random_perk_index);

                            selected_perks.push(perk_id.as_i64().unwrap());
                        }

                        // create a new json
                        let json = json!({
                            "name": format!("{} & {}", primary_style.get("name").unwrap().as_str().unwrap(), secondary_style.get("name").unwrap().as_str().unwrap()),
                            "selectedPerkIds": selected_perks,
                            "current": true,
                            "primaryStyleId": primary_style.get("id").unwrap().as_i64().unwrap(),
                            "subStyleId": secondary_style.get("id").unwrap().as_i64().unwrap()
                        });

                        json
                    };

                    socket
                        .send(tungstenite::Message::Text(
                            format!(
                                "[2, \"bundolrequest\", \"DELETE /lol-perks/v1/pages\", {}]",
                                
                                runes.to_string()
                            )
                            .to_string(),
                        ))
                        .unwrap();

                    loop {
                        let read = socket.read();
                        let msg = read.unwrap();

                        let desserialized =
                            serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

                        if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                            break;
                        }
                    }

                    socket
                        .send(tungstenite::Message::Text(
                            format!(
                                "[2, \"bundolrequest\", \"POST /lol-perks/v1/pages\", {}]",
                                runes.to_string()
                            )
                            .to_string(),
                        ))
                        .unwrap();

                    loop {
                        let read = socket.read();
                        let msg = read.unwrap();

                        let desserialized =
                            serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

                        if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                            break;
                        }
                    }

                    // [SELECT RANDOM CHAMPION]
                    'champion_loop: loop {
                        socket
                            .send(tungstenite::Message::Text(format!(
                                "[2, \"bundolrequest\", \"GET /lol-champ-select-legacy/v1/pickable-champion-ids\"]"
                            )))
                            .unwrap();

                        let pickable_champions = loop {
                            let read = socket.read();
                            let msg = read.unwrap();

                            let desserialized =
                                serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

                            if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                                if desserialized.index(0).as_u64().unwrap() == 3 {
                                    break desserialized
                                        .index(2)
                                        .clone()
                                        .as_array()
                                        .unwrap()
                                        .clone();
                                }
                            }
                        };

                        let random_champ = pickable_champions
                            [rand::random::<usize>() % pickable_champions.len()]
                        .as_i64()
                        .unwrap();

                        socket
                            .send(tungstenite::Message::Text(
                                format!("[2, \"bundolrequest\", \"PATCH /lol-champ-select-legacy/v1/session/actions/1\", {{ \"championId\": {}, \"completed\": true }}]",
                                
                                random_champ.to_string()
                            )
                            ))
                            .unwrap();

                        'pick_loop: loop {
                            let read = socket.read();
                            let msg = read.unwrap();

                            let desserialized =
                                serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

                            if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                                if desserialized.index(0).as_u64().unwrap() == 3 {
                                    break 'champion_loop;
                                }

                                if desserialized.index(0).as_u64().unwrap() == 4 {
                                    break 'pick_loop;
                                }
                            }
                        }
                    }
                }
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
    let quit = CustomMenuItem::new("quit".to_string(), "Sair");
    let restart = CustomMenuItem::new("restart".to_string(), "Reiniciar");
    let status = CustomMenuItem::new(
        "status".to_string(),
        if IS_USER_READY.load(std::sync::atomic::Ordering::Relaxed) {
            "Desativar"
        } else {
            "Ativar"
        },
    );

    let tray_menu = SystemTrayMenu::new()
        .add_item(status)
        .add_item(restart)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(quit);
    let system_tray = SystemTray::new().with_menu(tray_menu);

    tauri::Builder::default()
        .system_tray(system_tray)
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::LeftClick {
                position: _,
                size: _,
                ..
            } => {
                let window = app.get_window("main").unwrap();
                window.show().unwrap();
                window.set_focus().unwrap();
            }
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "quit" => {
                    std::process::exit(0);
                }
                "restart" => {
                    app.restart();
                }
                "status" => {
                    IS_USER_READY.store(
                        !IS_USER_READY.load(std::sync::atomic::Ordering::Relaxed),
                        std::sync::atomic::Ordering::Relaxed,
                    );

                    let item_handle = app.tray_handle().get_item("status");
                    let _ = item_handle.set_title(
                        if IS_USER_READY.load(std::sync::atomic::Ordering::Relaxed) {
                            "Desativar"
                        } else {
                            "Ativar"
                        },
                    );

                    let window = app.get_window("main").unwrap();
                    window
                        .emit(
                            "user-ready",
                            Some(IS_USER_READY.load(std::sync::atomic::Ordering::Relaxed)),
                        )
                        .unwrap();
                }
                _ => {}
            },
            _ => {}
        })
        .setup(move |app| {
            let window = app.get_window("main").unwrap();

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
            get_current_lobby,
            get_pickable_champions,
            ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
