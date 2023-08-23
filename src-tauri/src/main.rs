// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::{engine::general_purpose, Engine as _};
use lazy_static::lazy_static;
use log::{info, LevelFilter};
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
use tauri_plugin_log::{LogTarget, TimezoneStrategy};
use tungstenite::http::Request;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::stream::MaybeTlsStream;

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
fn create_lobby() {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let port = LCU_PORT.load(std::sync::atomic::Ordering::Relaxed);
    let enc = ENCODED_PASSWORD.lock().unwrap().clone();

    let res = client
        .post(format!("https://127.0.0.1:{}{}", port, "/lol-lobby/v2/lobby").as_str())
        .body("{ \"queueId\": 430, \"isCustom\": false }")
        .header("Authorization", format!("Basic {}", enc).as_str())
        .send();

    if res.is_err() {
        info!("Failed to create lobby: {}", res.err().unwrap());
    }
}

#[tauri::command]
fn set_user_status(status: &str) {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let port = LCU_PORT.load(std::sync::atomic::Ordering::Relaxed);
    let enc = ENCODED_PASSWORD.lock().unwrap().clone();

    let res = client
        .get(format!("https://127.0.0.1:{}{}", port, "/lol-chat/v1/me").as_str())
        .header("Authorization", format!("Basic {}", enc).as_str())
        .send();

    if res.is_ok() {
        let res = res.unwrap();

        if res.status().is_success() {
            let mut res = res.json::<Value>().unwrap();

            res["availability"] = Value::String(status.to_string());

            let _ = client
                .put(format!("https://127.0.0.1:{}{}", port, "/lol-chat/v1/me").as_str())
                .header("Authorization", format!("Basic {}", enc).as_str())
                .body(res.to_string())
                .send();
        }
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

fn process_events(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    window: &Window,
    spell_ids: &Vec<i64>,
) -> bool {
    let read = socket.read();

    if read.is_ok() {
        let msg = read.unwrap();

        if IS_USER_READY.load(std::sync::atomic::Ordering::Relaxed) {
            let desserialized = serde_json::from_str::<Vec<Value>>(&msg.to_string());

            if desserialized.is_err() {
                return false;
            }

            let desserialized = desserialized.unwrap();

            if desserialized.index(0).as_u64().unwrap() != 8 {
                return false;
            }

            info!("trying to find");
            if desserialized.index(2).get("uri").unwrap().as_str().unwrap() ==
                "/lol-champ-select/v1/session" &&
                    desserialized
                        .index(2)
                        .get("eventType")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        == "Create"
                    {
                        info!("IN CHAMP SELECT");

                        // [GET MY ACTION ID]
                        let action_id = {
                            let data = desserialized.index(2).get("data").unwrap();
                            let my_cell_id = data.get("localPlayerCellId").unwrap().as_u64().unwrap();
                            data.get("actions").unwrap()[0]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|a| a.get("actorCellId").unwrap().as_u64().unwrap() == my_cell_id)
                                .next()
                                .unwrap()
                                .get("id")
                                .unwrap()
                                .as_u64()
                                .unwrap()
                        };
        
                        // [SELECT RANDOM CHAMPION]
                        'champion_loop: loop {
                            info!("champion loop");
        
                            socket
                                .send(tungstenite::Message::Text(format!(
                                    "[2, \"bundolrequest\", \"GET /lol-champ-select/v1/pickable-champion-ids\"]"
                                )))
                                .unwrap();
        
                            let pickable_champions = loop {
                                let read = socket.read();
                                let msg = read.unwrap();
        
                                let desserialized =
                                    serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();
        
                                if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                                    if desserialized.index(0).as_u64().unwrap() == 3 {
                                        break desserialized.index(2).clone().as_array().unwrap().clone();
                                    }
                                } else {
                                    let broke = process_events(socket, window, spell_ids);
                                    if broke {
                                        return broke;
                                    }
                                }
                            };
        
                            let random_champ = pickable_champions
                                [rand::random::<usize>() % pickable_champions.len()]
                            .as_u64()
                            .unwrap();
        
                            // pick champion
                            socket
                                .send(tungstenite::Message::Text(
                                    format!("[2, \"bundolrequest\", \"PATCH /lol-champ-select/v1/session/actions/{}\", {{ \"championId\": {}, \"completed\": true }}]",
                                    action_id,
                                    random_champ
                                )
                                ))
                                .unwrap();
        
                            loop {
                                let read = socket.read();
                                let msg = read.unwrap();
        
                                let desserialized =
                                    serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();
        
                                if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                                    if desserialized.index(0).as_u64().unwrap() == 4 {
                                        println!(
                                            "FAILED TO PICK CHAMPION: {}",
                                            desserialized.index(3).to_string()
                                        );
                                        break 'champion_loop;
                                    }
        
                                    break 'champion_loop;
                                } else {
                                    let broke = process_events(socket, window, spell_ids);
                                    if broke {
                                        return broke;
                                    }
                                }
                            }
                        }
        
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
                                    if desserialized.index(0).as_u64().unwrap() == 4 {
                                        info!(
                                            "FAILED TO GET SPELLS: {}",
                                            desserialized.index(3).to_string()
                                        );
                                    }
        
                                    break desserialized
                                        .index(2)
                                        .get("spells")
                                        .unwrap()
                                        .as_array()
                                        .unwrap()
                                        .clone();
                                } else {
                                    let broke = process_events(socket, window, spell_ids);
                                    if broke {
                                        return broke;
                                    }
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
                                format!("[2, \"bundolrequest\", \"PATCH /lol-champ-select/v1/session/my-selection\", {{ \"spell1Id\": {}, \"spell2Id\": {} }}]",
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
                                if desserialized.index(0).as_u64().unwrap() == 4 {
                                    info!(
                                        "FAILED TO PATCH SUMMONER SPELLS: {}",
                                        desserialized.index(3).to_string()
                                    );
                                }
        
                                break;
                            } else {
                                let broke = process_events(socket, window, spell_ids);
                                if broke {
                                    return broke;
                                }
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
                                    if desserialized.index(0).as_u64().unwrap() == 4 {
                                        info!(
                                            "FAILED TO GET RUNES: {}",
                                            desserialized.index(3).to_string()
                                        );
                                    }
        
                                    break desserialized.index(2).clone();
                                } else {
                                    let broke = process_events(socket, window, spell_ids);
                                    if broke {
                                        return broke;
                                    }
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
        
                            let mut sub_slots_read = 0;
                            let random_slot_to_ignore = rand::random::<usize>() % 3 + 1;
        
                            for slot in slots {
                                if slot.get("type").unwrap().as_str().unwrap() == "kKeyStone" {
                                    continue;
                                }
        
                                if slot.get("type").unwrap().as_str().unwrap() == "kMixedRegularSplashable"
                                {
                                    sub_slots_read += 1;
                                    if sub_slots_read == random_slot_to_ignore {
                                        continue;
                                    }
                                }
        
                                let perks = slot.get("perks").unwrap().as_array().unwrap();
        
                                let random_perk_index = rand::random::<usize>() % perks.len();
                                let perk_id = perks.index(random_perk_index);
        
                                selected_perks.push(perk_id.as_i64().unwrap());
                            }
        
                            info!("Selected perks: {:?}", selected_perks);
        
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
                                "[2, \"bundolrequest\", \"DELETE /lol-perks/v1/pages\"]".to_string(),
                            ))
                            .unwrap();
        
                        loop {
                            let read = socket.read();
                            let msg = read.unwrap();
        
                            let desserialized =
                                serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();
        
                            if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
                                if desserialized.index(0).as_u64().unwrap() == 4 {
                                    info!(
                                        "FAILED TO DELETE RUNES: {}",
                                        desserialized.index(3).to_string()
                                    );
                                }
        
                                break;
                            } else {
                                let broke = process_events(socket, window, spell_ids);
                                if broke {
                                    return broke;
                                }
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
                                if desserialized.index(0).as_u64().unwrap() == 4 {
                                    info!(
                                        "FAILED TO CHANGE RUNES: {}",
                                        desserialized.index(3).to_string()
                                    );
                                }
                                break;
                            } else {
                                let broke = process_events(socket, window, spell_ids);
                                if broke {
                                    return broke;
                                }
                            }
                        }
                    }
        }

        window.emit("lcu-message", Some(msg.to_string())).unwrap();

        if msg.is_close() {
            return true;
        }
    };
    return false;
}

