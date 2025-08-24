# Remote Filesystem – README

## ✨ Descrizione del Progetto

Remote Filesystem è un client filesystem remoto scritto in Rust, capace di montare via FUSE (Linux/WSL) o WinFSP (Windows nativo) la struttura di un server remoto accessibile tramite REST API.
Il sistema è multipiattaforma, e ogni backend (FUSE/WinFSP) espone le classiche operazioni da shell/Explorer sul mountpoint virtuale: navigazione, lettura/scrittura, gestione permessi, creazione/renomina/rimozione, con cache per performance ottimali.

***

## 🐧 Modalità 1 – Linux/WSL (FUSE)

### 📋 Requisiti

- **Sistema:** Linux nativo o WSL (Windows Subsystem for Linux, preferibilmente Ubuntu)
- **Software:**
    - Rust (toolchain con cargo)
    - Node.js e npm
    - FUSE3 (`fuse3`, `libfuse3-dev`, `pkg-config`)
- **Permessi:** Accesso ad una shell con privilegi per mount FUSE


### 🛠️ Installazione

```bash
# 1. Installa i prerequisiti
sudo apt update
sudo apt install fuse3 libfuse3-dev nodejs npm pkg-config

# 2. Installa Rust
curl https://sh.rustup.rs -sSf | sh

# 3. Clona il repository
git clone https://github.com/DavideCarletto/RemoteFs.git
cd RemoteFs

# 4. Rendi FUSE utilizzabile dagli utenti
sudo sed -i 's/^#user_allow_other/user_allow_other/' /etc/fuse.conf
```


### 🚀 Avvio

```bash
# Terminale 1: Avvia il server REST
cd server
npm install
npm start
# Ora il server è su http://localhost:3000

# Terminale 2: Avvia il client FUSE
cd client
cargo run
# Modalità daemon (background)
cargo run -- --daemon
# Mountpoint personalizzato
cargo run -- --mount-point /tmp/my-remote-fs
```

Il filesystem sarà montato in `/tmp/remote-fs` di default (modificabile).

***

### 🧑‍💻 Comandi da Linux/WSL nel Mountpoint (esempi pratici)

> Da qualsiasi shell dentro la dir montata (es. cd /tmp/remote-fs):

```bash
ls                        # Elenca il contenuto della directory attuale
ls -la                    # Elenco dettagliato (permessi, inode, tipo)
pwd                       # Mostra il path corrente
cd newdir                 # Entra in una directory
cd ..                     # Torna indietro
cat file.txt              # Legge il contenuto di un file (streaming)
stat file.txt             # Mostra metadati avanzati
file file.txt             # Mostra tipo file
head file.txt             # Prime 10 righe
tail file.txt             # Ultime 10 righe
touch nuovo.txt           # Crea un nuovo file vuoto
echo "ciao" > nuovo.txt   # Scrive in un file (streaming)
cp file.txt copia.txt     # Copia file (streaming)
mv file.txt nuovo.txt     # Rinomina o sposta file
mkdir nuova_dir           # Crea una directory
rm file.txt               # Elimina un file
rmdir nuova_dir           # Elimina directory vuota
chmod 755 file.txt        # Modifica permessi
```

> Le stesse azioni possono essere automatizzate via script bash/copia.

***

## 🪟 Modalità 2 – Windows nativo (WinFSP)

### 📋 Requisiti

- **Sistema:** Windows 10/11 (64 bit)
- **Software:**
    - Rust (toolchain con cargo)
    - Node.js e npm
    - WinFSP ([choco install winfsp -y] oppure installer ufficiale)
    - Chocolatey (gestore pacchetti, opzionale ma consigliato)
- **Permessi:** Privilegi amministrativi per la configurazione driver/DLL


### 🛠️ Installazione WinFSP

