use clap::{Command as ClapCommand};
use log::{info, warn, error};
use colored::*;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;

fn setup_logging(log_to_file: Option<String>, level: log::LevelFilter) {
    // Clona la stringa dentro la closure per evitare problemi di lifetime
    let log_to_file_clone = log_to_file.clone();

    let base_dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            let level_color = match record.level() {
                log::Level::Error => "ERROR".red().bold(),
                log::Level::Warn => "WARN".yellow().bold(),
                log::Level::Info => "INFO".green().bold(),
                log::Level::Debug => "DEBUG".blue().bold(),
                log::Level::Trace => "TRACE".magenta().bold(),
            };
            let time_fmt = if log_to_file_clone.is_some() {
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                chrono::Local::now().format("%H:%M:%S").to_string()
            };
            out.finish(format_args!(
                "{}[{}][{}] {}",
                time_fmt.cyan(),
                record.target().dimmed(),
                level_color,
                message
            ))
        })
        .level(level);

    let dispatch = if let Some(file_path) = log_to_file {
        base_dispatch.chain(std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)
            .unwrap())
    } else {
        base_dispatch.chain(std::io::stdout())
    };

    dispatch.apply().unwrap();
}


fn setup_graceful_shutdown() -> Arc<AtomicBool> {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = shutdown_flag.clone();

    ctrlc::set_handler(move || {
        warn!("Ricevuto segnale di shutdown (CTRL+C)");
        shutdown_flag_clone.store(true, Ordering::SeqCst);
    }).expect("Errore nel setup del signal handler");

    shutdown_flag
}

fn wait_for_shutdown(shutdown_flag: Arc<AtomicBool>) {
    while !shutdown_flag.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(500));
    }
}

fn parse_common_args(cmd: &mut ClapCommand) -> (String,) {
    let matches = cmd.clone().get_matches();

    let api_url: String = matches
        .get_one::<String>("api-url")
        .unwrap()
        .to_string();

    (api_url,)
}

#[cfg(target_os = "linux")]
mod linux_main {
    use super::*;
    use clap::{Arg, Command as ClapCommand};
    use fuser::MountOption;
    use std::{fs::create_dir_all, io::ErrorKind};
    use daemonize::Daemonize;
    use remote_fs::FuseRemoteFs;

    pub fn run() {
        let mut cmd = ClapCommand::new("remote-fs-client")
            .version("0.1.0")
            .author("Davide Carletto & Michele Carena")
            .about("Client filesystem remoto modulare con architettura backend")
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
                    .help("Run the client as a daemon in background"),
            )
            .arg(
                Arg::new("api-url")
                    .long("api-url")
                    .value_name("URL")
                    .default_value("http://localhost:3000")
                    .help("URL del server RemoteFS"),
            );

        let (api_url,) = super::parse_common_args(&mut cmd);

        let matches = cmd.get_matches();

        let mountpoint: String = matches
            .get_one::<String>("mount-point").unwrap()
            .to_string();

        let run_daemon = matches.get_flag("daemon");

        let log_file = run_daemon.then(|| "/tmp/remote-fs-client.log".to_string());
        super::setup_logging(log_file, if run_daemon { log::LevelFilter::Info } else { log::LevelFilter::Debug });

        if run_daemon {
            daemonize_process();
        }

        let shutdown_flag = super::setup_graceful_shutdown();

        create_dir_all(&mountpoint).unwrap();

        let fs = FuseRemoteFs::new(api_url);
        info!("Cache configurata con successo");

        let options = vec![
            MountOption::FSName("remote-fs".to_string()),
            MountOption::AutoUnmount,
            MountOption::AllowOther,
        ];

        let mountpoint_clone = mountpoint.clone();
        let mount_handle = std::thread::spawn(move || {
            let result = fuser::mount2(fs, mountpoint_clone, &options);
            if let Err(e) = result {
                if e.kind() == ErrorKind::PermissionDenied {
                    error!("Permessi insufficienti: {}", e);
                    std::process::exit(2);
                } else {
                    error!("Errore mount: {}", e);
                    std::process::exit(1);
                }
            }
        });

        info!("Filesystem montato con successo! Premi CTRL+C per uscire.");

        super::wait_for_shutdown(shutdown_flag);

        perform_graceful_shutdown(&mountpoint, run_daemon, mount_handle);
    }

    fn daemonize_process() {
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
    }

    fn perform_graceful_shutdown(
        mountpoint: &str,
        daemon_mode: bool,
        mount_handle: std::thread::JoinHandle<()>,
    ) {

        let unmount_result = std::process::Command::new("fusermount")
            .args(["-u", mountpoint])
            .output();

        match unmount_result {
            Ok(output) if output.status.success() => {
                info!("Filesystem smontato con successo");
            }
            Ok(output) => {
                error!("Unmount fallito: {}", String::from_utf8_lossy(&output.stderr));
            }
            Err(e) => error!("Errore comando unmount: {}", e),
        }

        if daemon_mode {
            if let Err(e) = std::fs::remove_file("/tmp/remote-fs-client.pid") {
                error!("Errore rimozione PID file: {}", e);
            } else {
                info!("PID file rimosso");
            }
        }

        let join_result = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            mount_handle.join()
        })
        .join();

        match join_result {
            Ok(_) => info!("Mount thread terminato"),
            Err(_) => warn!("Mount thread timeout"),
        }

        info!("Graceful shutdown completato!");
        std::process::exit(0);
    }
}

#[cfg(target_os = "windows")]
mod windows_main {
    use super::*;
    use clap::{Arg, Command as ClapCommand};
    use log::error;
    use remote_fs::backends::WinFspRemoteFs;

    pub fn run() {
        let mut cmd = ClapCommand::new("remote-fs-client")
            .version("0.1.0")
            .author("Davide Carletto & Michele Carena")
            .about("Client filesystem remoto Windows con WinFSP")
            .arg(
                Arg::new("drive-letter")
                    .long("drive-letter")
                    .value_name("DRIVE")
                    .default_value("R:")
                    .help("Lettera del drive per il mount Windows"),
            )
            .arg(
                Arg::new("api-url")
                    .long("api-url")
                    .value_name("URL")
                    .default_value("http://localhost:3000")
                    .help("URL del server RemoteFS"),
            );

        let (api_url,) = super::parse_common_args(&mut cmd);

        let matches = cmd.get_matches();

        let drive_letter: String = matches
            .get_one::<String>("drive-letter")
            .unwrap()
            .to_string();

        super::setup_logging(None, log::LevelFilter::Debug);

        let shutdown_flag = super::setup_graceful_shutdown();

        let fs = WinFspRemoteFs::new(api_url, drive_letter.clone());

        info!("Filesystem Windows pronto! Premi CTRL+C per uscire");

        super::wait_for_shutdown(shutdown_flag);

        perform_graceful_shutdown(fs);
    }

    fn perform_graceful_shutdown(fs: WinFspRemoteFs) {

        if let Err(e) = fs.unmount() {
            error!("Errore unmount Windows: {}", e);
        }

        info!("Graceful shutdown Windows completato!");
        std::process::exit(0);
    }
}

#[cfg(target_os = "linux")]
fn main() {
    linux_main::run();
}

#[cfg(target_os = "windows")]
fn main() {
    windows_main::run();
}