fn find_league_lockfile() -> String {
    let mut file: File = loop {
        let file = File::open("C:/Riot Games/League of Legends/lockfile");

        if file.is_ok() {
            info!("Found lockfile");
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

    info!("Connected to LCU");
    IS_CONNECTED_TO_LCU.store(true, std::sync::atomic::Ordering::Relaxed);

    window.emit("lcu-connected", Option::<()>::None).unwrap();

    socket
        .send(tungstenite::Message::Text(
            "[2, \"bundolrequest\", \"GET /lol-settings/v2/ready\"]".to_string(),
        ))
        .unwrap();

    let ready = loop {
        let read = socket.read();
        let msg = read.unwrap();

        let desserialized = serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

        if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
            if desserialized.index(0).as_u64().unwrap() == 3 {
                break desserialized.index(2).as_bool().unwrap();
            }

            println!("{}", desserialized.index(2).to_string());
            break false;
        } else {
            let broke = process_events(&mut socket, window, &spell_ids);
            if broke {
                return;
            }
        }
    };

    info!("Ready: {}", ready);

    if !ready {
        socket
            .send(tungstenite::Message::Text(
                "[5, \"OnJsonApiEvent_lol-settings_v2_ready\"]".to_string(),
            ))
            .unwrap();

        loop {
            let read = socket.read();

            if read.is_ok() {
                let msg: String = read.unwrap().to_string();

                if msg == "" {
                    continue;
                }

                info!("Ready: {}", msg);
                break;
            }
        }
    }

    socket
        .send(tungstenite::Message::Text(
            "[2, \"bundolrequest\", \"GET /lol-summoner/v1/current-summoner\"]".to_string(),
        ))
        .unwrap();

    loop {
        let read = socket.read();
        let msg = read.unwrap();

        let desserialized = serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

        if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
            if desserialized.index(0).as_u64().unwrap() == 3 {
                let summoner_name = desserialized
                    .index(2)
                    .get("displayName")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .clone();
                let summoner_id = desserialized
                    .index(2)
                    .get("summonerId")
                    .unwrap()
                    .to_string()
                    .clone();

                SUMMONER_NAME.lock().unwrap().clear();
                SUMMONER_NAME.lock().unwrap().push_str(&summoner_name);

                SUMMONER_ID.lock().unwrap().clear();
                SUMMONER_ID.lock().unwrap().push_str(&summoner_id);

                window
                    .emit("lcu-summoner-name", Some(summoner_name))
                    .unwrap();
            }

            println!("{}", desserialized.index(2).to_string());
            break;
        } else {
            let broke = process_events(&mut socket, window, &spell_ids);
            if broke {
                return;
            }
        }
    }

    socket
        .send(tungstenite::Message::Text(
            "[2, \"bundolrequest\", \"GET /lol-lobby/v2/lobby\"]".to_string(),
        ))
        .unwrap();

    loop {
        let read = socket.read();
        let msg = read.unwrap();

        let desserialized = serde_json::from_str::<Vec<Value>>(&msg.to_string()).unwrap();

        if desserialized.index(1).as_str().unwrap() == "bundolrequest" {
            if desserialized.index(0).as_u64().unwrap() == 4 {
                info!(
                    "FAILED TO GET CURRENT LOBBY: {}",
                    desserialized.index(3).to_string()
                );
            } else {
                let members = desserialized.index(2).get("members").unwrap().to_string();
                window.emit("lobby-members", Some(members)).unwrap();
            }

            break;
        } else {
            let broke = process_events(&mut socket, window, &spell_ids);
            if broke {
                return;
            }
        }
    }

    let sub_events = vec![
        "OnJsonApiEvent_lol-lobby_v2_lobby",
        "OnJsonApiEvent_lol-summoner_v1_current-summoner",
        "OnJsonApiEvent_lol-champ-select_v1_session",
        "OnJsonApiEvent_lol-chat_v1_me",
    ];

    for event in sub_events {
        socket
            .send(tungstenite::Message::Text(
                format!("[5, \"{}\"]", event).to_string(),
            ))
            .unwrap();
    }

    loop {
        let broke = process_events(&mut socket, window, &spell_ids);
        if broke {
            info!("Connection closed");
            break;
        }
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
                    info!("Starting league watcher");

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
                    info!("Connection closed");
                }
            });

            Ok(())
        })
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(LevelFilter::Info)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .targets([
                    LogTarget::Stdout,
                    LogTarget::Folder(
                        tauri::api::path::app_log_dir(&tauri::Config::default()).unwrap(),
                    ),
                ])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            is_connected_to_lcu,
            lcu_summoner_name,
            get_pickable_champions,
            ready,
            create_lobby,
            set_user_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