```powershell
# Installa Chocolatey (se non presente)
Set-ExecutionPolicy Bypass -Scope Process -Force
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))

# Installa WinFSP
choco install winfsp -y

# Eventualmente aggiorna
choco upgrade winfsp -y

# Dopo l’installazione:
# - Assicurati che winfsp-x64.dll sia in C:\Program Files\WinFsp\bin\
# - Se necessario, copia winfsp-x64.dll nella cartella dell’eseguibile Rust (target\debug o target\release):
copy "C:\Program Files\WinFsp\bin\winfsp-x64.dll" .\target\debug\
# - Aggiungi C:\Program Files\WinFsp\bin\ al PATH (se non già)
```


### 🚀 Avvio

```powershell
# Terminale 1 (PowerShell): Avvia il server Node.js in WSL o Docker
wsl
cd ~/RemoteFs/server   # o percorso equivalente
npm install
npm start

# Terminale 2 (PowerShell admin): Avvia il client Rust WinFSP
cd .\RemoteFs\client
cargo run
# Il filesystem remount sarà visibile come un nuovo drive es: R:\
```


***

### 🧑‍💻 Comandi su Mountpoint Windows (esempi pratici, PowerShell/CMD)

> All’interno del nuovo drive virtuale (`R:\`), tramite PowerShell, CMD e Explorer:

```powershell
# Navigazione e informazione
ls                           # Elenca contenuto directory
ls -la                       # Elenco dettagliato (solo PowerShell 7+)
pwd                          # Mostra percorso corrente
cd nuova_dir                 # Cambia directory
cd ..                        # Torna alla directory precedente
# Lettura e scrittura file
type file.txt                # Leggi contenuto file (cmd)
Get-Content file.txt         # Leggi contenuto file (PowerShell)
stat file.txt                # Mostra informazioni base (Get-Item file.txt | Format-List *)
head -n 10 file.txt          # Prime righe (Get-Content file.txt -TotalCount 10)
tail -n 10 file.txt          # Ultime righe (Get-Content file.txt | Select-Object -Last 10)
New-Item nuovo.txt -ItemType File  # Crea nuovo file vuoto
echo "ciao" > nuovo.txt      # Scrive in un file (streaming)
cp file.txt copia.txt        # Copia file (PowerShell/CMD)
mv file.txt nuovo.txt        # Rinomina o sposta file (PowerShell/CMD)
New-Item nuova_dir -ItemType Directory # Crea directory
del file.txt                 # Elimina file (CMD)
Remove-Item file.txt         # Elimina file (PowerShell)
Remove-Item nuova_dir        # Elimina directory vuota (PowerShell)
Rename-Item file.txt nuovo.txt # Rinomina file
# Modifica permessi (parziale)
# NB: L’attributo sola lettura si può modificare con:
Set-ItemProperty -Path file.txt -Name IsReadOnly -Value $true
```

> Anche tramite Esplora Risorse è possibile navigare, copiare, rinominare, eliminare file e directory, oltre che vedere le dimensioni e le date.

***

### 🗒️ Differenze \& Note Implementative

- **Permessi:** Su Linux/WSL i permessi UNIX sono pienamente supportati. Su Windows le operazioni di chmod sono emulate dove possibile (attributi sola lettura/sistema), ma non c’è corrispondenza 1:1 con i permessi POSIX.
- **Symlink/Hardlink:** Non supportati da nessuna delle due versioni.
- **Streaming:** Entrambe le versioni supportano la lettura/scrittura di file grandi via REST chunked.
- **Cache:** Sia FUSE che WinFSP usano cache per metadati e contenuti.

***

## 🖥️ Logging \& Debug

- Modalità console: log colorati con timestamp, modulo e livello.
- Modalità daemon: file `/tmp/remote-fs-client.log` (Linux) o file log in cartella temp (Windows).

***

## 🔌 API del Server

Il server espone i seguenti endpoint:

| Endpoint                     | Metodo  | Descrizione                           | Status |
|------------------------------|---------|---------------------------------------|--------|
| `/health`                    | GET     | Health check del server               | ✅ |
| `/list?path=<path>`          | GET     | Lista contenuto di una directory      | ✅ |
| `/files`                     | GET     | Legge contenuto di un file (streaming) | ✅ |
| `/files`                     | PUT     | Scrive contenuto di un file (streaming) | ✅ |
| `/files`                     | POST    | Crea nuovo file regolare              | ✅ |
| `/files`                     | DELETE  | Rimuove file/directory                | ✅ |
| `/mkdir`                     | POST    | Crea nuova directory                  | ✅ |
| `/open`                      | POST    | Apre un file e restituisce file handle | ✅ |
| `/rename`                    | POST    | Rinomina/sposta file/directory        | ✅ |
| `/resolve-inode/:ino`        | GET     | Risolve inode in path                 | ✅ |
| `/metadata?path=<path>`      | GET     | Ottiene metadati di un file           | ✅ |
| `/metadata?path=<path>`      | PATCH   | Aggiorna metadati di un file          | ✅ |

---

### ⚙️ Compatibilità
- **Node.js / TypeScript**  
- **SQLite** per metadati e storage fisico file  

***

## 🏗️ Architettura

```
┌─────────────────┐    HTTP     ┌─────────────────┐
│   FUSE Client   │ ◄────────► │   REST Server   │
│     (Rust)      │   API       │ (Node.js/TS)    │
│   + Cache LRU   │             │   + Streaming   │
└─────────────────┘             └─────────────────┘
         │                               │
         │ FUSE / WinFSP                │ SQLite DB + File Storage
         ▼                               ▼
┌─────────────────┐             ┌─────────────────┐
│  Mount Point    │             │ fs_nodes (meta) │
│ /tmp/remote-fs  │             │ + fs/ (files)   │
│ R:\             │             └─────────────────┘
└─────────────────┘
```


***

## 🚨 Avvertimenti Importanti

- Su Windows il server Node.js **va avviato sempre da WSL** per garantire compatibilità.
- Il mountpoint viene smontato automaticamente alla chiusura del client.
- Per unmount manuale su Linux:

```bash
fusermount -u /tmp/remote-fs
# oppure
sudo umount /tmp/remote-fs
```


***

### 🔧 **Possibili Estensioni Future**

#### **1. Supporto MacOs**
- **⚪ macOS Support**: Integrazione con macFUSE per supporto macOS nativo
  ```bash
  # Target: Supporto macOS con macFUSE
  brew install macfuse
  cargo build --target x86_64-apple-darwin
  ```

#### **2. Sicurezza e Produzione**
- **🔒 HTTPS/TLS Support**: Comunicazione sicura client-server
  ```typescript
  // Server con certificati SSL/TLS
  const https = require('https');
  const fs = require('fs');
  
  const options = {
    key: fs.readFileSync('private-key.pem'),
    cert: fs.readFileSync('certificate.pem')
  };
  
  https.createServer(options, app).listen(443);
  ```

#### **3. Containerizzazione e Deployment**
- **🐳 Docker Support**: Containerizzazione completa del sistema
  ```dockerfile
  # Dockerfile per server
  FROM node:18-alpine
  WORKDIR /app
  COPY package*.json ./
  RUN npm ci --only=production
  COPY . .
  EXPOSE 3000
  CMD ["npm", "start"]
  ```
  ```dockerfile
  # Dockerfile per client (con FUSE support)
  FROM rust:1.70-slim
  RUN apt-get update && apt-get install -y fuse libfuse-dev
  WORKDIR /app
  COPY . .
  RUN cargo build --release
  CMD ["./target/release/remote-fs"]
  ```
- **🎛️ Docker Compose**: Orchestrazione multi-container
  ```yaml
  version: '3.8'
  services:
    remote-fs-server:
      build: ./server
      ports:
        - "3000:3000"
      volumes:
        - ./data:/app/data
    
    remote-fs-client:
      build: ./client
      privileged: true  # Required for FUSE
      devices:
        - /dev/fuse
      volumes:
        - /tmp/remote-fs:/mnt/remote-fs:shared
  ```

**Sviluppatori:** Davide Carletto & Michele Carena  