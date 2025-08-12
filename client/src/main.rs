use clap::{Arg, Command as ClapCommand};
use log::error;
use std::{fs::create_dir_all, io::ErrorKind};
use fuser::MountOption;
use remote_fs::RemoteFsClient;
fn main() {
    let matches = ClapCommand::new("remote-fs-client")
        .version("0.1.0")
        .author("Davide Carletto")
        .arg(
            Arg::new("mount-point")
                .long("mount-point")
                .value_name("MOUNT_POINT")
                .default_value("/tmp/remote-fs")
                .help("Mount FUSE at given path"),
        )
        .get_matches();

    let mountpoint: String = matches
        .get_one::<String>("mount-point")
        .unwrap()
        .to_string();

    create_dir_all(&mountpoint).unwrap();

    let client = RemoteFsClient::new("http://localhost:3000".into());

    let options = vec![MountOption::FSName("remote-fs".to_string()), MountOption::AutoUnmount];
    
    let result = fuser::mount2(client, mountpoint, &options);

    if let Err(e) = result {
        if e.kind() == ErrorKind::PermissionDenied {
            error!("{e}");
            std::process::exit(2);
        }
    }
}
