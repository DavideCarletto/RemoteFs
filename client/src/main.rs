use clap::{Arg, Command as ClapCommand};
use colored::*;
use log::{error, info, warn};
use remote_fs::CacheInvalidationStrategy;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

fn setup_logging(log_to_file: Option<String>, level: log::LevelFilter) {
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
        base_dispatch.chain(
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(file_path)
                .unwrap(),
        )
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
    })
    .expect("Errore nel setup del signal handler");

    shutdown_flag
}

fn parse_common_args(cmd: &mut ClapCommand) -> (String, CacheInvalidationStrategy) {
    let matches = cmd.clone().get_matches();

    let api_url: String = matches.get_one::<String>("api-url").unwrap().to_string();
    let cache_inv_strategy = matches
        .get_one::<String>("cache-inv")
        .map(|s| {
            parse_cache_strategy(s)
                .unwrap_or_else(|e| panic!("Invalid cache strategy '{}': {}", s, e))
        })
        .unwrap_or_else(|| CacheInvalidationStrategy::TTL(Duration::from_secs(60)));

    (api_url, cache_inv_strategy)
}

fn parse_cache_strategy(s: &str) -> Result<CacheInvalidationStrategy, String> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.as_slice() {
        ["ttl", secs] => {
            let secs: u64 = secs
                .parse()
                .map_err(|_| "TTL must be an integer (seconds)".to_string())?;
            Ok(CacheInvalidationStrategy::TTL(Duration::from_secs(secs)))
        }
        ["lru", size] => {
            let size: usize = size
                .parse()
                .map_err(|_| "LRU size must be an integer".to_string())?;
            Ok(CacheInvalidationStrategy::LRU(size))
        }
        _ => Err("Expected format 'ttl:<seconds>' or 'lru:<entries>'".to_string()),
    }
}

fn create_base_command(description: &str) -> ClapCommand {
    ClapCommand::new("remote-fs-client")
        .version("0.1.0")
        .author("Davide Carletto & Michele Carena")
        .about(&format!("Client filesystem remoto {}", description))
        .arg(
            Arg::new("api-url")
                .long("api-url")
                .value_name("URL")
                .default_value("http://localhost:3000")
                .help("URL del server RemoteFS"),
        )
        .arg(
            Arg::new("cache-inv")
                .long("cache-inv")
                .value_name("STRATEGY")
                .default_value("ttl:60")
                .help("Cache invalidation strategy: ttl:SECS, always, never"),
        )
        .arg(
            Arg::new("foreground")
                .long("foreground")
                .short('f')
                .action(clap::ArgAction::SetTrue)
                .help("Run in foreground mode (default is daemon mode)"),
        )
        .arg(
            Arg::new("daemon-child")
                .long("daemon-child")
                .action(clap::ArgAction::SetTrue)
                .help("Internal flag - indicates this is the daemon child process")
                .hide(true),
        )
}

fn handle_mount_error(error_msg: &str, exit_code: i32) -> ! {
    error!("{}", error_msg);
    std::process::exit(exit_code);
}

fn log_startup_info(platform: &str, target: &str) {
    info!(
        "=== Remote-FS Client {} - Montaggio su: {} - Premi CTRL+C per uscire ===",
        platform, target
    );
}

fn log_shutdown_info(platform: &str) {
    info!("=== Shutdown {} ===", platform);
}

fn final_cleanup_and_exit(success: bool) {
    if success {
        info!("Graceful shutdown completato!");
        std::process::exit(0);
    } else {
        error!("Shutdown con errori");
        std::process::exit(1);
    }
}

fn health_check_server(api_url: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Errore creazione client HTTP: {}", e))?;

    let health_url = format!("{}/health", api_url);

    match client.get(&health_url).send() {
        Ok(resp) if resp.status().is_success() => {
            info!("Server disponibile e raggiungibile");
            Ok(())
        }
        Ok(resp) => Err(format!("Server restituisce errore: {}", resp.status())),
        Err(e) if e.is_timeout() => Err("Timeout connessione al server (5s)".to_string()),
        Err(e) if e.is_connect() => Err(format!("Impossibile connettersi al server: {}", e)),
        Err(e) => Err(format!("Errore di rete: {}", e)),
    }
}

#[cfg(target_os = "linux")]
mod linux_main {
    use super::*;
    use fuser::MountOption;
    use remote_fs::FuseRemoteFs;
    use std::{fs::create_dir_all, io::ErrorKind};

