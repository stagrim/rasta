
use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, stream::SplitSink, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{sync::{Mutex, watch::Receiver}};
use tokio_util::sync::CancellationToken;

use crate::store::store::{Content, PlaylistItem};

#[derive(Deserialize, Clone)]
struct HelloRequest {
    uuid: String,
    hostname: String
}

#[derive(Debug, Serialize)]
pub enum Payload {
    #[serde(rename(serialize = "display"))]
    Display(DisplayPayload),
    #[serde(rename(serialize = "name"))]
    Name(String),
    #[serde(rename(serialize = "pending"))]
    Pending(bool)
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum DisplayPayload {
    #[serde(rename(serialize = "WEBSITE"))]
    Website { data: WebsitePayload },
    // #[serde(rename(serialize = "TEXT"))]
    // Text { data: WebsitePayload },
    // #[serde(rename(serialize = "IMAGE"))]
    // Image { data: WebsitePayload }
}

#[derive(Serialize, Debug, Clone)]
pub struct WebsitePayload {
    pub content: String
}

pub async fn client_connection(socket: WebSocket, who: SocketAddr, mut rx: Receiver<Content>) {
    let (sender, mut reciever) = socket.split();
    let sender = Arc::new(Mutex::new(sender));
    
    let mut heart_beat_handle = heart_beat(sender.clone());

    // Wait for a hellorespone from connected client to get its UUID
    let hello: HelloRequest = loop {
        if let Some(Ok(Message::Text(msg))) = reciever.next().await {
            match serde_json::from_str(&msg) {
                Ok(msg) => break msg,
                Err(_) => println!("{msg:?} was not a HelloRequest")
            };
        }
    };

    let mut client_handle = tokio::spawn(async move {
        let display = loop {
            let display_option = rx.borrow().displays.get(&hello.uuid).and_then(|d| Some(d.clone()));
            match display_option {
                Some(d) => break d,
                None => {
                    println!("No screen defined with UUID {:?}", hello.uuid);
                    let msg = Message::Text(serde_json::to_string(&Payload::Pending(true)).unwrap());
                    sender.lock().await.send(msg).await.unwrap();

                    // Wait until content change, then try again 
                    rx.changed().await.unwrap();
                    continue;
                },
            }
        };
    
        let msg = Message::Text(serde_json::to_string(&Payload::Name(display.name)).unwrap());
        sender.lock().await.send(msg).await.unwrap();

        let mut cancellation_token = CancellationToken::new();
        let mut send_loop_handle = tokio::spawn(send_task(rx.clone(), hello.clone(), sender.clone(), cancellation_token.clone()));

        loop {
            tokio::select! {
                message = rx.changed() => {
                    if message.is_ok() {
                        // TODO: check if playlist has actually changed
                        cancellation_token.cancel();
                        send_loop_handle.await.unwrap();
                        cancellation_token = CancellationToken::new();
                        send_loop_handle = tokio::spawn(send_task(rx.clone(), hello.clone(), sender.clone(), cancellation_token.clone()));
                    }
                },
                _ = &mut send_loop_handle => {
                    println!("Send loop did not exit correctly, exiting send thread");
                    break
                },
            };   
        }
    });

    tokio::select! {
        _ = &mut heart_beat_handle => client_handle.abort(),
        _ = &mut client_handle => heart_beat_handle.abort(),
    };

    println!("Done with {who}!");
}

async fn send_task(rx: Receiver<Content>, hello: HelloRequest, sender: Arc<Mutex<SplitSink<WebSocket, Message>>>, cancellation_token: CancellationToken) {
    let playlist = match get_display_playlist(&rx, &hello.uuid) {
        Some(p) => p,
        None => {
            println!("Error: Display playlist could not be found");
            return
        },
    };

    'outer: loop {
        for item in &playlist {
            let sleep;
            // println!("{item:?}");
            let payload = match item {
                PlaylistItem::Website { name, settings } => {
                    println!("Sending {name:?} website to {:?}", hello.hostname);
                    sleep = settings.duration;
                    DisplayPayload::Website { data: WebsitePayload { content: settings.url.clone() } }
                },
            };

            let msg = Message::Text(serde_json::to_string(&Payload::Display(payload)).unwrap());
            sender.lock().await.send(msg).await.unwrap();
            
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(sleep)) => (),
                _ = cancellation_token.cancelled() => break 'outer,
            };
        }
    };
}

fn get_display_playlist(rx: &Receiver<Content>, uuid: &String) -> Option<Vec<PlaylistItem>> {
    let content = rx.borrow();
    let schedule = &content.displays.get(uuid)?.schedule;
    Some(content.playlists.get(&content.schedules.get(schedule)?.playlist)?.items.clone())
}


fn heart_beat(sender: Arc<Mutex<SplitSink<WebSocket, Message>>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;
            let mut socket = sender.lock().await;
            match socket.send(Message::Ping(vec![])).await {
                Ok(_) => println!("Sent Ping"),
                Err(_) => {
                    println!("Could not ping, fuck");
                    match socket.close().await {
                        Ok(_) => println!("hej"),
                        Err(_) => println!("nej"),
                    }
                    return;
                },
            };
        };
    })
}