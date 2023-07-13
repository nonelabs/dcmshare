mod matrix;
mod studies;
mod dcm;
mod ui;
use dcm::storescp::receive;
use dicom_core::chrono::Duration;
use studies::store::{StudyStore, Study};
use matrix::client::process_messages;
use tokio::time::sleep;
use tokio::task;
use std::thread;
use std::sync::Arc;
use tokio::sync::mpsc;

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use clap::Parser;
use tracing::{error, info, Level};
use dicom_dictionary_std::tags;
use dicom_object::open_file;
use iced::{Application, Settings};
use microkv::MicroKV;
use snafu::{ResultExt, Whatever};
use url::Url;
use structopt::StructOpt;
use tokio::sync::mpsc::Sender;
use ui::activity::SetupUI;


#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Args{
    #[structopt(long = "ae_title", default_value = "DCMSHARE")]
    ae_title: String,
    #[structopt(long = "run_dir", default_value = ".dcmshare")]
    run_dir: PathBuf,
    #[structopt(long = "port", default_value = "11111")]
    port: u16,
    #[structopt(long = "db_dir", default_value = "")]
    db_dir: PathBuf,
    #[structopt(long = "db_password", default_value = "correcthorsestablebattery")]
    db_password: String,
    #[structopt(long = "matrix_user", default_value = "@joker:serverella")]
    user: String,
    #[structopt(long = "matrix_password", default_value = "joker")]
    user_password: String,
}

// impl Clone for Args {
//     fn clone(&self) -> Self {
//         Args {
//             ae_title: self.ae_title.clone(),
//             run_dir: self.run_dir.clone(),
//             db_dir: self.db_dir.clone(),
//             port: self.port,
//             db_password: self.db_password.clone(),
//             user: self.user.clone(),
//             user_password: self.user_password.clone(),
//         }
//     }
// }

async fn handle_connection(scu_stream: TcpStream, tx: Sender<Study>) {
    let study_store = StudyStore {
        run_dir: PathBuf::from(".dcmshare"),
        db: MicroKV::open_with_base_path("studies", PathBuf::from(""))
            .expect("Failed to create MicroKV from a stored file or create MicroKV for this file")
            .set_auto_commit(true)
            .with_pwd_clear("correcthorsestaplebattery".to_string())
    };
    match receive(scu_stream, "DCMSHARE".to_string(), PathBuf::from(".dcmshare")) {
        Ok(sop_instance_uid) => {
            println!("receiving study");
            let study = study_store.add_series(&sop_instance_uid).await.unwrap();
            let res = tx.send(study).await;
            match res {
                Err(E) => println!("{}",E.to_string()),
                Ok(T) => println!("sent")
            }
        },
        Err(E) => error!("Something went wrong"),
    }
}

async fn listen(tx: Sender<Study>) {
    let ae_title = String::from("DCMSHARE");
    let listen_addr = SocketAddrV4::new(Ipv4Addr::from(0), 11111);
    let listener = TcpListener::bind(listen_addr).unwrap();
    info!(
        "{} listening on: tcp://{}",
        ae_title, listen_addr
    );
    for stream in listener.incoming() {
        println!("Incoming");
        handle_connection(stream.unwrap(), tx.clone()).await;
    }
}

#[tokio::main]
async fn main() {

    let (tx, mut rx) = mpsc::channel::<Study>(100);
    let args = Args::from_args();

    let mut settings = Settings::default();
    settings.window.size = (500,700);
    SetupUI::run(settings);

    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish(),
    )
        .unwrap_or_else(|e| {
            eprintln!(
                "Could not set up global logger: {}",
                snafu::Report::from_error(e)
            );
        });

    std::fs::create_dir_all(PathBuf::from(".dcmshare")).unwrap_or_else(|e| {
        error!("Could not create output directory: {}", e);
        std::process::exit(-2);
    });

    tokio::spawn( process_messages("https://nonelabs.com:4327","radioshare", "bHuj/?>.2334", rx) );
    listen(tx).await;

}