    fn daemonize_process() {
        use daemonize::Daemonize;

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
        log_shutdown_info("Linux");

        let unmount_result = std::process::Command::new("fusermount")
            .args(["-u", mountpoint])
            .output();

        match unmount_result {
            Ok(output) if output.status.success() => {
                info!("Filesystem smontato con successo");
            }
            Ok(output) => {
                error!(
                    "Unmount fallito: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
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
            info!("Waiting for mount thread to terminate...");
            std::thread::sleep(Duration::from_secs(3));
            info!("Attempting to join mount thread...");
            mount_handle.join()
        })
        .join();

        match join_result {
            Ok(Ok(())) => info!("Mount thread terminato con successo"),
            Ok(Err(_)) => warn!("Mount thread terminato con errore"),
            Err(_) => {
                warn!("Mount thread timeout dopo 3 secondi - forzando l'uscita");
                info!("Graceful shutdown completato - terminazione forzata");
                std::process::exit(0);
            }
        }

        final_cleanup_and_exit(true);
    }

    pub fn run() {
        let mut cmd = create_base_command("Linux con FUSE")
            .arg(
                Arg::new("mount-point")
                    .long("mount-point")
                    .value_name("MOUNT_POINT")
                    .default_value("/tmp/remote-fs")
                    .help("Mount FUSE at given path"),
            );

        let (api_url, cache_inv_strategy) = super::parse_common_args(&mut cmd);
        let matches = cmd.get_matches();

        let mountpoint: String = matches
            .get_one::<String>("mount-point")
            .unwrap()
            .to_string();

        let run_foreground = matches.get_flag("foreground");
        let run_daemon = !run_foreground; // Inverso: default è daemon mode

        let log_file = run_daemon.then(|| "/tmp/remote-fs-client.log".to_string());
        super::setup_logging(log_file, log::LevelFilter::Info);

        if run_daemon {
            info!("Avvio in modalità daemon (usa --foreground per debug)");
            daemonize_process();
            info!("Processo daemon avviato");
        } else {
            info!("Avvio in modalità foreground (debug mode)");
        }

        let shutdown_flag = setup_graceful_shutdown();

        if let Err(e) = super::health_check_server(&api_url) {
            error!("Health check fallito: {}", e);
            std::process::exit(1);
        }

        super::log_startup_info("Linux FUSE", &mountpoint);

        create_dir_all(&mountpoint).unwrap();
        let fs = FuseRemoteFs::new(api_url, cache_inv_strategy);
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
                let error_msg = if e.kind() == ErrorKind::PermissionDenied {
                    format!("Permessi insufficienti: {}", e)
                } else {
                    format!("Errore mount: {}", e)
                };
                super::handle_mount_error(
                    &error_msg,
                    if e.kind() == ErrorKind::PermissionDenied {
                        2
                    } else {
                        1
                    },
                );
            }
        });

        info!("Filesystem montato con successo su {}!", mountpoint);

        let mut mount_finished = false;

        loop {
            if !mount_finished && mount_handle.is_finished() {
                info!("Mount thread terminato - filesystem smontato esternamente");
                mount_finished = true;
                break;
            }

            if shutdown_flag.load(Ordering::Relaxed) {
                break;
            }

            std::thread::sleep(Duration::from_millis(100));
        }

        if mount_finished {
            info!("Terminazione automatica dopo smontaggio esterno");
            info!("Graceful shutdown completato!");
            std::process::exit(0);
        } else {
            perform_graceful_shutdown(&mountpoint, run_daemon, mount_handle);
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_main {
    use std::thread;
    use super::*;
    use remote_fs::backends::WinFspRemoteFs;

    fn wait_for_shutdown(shutdown_flag: Arc<AtomicBool>) {
        while !shutdown_flag.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(500));
        }
    }

    fn hide_console_window() {
        #[cfg(target_os = "windows")]
        {
            extern "system" {
                fn GetConsoleWindow() -> *mut std::ffi::c_void;
                fn ShowWindow(hwnd: *mut std::ffi::c_void, nCmdShow: i32) -> i32;
            }
            const SW_HIDE: i32 = 0;
            
            unsafe {
                let console_window = GetConsoleWindow();
                if !console_window.is_null() {
                    ShowWindow(console_window, SW_HIDE);
                }
            }
        }
    }

    fn perform_graceful_shutdown(daemon_mode: bool) {
        super::log_shutdown_info("Windows");

        if daemon_mode {
            info!("Cleanup PID file Windows");
            if let Err(e) = std::fs::remove_file("C:\\temp\\remote-fs-client.pid") {
                error!("Errore rimozione PID file: {}", e);
            } else {
                info!("PID file Windows rimosso");
            }
        }

        info!("Cleanup Windows completato");

        super::final_cleanup_and_exit(true);
    }

    pub fn run() {
        let mut cmd = create_base_command("Windows con WinFSP").arg(
            Arg::new("drive-letter")
                .long("drive-letter")
                .value_name("DRIVE")
                .default_value("R:")
                .help("Lettera del drive per il mount Windows"),
        );

        let (api_url, cache_inv_strategy) = super::parse_common_args(&mut cmd);
        let matches = cmd.get_matches();

        let drive_letter: String = matches
            .get_one::<String>("drive-letter")
            .unwrap()
            .to_string();

        let run_foreground = matches.get_flag("foreground");
        let is_daemon_child = matches.get_flag("daemon-child");
        let run_daemon = !run_foreground && !is_daemon_child;

        if run_daemon {
            println!("Avvio daemon in background...");
            
            let current_exe = std::env::current_exe().expect("Cannot get current executable path");
            let mut child_cmd = std::process::Command::new(&current_exe);
            
            child_cmd.arg("--daemon-child");
            child_cmd.arg("--api-url").arg(&api_url);
            child_cmd.arg("--drive-letter").arg(&drive_letter);
            
            let cache_arg = matches.get_one::<String>("cache-inv").unwrap();
            if cache_arg != "ttl:60" {
                child_cmd.arg("--cache-inv").arg(cache_arg);
            }
            
            match child_cmd
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    println!("Daemon avviato con PID: {}", child.id());
                    println!("Filesystem in fase di montaggio su {}...", drive_letter);
                    
                    std::thread::sleep(Duration::from_secs(2));
                    
                    println!("✅ Daemon attivo!");
                    println!("📂 Usa: Get-ChildItem {} per accedere ai file", drive_letter);
                    println!("📂 Oppure: cd {} && dir", drive_letter);
                    println!("📋 Log: {:?}", std::env::temp_dir().join("remote-fs-client.log"));
                    println!("🛑 Per fermare: taskkill /f /im remote_fs.exe");
                    
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("Errore nell'avvio del daemon: {}", e);
                    std::process::exit(1);
                }
            }
        }

        let log_file = if is_daemon_child {
            let temp_dir = std::env::temp_dir();
            Some(temp_dir.join("remote-fs-client.log").to_string_lossy().to_string())
        } else {
            None
        };
        super::setup_logging(log_file, log::LevelFilter::Debug);

        if is_daemon_child {
            info!("Processo daemon child avviato");
            
            let temp_dir = std::env::temp_dir();
            let pid_file = temp_dir.join("remote-fs-client.pid");
            if let Err(e) = std::fs::write(&pid_file, std::process::id().to_string()) {
                warn!("Impossibile scrivere PID file {:?}: {}", pid_file, e);
            }
            
            hide_console_window();
        } else {
            info!("Avvio in modalità foreground Windows (debug mode)");
        }

        let shutdown_flag = super::setup_graceful_shutdown();

        if let Err(e) = super::health_check_server(&api_url) {
            error!("Health check fallito: {}", e);
            std::process::exit(1);
        }

        super::log_startup_info("Windows WinFSP", &drive_letter);

        let fs = WinFspRemoteFs::new(api_url, drive_letter.clone(), cache_inv_strategy);

        let shutdown_clone = shutdown_flag.clone();
        let drive_letter_clone = drive_letter.clone();
        let mount_handle = std::thread::spawn(move || {
            if let Err(e) = fs.mount(shutdown_clone) {
                super::handle_mount_error(&format!("Errore mount Windows: {}", e), 1);
            }
        });

        info!("Filesystem montato con successo su {}!", drive_letter_clone);

        wait_for_shutdown(shutdown_flag);

        let join_result = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            mount_handle.join()
        })
        .join();

        match join_result {
            Ok(_) => info!("Mount thread terminato correttamente"),
            Err(_) => warn!("Mount thread timeout durante shutdown"),
        }

        info!("Filesystem Windows terminato!");
        perform_graceful_shutdown(is_daemon_child);
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
