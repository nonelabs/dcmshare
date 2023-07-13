///
///  This is an example showcasing how to build a very simple bot using the
/// matrix-sdk. To try it, you need a rust build setup, then you can run:
/// `cargo run -p example-getting-started -- <homeserver_url> <user> <password>`
///
/// Use a second client to open a DM to your bot or invite them into some room.
/// You should see it automatically join. Then post `!party` to see the client
/// in action.
///
/// Below the code has a lot of inline documentation to help you understand the
/// various parts and what they do
// The imports we need



use std::{env, fs, process::exit};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use dcm::storescu::send;
use anyhow::anyhow;
use iced::futures::TryFutureExt;
use iced::subscription::Recipe;
use tokio::sync::mpsc::channel;

use matrix_sdk::{config::SyncSettings, room::Room, ruma::events::room::{
    member::StrippedRoomMemberEvent,
    message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
}, Client, LoopCtrl};
use s3::{Bucket, Region};
use s3::creds::Credentials;
use sodiumoxide::crypto::secretbox;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::error;
use url::Url;
use crate::dcm;

use crate::studies::store::Study;

pub async fn process_messages(
    homeserver: &str,
    matrix_username: &str,
    matrix_password: &str,
    mut rx: mpsc::Receiver<Study>
) -> anyhow::Result<()> {
    println!("logged in as {matrix_username}");
    let client = Client::builder()
        .homeserver_url(homeserver)
        .handle_refresh_tokens()
        .build()
        .await?;
    let response = client
        .login_username(matrix_username, matrix_password)
        .initial_device_display_name("dcmshare client")
        .request_refresh_token()
        .send()
        .await?;

    let cc = client.clone();
    let thread = tokio::spawn( async move {
        cc.add_event_handler(room_invitation);
        let sync_token = cc.sync_once(SyncSettings::default()).await.unwrap().next_batch;
        cc.add_event_handler(on_room_message);
        let settings = SyncSettings::default().token(sync_token);
        cc.sync(settings).await;
    });
    while let Some(study) = rx.recv().await {
        let sync_token = client.sync_once(SyncSettings::default()).await.unwrap().next_batch;
        let invited_rooms = client.joined_rooms();
        let content = RoomMessageEventContent::text_plain(format!("PatientenID:{}\nName:{}\nGeburtstag:{}\nKey:{}/{}",&study.patient_id,&study.patient_name,&study.patient_birth_date,&study.study_instance_uid_hash,&study.hex_key));
        for joined_id in invited_rooms {
            let room = client.get_joined_room(&joined_id.room_id()).unwrap();
            room.send(content.clone(), None).await.unwrap();
        }
    }
    thread.await;
    Ok(())
}

async fn room_invitation(
    room_member: StrippedRoomMemberEvent,
    client: Client,
    room: Room,
) {
    if room_member.state_key != client.user_id().unwrap() {
        return;
    }
    if let Room::Invited(room) = room {
        tokio::spawn(async move {
            println!("Joining room {}", room.room_id());
            let mut delay = 2;
            while let Err(err) = room.accept_invitation().await {
                eprintln!("Failed to join room {} ({err:?}), retrying in {delay}s", room.room_id());
                sleep(Duration::from_secs(delay)).await;
                delay *= 2;
                if delay > 3600 {
                    eprintln!("Can't join room {} ({err:?})", room.room_id());
                    break;
                }
            }
            println!("Successfully joined room {}", room.room_id());
        });
    }
}

async fn on_room_message(event: OriginalSyncRoomMessageEvent, room: Room) {
    println!("New room message");
    let Room::Joined(room) = room else { return };
    let MessageType::Text(text_content) = event.content.msgtype else { return };
    let sender = event.sender;
    println!("{}",sender.to_string());
    if !sender.to_string().contains("share") && text_content.body.contains("Key:") {
        let start_pattern = "creds:";
        let end_pattern = "/";
        let start_index = text_content.body.find(start_pattern).unwrap() + start_pattern.len();
        let end_index = text_content.body[start_index..].find(end_pattern).unwrap();
        let study_id = &text_content.body[start_index..start_index+end_index];
        let hex_key = &text_content.body[start_index+end_index + end_pattern.len()..];
        let bucket = Bucket::new_with_path_style("studies",Region::Custom {region:"".to_owned(),endpoint: "https://nonelabs.com:9090".to_owned()}, Credentials{
            access_key: Some("minio".to_owned()),
            secret_key: Some("f381hf1h2g70hvq23ubgiu123r".to_owned()),
            expiration: None,
            security_token: None,
            session_token: None,
        }).unwrap();
        let results = bucket.list(study_id.to_string(), None).await;
        let mut dicom_files: Vec<PathBuf> = vec!();
        match results {
            Ok(res) => {
                for r in res {
                    for object in r.contents {
                        let data = bucket.get_object(&object.key).await.unwrap();
                        let file_name = format!("./{}.dat",&object.key);
                        std::fs::create_dir_all(PathBuf::from(study_id)).unwrap_or_else(|e| {
                            error!("Could not create output directory: {}", e);
                            std::process::exit(-2);
                        });
                        let mut file = File::create(&file_name).unwrap();
                        file.write_all(&data.bytes());

                    }
                }
            },
            Err(E) => println!("{}",E.to_string())
        }
        send(dicom_files);
        // // download all files sequentially
        //
        // // download all files
        //
        let content = RoomMessageEventContent::text_plain("Transfering study to workstation...");
        room.send(content, None).await.unwrap();
    }
}

