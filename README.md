# Remote Filesystem – README

## ✨ Descrizione del Progetto

Remote Filesystem è un client filesystem remoto scritto in Rust, capace di montare via FUSE (Linux/WSL) o WinFSP (Windows nativo) la struttura di un server remoto accessibile tramite REST API.
Il sistema è multipiattaforma, e ogni backend (FUSE/WinFSP) espone le classiche operazioni da shell/Explorer sul mountpoint virtuale: navigazione, lettura/scrittura, gestione permessi, creazione/renomina/rimozione, con cache per performance ottimali.

## 🏗️ Architettura

- **Server**: Node.js con TypeScript, containerizzato con Docker
- **Client**: Rust nativo che gira sul sistema operativo host in modalità daemon
- **Storage**: SQLite per i metadati, volume Docker per la persistenza
- **Script di controllo**: PowerShell (Windows) e Bash (Linux) per gestione automatica

## 🐧 Setup Linux/WSL (FUSE)

### 📋 Requisiti

- **Sistema:** Linux nativo o WSL
- **Software:**
    - Rust (toolchain con cargo)
    - FUSE3 (`fuse3`, `libfuse3-dev`, `pkg-config`)

### 🛠️ Installazione

```bash
# 1. Installa i prerequisiti
sudo apt update
sudo apt install fuse3 libfuse3-dev pkg-config

# 2. Installa Rust
curl https://sh.rustup.rs -sSf | sh

# 3. Rendi FUSE utilizzabile dagli utenti
sudo sed -i 's/^#user_allow_other/user_allow_other/' /etc/fuse.conf
```

### 🚀 Avvio del Server

#### Script di controllo
```bash
cd scripts/linux

# Mostra help
./run-server.sh help

# Installa dipendenze
./run-server.sh install

# Avvia server
./run-server.sh start

# Controlla status
./run-server.sh status

# Mostra log
./run-server.sh logs 20

# Ferma server
./run-server.sh stop

# Riavvia server
./run-server.sh restart
```

#### Manuale
```bash
cd server
npm install
npm run build    # Compila TypeScript
npm start        # Avvia server

```

### 🚀 Avvio del Client

#### Script di controllo
```bash
cd scripts/linux

# Mostra help
./remote-fs-ctl.sh help

# Avvia daemon (modalità background)
./remote-fs-ctl.sh start

# Controlla status
./remote-fs-ctl.sh status

# Mostra log
./remote-fs-ctl.sh logs 30

# Esegui test
./remote-fs-ctl.sh test

# Ferma daemon
./remote-fs-ctl.sh stop

# Riavvia daemon
./remote-fs-ctl.sh restart
```

#### Manuale con opzioni avanzate
```bash
cd client

# Modalità daemon (default)
cargo run --release

# Modalità foreground (debug)
cargo run --release -- --foreground

# Mountpoint personalizzato
cargo run --release -- --mount-point /tmp/my-remote-fs

# Server personalizzato
cargo run --release -- --api-url http://custom-server:3000

# Invalidazione cache personalizzata (secondi)
cargo run --release -- --cache-inv 60

# Combinazione opzioni
cargo run --release -- --foreground --mount-point /tmp/test --cache-inv 30
```

Il filesystem sarà montato in `/tmp/remote-fs` di default.

### 🧑‍💻 Comandi Linux/WSL nel Mountpoint

```bash
# Navigazione base
ls /tmp/remote-fs                    # Elenca contenuto
ls -la /tmp/remote-fs               # Elenco dettagliato
cd /tmp/remote-fs                   # Entra nel mountpoint
pwd                                 # Path corrente

# Operazioni file
echo "test content" > file.txt      # Crea e scrive file
cat file.txt                        # Legge contenuto
cp file.txt backup.txt              # Copia file
mv file.txt renamed.txt             # Rinomina file
rm backup.txt                       # Elimina file

# Operazioni directory
mkdir test_dir                      # Crea directory
ls test_dir                         # Lista directory
rmdir test_dir                      # Rimuove directory vuota

# Metadati e attributi
stat file.txt                       # Mostra metadati
file file.txt                       # Tipo file
du -h file.txt                      # Dimensione file
```

***

## 🪟 Setup Windows (WinFSP)

### 📋 Requisiti

- **Sistema:** Windows 10/11 (64 bit)
- **Software:**
    - Rust (toolchain con cargo)
    - Node.js e npm
    - WinFSP ([choco install winfsp -y] oppure installer ufficiale)
    - Docker Desktop (per il server)
- **Permessi:** Privilegi amministrativi per la configurazione driver

### 🛠️ Installazione WinFSP

```powershell
# Installa Chocolatey (se non presente)
Set-ExecutionPolicy Bypass -Scope Process -Force
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))

# Installa WinFSP
choco install winfsp -y

# Verifica installazione
Get-ChildItem "C:\Program Files (x86)\WinFsp" -ErrorAction SilentlyContinue
```

