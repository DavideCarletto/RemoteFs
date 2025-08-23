use crate::client::RemoteFsClient;
use log::info;

/// Implementazione WinFSP del filesystem remoto
#[cfg(target_os = "windows")]
pub struct WinFspRemoteFs {
    client: RemoteFsClient,
    drive_letter: String,
}

#[cfg(target_os = "windows")]
impl WinFspRemoteFs {
    pub fn new(api_url: String, drive_letter: String) -> Self {
        info!(
            "Inizializzazione WinFspRemoteFs per API: {} su unità {}",
            api_url, drive_letter
        );

        let client = RemoteFsClient::new(api_url);

        Self {
            client,
            drive_letter,
        }
    }

    /// Ottiene un riferimento al client
    pub fn client(&self) -> &RemoteFsClient {
        &self.client
    }

    /// Ottiene un riferimento mutabile al client
    pub fn client_mut(&mut self) -> &mut RemoteFsClient {
        &mut self.client
    }

    /// Ottiene la lettera del drive
    pub fn drive_letter(&self) -> &str {
        &self.drive_letter
    }

    pub fn unmount(&self) -> Result<(), std::io::Error> {
        todo!();
    }
}
