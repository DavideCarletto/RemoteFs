use remote_fs::RemoteFs;
use std::env;
use std::path::Path;

fn main() {
    env_logger::init();
    
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <server_url> <mountpoint>", args[0]);
        eprintln!("Example: {} http://localhost:8080 /mnt/remote-fs", args[0]);
        std::process::exit(1);
    }

    let server_url = &args[1];
    let mountpoint = &args[2];

    println!("Remote FS: Connecting to server at {}", server_url);
    println!("Remote FS: Mounting at {}", mountpoint);

    let filesystem = RemoteFs::new(server_url.clone());
    
    let options = vec![
        fuser::MountOption::RW,
        fuser::MountOption::FSName("remote-fs".to_string()),
    ];

    match fuser::mount2(filesystem, mountpoint, &options) {
        Ok(()) => {
            println!("Filesystem unmounted successfully");
        }
        Err(e) => {
            eprintln!("Failed to mount filesystem: {}", e);
            std::process::exit(1);
        }
    }
}