> **💡 Gestione automatica DLL**: Lo script `remote-fs-ctl.ps1` copia automaticamente `winfsp-x64.dll` nella directory `target/release` durante l'avvio del daemon. Non è necessaria configurazione manuale.

### 📋 Gestione manuale DLL (opzionale)

Se necessario, puoi copiare manualmente la DLL WinFSP:

```powershell
# Copia la DLL WinFSP nel target di build
Copy-Item "C:\Program Files (x86)\WinFsp\bin\winfsp-x64.dll" ".\client\target\release\"

# Verifica che la DLL sia presente
Test-Path ".\client\target\release\winfsp-x64.dll"
Get-ChildItem "C:\Program Files\WinFsp\bin\"
```

### 🚀 Avvio del Server

#### Opzione A: Script PowerShell nativo (con Dokcker)
```powershell
# Vai nella directory degli script Windows
cd scripts\windows

# Mostra help
.\run-server.ps1 help

# Builda l'immagine Docker (prima volta)
.\run-server.ps1 build

# Avvia server
.\run-server.ps1 start

# Controlla status
.\run-server.ps1 status

# Mostra log
.\run-server.ps1 logs

# Ferma server
.\run-server.ps1 stop

# Riavvia server
.\run-server.ps1 restart
```

#### Opzione B: manuale con Docker
```powershell
# Avvia server Docker 
docker run -d --name remotefs-server -p 3000:3000 remotefs-server 

# Verifica
Invoke-RestMethod http://localhost:3000/health
```
#### Opzione C: Script di controllo via WSL
```powershell
# Avvia WSL e usa gli script Linux
wsl
cd /mnt/c/Users/Davide/Projects/RustProjects/remote_fs/scripts/linux

# Mostra help
./run-server.sh help

# Installa dipendenze (prima volta)
./run-server.sh install

# Avvia server
./run-server.sh start

# Controlla status
./run-server.sh status

# Mostra log
./run-server.sh logs 20

# Ferma server
./run-server.sh stop

# Riavvia server
./run-server.sh restart
```

#### Opzione D: Manuale via WSL
```powershell
# Avvia WSL
wsl
cd /mnt/c/Users/Davide/Projects/RustProjects/remote_fs/server

# Installa dipendenze
npm install

# Compila TypeScript
npm run build

# Avvia server
npm start

# Oppure modalità sviluppo
npm run start:dev
```

### 🚀 Avvio del Client

#### Script di controllo (Raccomandato)
```powershell
cd scripts\windows

# Mostra help
.\remote-fs-ctl.ps1 help

# Avvia daemon (modalità background)
.\remote-fs-ctl.ps1 start

# Controlla status
.\remote-fs-ctl.ps1 status

# Apre nuova finestra PowerShell con accesso al drive R:
.\remote-fs-ctl.ps1 shell

# Mostra log
.\remote-fs-ctl.ps1 logs

# Esegui test completi
.\remote-fs-ctl.ps1 test

# Ferma daemon
.\remote-fs-ctl.ps1 stop

# Riavvia daemon
.\remote-fs-ctl.ps1 restart
```

#### Manuale con opzioni avanzate
```powershell
cd client

# Modalità daemon (default)
cargo run --release

# Modalità foreground (debug)
cargo run --release -- --foreground

# Drive personalizzato
cargo run --release -- --mount-point S:

# Server personalizzato
cargo run --release -- --api-url http://custom-server:3000

# Invalidazione cache personalizzata (secondi)
cargo run --release -- --cache-inv 120

# Combinazione opzioni
cargo run --release -- --foreground --mount-point T: --cache-inv 60
```

Il filesystem sarà montato come drive `R:` di default.

### 🧑‍💻 Comandi Windows nel Mountpoint

```powershell
# Navigazione base
Get-ChildItem R:\                           # Elenca contenuto
Get-ChildItem R:\ -Force                    # Include file nascosti
Set-Location R:\                            # Entra nel drive
Get-Location                                # Path corrente

# Operazioni file
Set-Content R:\file.txt "test content"      # Crea e scrive file
Get-Content R:\file.txt                     # Legge contenuto
Copy-Item R:\file.txt R:\backup.txt         # Copia file
Rename-Item R:\file.txt R:\renamed.txt      # Rinomina file
Remove-Item R:\backup.txt                   # Elimina file

# Operazioni directory
New-Item R:\test_dir -ItemType Directory    # Crea directory
Get-ChildItem R:\test_dir                   # Lista directory
Remove-Item R:\test_dir                     # Rimuove directory vuota

# Metadati e attributi
Get-Item R:\file.txt | Format-List *        # Mostra metadati
Get-Item R:\file.txt | Select Name,Length   # Info specifiche
```

### 🗒️ Differenze & Note Implementative

