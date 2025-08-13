use clap::{Arg, Command as ClapCommand};
use fuser::MountOption;
use log::{error, info};
use remote_fs::RemoteFsClient;
use std::{fs::create_dir_all, io::ErrorKind};
use daemonize::Daemonize;
use colored::*;

fn main() {
    let matches = ClapCommand::new("remote-fs-client")
        .version("0.1.0")
        .author("Davide Carletto & Michele Carena")
        .arg(
            Arg::new("mount-point")
                .long("mount-point")
                .value_name("MOUNT_POINT")
                .default_value("/tmp/remote-fs")
                .help("Mount FUSE at given path"),
        )
        .arg(
            Arg::new("daemon")
                .long("daemon")
                .action(clap::ArgAction::SetTrue)   
                .help("Run the client as a daemon in background")   
        )
        .get_matches();

    let mountpoint: String = matches
        .get_one::<String>("mount-point")
        .unwrap()
        .to_string();

    let run_daemon = matches.get_flag("daemon");

    // Configura il logging con fern
    if run_daemon {
        // Per daemon: log su file CON colori (forzati)
        colored::control::set_override(true); // Forza i colori anche su file
        
        fern::Dispatch::new()
            .format(|out, message, record| {
                let level_color = match record.level() {
                    log::Level::Error => "ERROR".red().bold(),
                    log::Level::Warn => "WARN".yellow().bold(),
                    log::Level::Info => "INFO".green().bold(),
                    log::Level::Debug => "DEBUG".blue().bold(),
                    log::Level::Trace => "TRACE".magenta().bold(),
                };
                
                out.finish(format_args!(
                    "{}[{}][{}] {}",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string().cyan(),
                    record.target().dimmed(),
                    level_color,
                    message
                ))
            })
            .level(log::LevelFilter::Info)
            .chain(std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)  // Svuota il file ad ogni avvio, togliere una volta finito lo sviluppo 
                .open("/tmp/remote-fs-client.log")
                .unwrap())
            .apply()
            .unwrap();

        let daemonize = Daemonize::new()
            .pid_file("/tmp/remote-fs-client.pid") 
            .working_directory("/"); 

        match daemonize.start() {
            Ok(_) => info!("Daemon avviato con successo"),
            Err(e) => {
                error!("Errore nel daemonizzare: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Per modalità normale: log sulla console con colori
        fern::Dispatch::new()
            .format(|out, message, record| {
                let level_color = match record.level() {
                    log::Level::Error => "ERROR".red().bold(),
                    log::Level::Warn => "WARN".yellow().bold(),
                    log::Level::Info => "INFO".green().bold(),
                    log::Level::Debug => "DEBUG".blue().bold(),
                    log::Level::Trace => "TRACE".magenta().bold(),
                };
                
                out.finish(format_args!(
                    "{}[{}][{}] {}",
                    chrono::Local::now().format("%H:%M:%S").to_string().cyan(),
                    record.target().dimmed(),
                    level_color,
                    message
                ))
            })
            .level(log::LevelFilter::Debug) //cambiare a info una volta finito
            .chain(std::io::stdout())
            .apply()
            .unwrap();
    }

    create_dir_all(&mountpoint).unwrap();

    info!("Avvio client per mountpoint: {}", mountpoint);
    
    let client = RemoteFsClient::new("http://localhost:3000".into());

    let options = vec![
        MountOption::FSName("remote-fs".to_string()),
        MountOption::AutoUnmount,
        MountOption::AllowOther,
    ];

    let result = fuser::mount2(client, mountpoint, &options);

    if let Err(e) = result {
        if e.kind() == ErrorKind::PermissionDenied {
            error!("{e}");
            std::process::exit(2);
        }
    }
}