- **Permessi:** Su Linux/WSL i permessi UNIX sono pienamente supportati. Su Windows le operazioni di chmod sono emulate dove possibile.
- **Drive Letters:** Windows usa drive letter (R:, S:, etc.), Linux usa mount points (/tmp/remote-fs).
- **Symlink/Hardlink:** Non supportati da nessuna delle due versioni.
- **Cache:** Entrambe le versioni usano cache per metadati e contenuti.

***

## 🧪 Testing

### Test Automatici

#### Linux
```bash
cd scripts/linux
./remote-fs-ctl.sh test    # Esegue test completi FUSE
```

#### Windows
```powershell
cd scripts\windows
.\remote-fs-ctl.ps1 test   # Esegue test completi WinFSP
```

### Test Manuali

I test coprono tutte le operazioni filesystem:
- ✅ **Prerequisites** - Verifica ambiente
- ✅ **Mkdir** - Creazione directory
- ✅ **Readdir** - Lettura directory
- ✅ **Create** - Creazione file
- ✅ **Read/Write** - Lettura/scrittura file
- ✅ **Attributes** - Gestione attributi
- ✅ **Rename** - Rinomina/spostamento
- ✅ **Unlink/Rmdir** - Eliminazione
- ✅ **Statfs** - Statistiche filesystem
- ✅ **Lookup** - Ricerca file
- ✅ **Concurrent** - Operazioni concorrenti
- ✅ **Performance** - Test performance

***

## 🖥️ Logging & Debug

### Modalità Foreground
- Log colorati in console con timestamp, modulo e livello
- Ideale per sviluppo e debug

### Modalità Daemon
- **Linux**: `/tmp/remote-fs-client.log`
- **Windows**: `%TEMP%\remote-fs-client.log`

### Visualizzazione Log
```bash
# Linux
./remote-fs-ctl.sh logs 50      # Ultimi 50 log
tail -f /tmp/remote-fs-client.log

# Windows
.\remote-fs-ctl.ps1 logs        # Ultimi log
Get-Content $env:TEMP\remote-fs-client.log -Wait
```

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

### Esempio API Usage
```bash
# Health check
curl http://localhost:3000/health

# Lista directory
curl "http://localhost:3000/list?path=/"

# Leggi file
curl "http://localhost:3000/files?path=/file.txt"

# Crea file
curl -X POST "http://localhost:3000/files" \
  -H "Content-Type: application/json" \
  -d '{"path": "/new.txt", "content": "Hello World"}'
```

***

## 🏗️ Architettura Aggiornata

```
┌─────────────────┐    HTTP     ┌─────────────────┐
│ FUSE/WinFSP     │ ◄────────►  │    REST Server  │
│ Client (Rust)   │   API       │ (Node.js/TS)    │
│ + Daemon Mode   │             │   + Docker      │
│ + Cache         │             │   + Streaming   │
│ + Control Script│             │   + Health API  │
└─────────────────┘             └─────────────────┘
         │                               │
         │ FUSE / WinFSP                 │ SQLite DB + File Storage
         ▼                               ▼
┌─────────────────┐             ┌─────────────────┐
│  Mount Point    │             │ fs_nodes (meta) │
│ /tmp/remote-fs  │             │ + fs/ (files)   │
│ R:\ (Windows)   │             │ + Docker Volume │
└─────────────────┘             └─────────────────┘

Control Scripts:
├── scripts/linux/remote-fs-ctl.sh    (Client Linux)
├── scripts/linux/run-server.sh       (Server Linux)
└── scripts/windows/remote-fs-ctl.ps1 (Client Windows)
```

***

## 📝 Opzioni Complete del Client

### Opzioni Generali
```bash
# Linux/Windows
--foreground              # Modalità foreground (default: daemon)
--mount-point <PATH>      # Mountpoint personalizzato
--api-url <URL>          # URL server (default: http://localhost:3000)
--cache-inv <SECONDS>    # Secondi invalidazione cache (default: 30)
--help                   # Mostra help

# Esempi
cargo run -- --foreground --mount-point /custom/path --cache-inv 60
cargo run -- --api-url http://remote-server:8080 --foreground
```

### Opzioni Specifiche Linux
```bash
--mount-point /path/to/mount    # Default: /tmp/remote-fs
```

### Opzioni Specifiche Windows
```bash
--mount-point X:               # Default: R:
```

***

## 🚨 Avvertimenti Importanti

- **Server**: Assicurati che il server sia attivo prima di avviare il client
- **Permessi Windows**: Potrebbero essere necessari privilegi amministrativi
- **WSL**: Per il server Node.js su Windows, usa WSL per migliore compatibilità
- **Unmount**: Il mountpoint viene smontato automaticamente alla chiusura del client

### Unmount Manuale

#### Linux
```bash
fusermount -u /tmp/remote-fs
# oppure
sudo umount /tmp/remote-fs
```

#### Windows
```powershell
# Normalmente automatico, ma se necessario:
.\remote-fs-ctl.ps1 stop
# oppure
taskkill /f /im remote_fs.exe
```

***

**Sviluppatori:** Davide Carletto & Michele Carena
