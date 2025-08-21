Ciao michi

installare winfsp 
choco install winfsp

se non hai choco:
Set-ExecutionPolicy Bypass -Scope Process -Force; `
[System.Net.ServicePointManager]::SecurityProtocol = `
[System.Net.ServicePointManager]::SecurityProtocol -bor 3072; `
iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))


# Remote Filesystem (FUSE)

Un client filesystem remoto implementato in Rust che utilizza FUSE per montare un filesystem che rispecchia la struttura di un server remoto via REST API.

## 📋 Prerequisiti

**Per WSL (Raccomandato per Windows):**
- WSL installato (consigliato Ubuntu)
- Node.js e npm: `sudo apt install nodejs npm`
- Rust: `curl https://sh.rustup.rs -sSf | sh`
- FUSE: `sudo apt-get install fuse3 libfuse3-dev pkg-config`

**Configurazione FUSE:**
```bash
sudo sed -i 's/^#user_allow_other/user_allow_other/' /etc/fuse.conf
```
> Questo comando decommenta la riga `user_allow_other` in `/etc/fuse.conf` per permettere l'auto-unmounting.

## 🛠️ Installazione e Configurazione

### 1. Clona il repository
```bash
git clone https://github.com/DavideCarletto/RemoteFs.git
cd RemoteFs
```

### 2. Avvia il server
```bash
cd server
npm install
npm start
```
Il server sarà disponibile su `http://localhost:3000`

### 3. Avvia il client

**Modalità normale (con log su console):**
```bash
cd client
cargo run
```

**Modalità daemon (esecuzione in background):**
```bash
cargo run -- --daemon
```

**Specififica mountpoint personalizzato:**
```bash
cargo run -- --mount-point /custom/mount/path
```

**Opzioni complete:**
```bash
# Daemon con mountpoint personalizzato
cargo run -- --daemon --mount-point /tmp/my-remote-fs
```

## 📝 Opzioni del Client

| Opzione | Descrizione | Default |
|---------|-------------|---------|
| `--mount-point <PATH>` | Directory dove montare il filesystem | `/tmp/remote-fs` |
| `--daemon` | Esegui come daemon in background | `false` |

## � Come Usare il Filesystem

### Procedura per Lavorare con il Filesystem

#### Step 1: Avvia il Server da un terminale wsl (Terminal 1)
```bash
cd server
npm start
```
Il server sarà disponibile su `http://localhost:3000`

#### Step 2: Avvia il Client da un altro terminale wsl (Terminal 2)
```bash
cd client
cargo run
```
Il filesystem sarà montato su `/tmp/remote-fs`

#### Step 3: Usa il Filesystem da un terzo terminale wsl (Terminal 3)
```bash
cd /tmp/remote-fs
# Da qui puoi lavorare con il filesystem remoto!
```

### 🎯 Comandi Supportati

Una volta nel mount point `/tmp/remote-fs`, puoi usare questi comandi:

#### ✅ Comandi Supportati Base
```bash
ls                    # Lista contenuto directory corrente
ls -la                # Lista dettagliata con permessi
pwd                   # Mostra directory corrente
cd documents          # Cambia directory
cd ..                 # Torna indietro
cat test.txt          # Legge contenuto di un file
file test.txt         # Mostra tipo di file
stat test.txt         # Mostra metadati dettagliati
head test.txt         # Prime righe del file
tail test.txt         # Ultime righe del file
touch newfile.txt     # Crea nuovo file vuoto
mkdir newdir          # Crea nuova directory
rm filename           # Rimuove un file
rmdir dirname         # Rimuove una directory vuota
```

#### ✅ Comandi Avanzati Supportati
```bash
echo "text" > file    # Scrittura file con streaming
cp file newfile       # Copia file
mv file newname       # Rinomina/sposta file  
chmod 755 file        # Modifica permessi
```

## 📊 Logging

**Modalità normale:**
- Output su console con colori
- Formato: `HH:MM:SS[modulo][LIVELLO] messaggio`

**Modalità daemon:**
- Output su file `/tmp/remote-fs-client.log`
- Formato: `YYYY-MM-DD HH:MM:SS[modulo][LIVELLO] messaggio`
- PID file: `/tmp/remote-fs-client.pid`

## 🔌 API del Server

Il server espone i seguenti endpoint:

| Endpoint | Metodo | Descrizione | Status |
|----------|--------|-------------|--------|
| `/health` | GET | Health check del server | ✅ |
| `/list?path=<path>` | GET | Lista contenuto di una directory | ✅ |
| `/files` | GET | Legge contenuto di un file (streaming) | ✅ |
| `/files` | PUT | Scrive contenuto di un file (streaming) | ✅ |
| `/files` | POST | Crea nuovo file regolare | ✅ |
| `/files` | DELETE | Rimuove file/directory | ✅ |
| `/mkdir` | POST | Crea nuova directory | ✅ |
| `/open` | POST | Apre un file e restituisce file handle | ✅ |
| `/rename` | POST | Rinomina/sposta file/directory | ✅ |
| `/resolve-inode/:ino` | GET | Risolve inode in path | ✅ |
| `/metadata?path=<path>` | GET | Ottiene metadati di un file | ✅ |
| `/metadata?path=<path>` | PATCH | Aggiorna metadati di un file | ✅ |

## 🏗️ Architettura

```
┌─────────────────┐    HTTP     ┌─────────────────┐
│   FUSE Client   │ ◄────────► │   REST Server   │
│     (Rust)      │   API       │ (Node.js/TS)    │
│   + Cache LRU   │             │   + Streaming   │
└─────────────────┘             └─────────────────┘
         │                               │
         │ FUSE                         │ SQLite DB + File Storage
         ▼                               ▼
┌─────────────────┐             ┌─────────────────┐
│  Mount Point    │             │ fs_nodes (meta) │
│ /tmp/remote-fs  │             │ + fs/ (files)   │
└─────────────────┘             └─────────────────┘
```

### 🗄️ **Persistenza e Storage**

Il sistema utilizza un'architettura ibrida per la persistenza dei dati:

- **Database SQLite** (`fs.sqlite`): Contiene solo i metadati dei file e directory
  - Struttura filesystem (path, inode, permessi, timestamp)
  - Informazioni sui file (dimensioni, tipo, proprietario)
  - Relazioni parent-child per la gerarchia delle directory

- **File fisici** (`fs/` directory): Contiene il contenuto effettivo dei file
  - Ogni file è salvato con nome uguale al suo inode (es. `fs/2` per inode 2)
  - Permette streaming efficiente per file di qualunque dimensione
  - Compatibile con strumenti standard del filesystem

**Vantaggi di questa architettura:**
- **Performance**: Query veloci sui metadati, streaming diretto sui contenuti
- **Scalabilità**: File grandi non impattano sul database
- **Debugging**: File ispezionabili direttamente nel filesystem
- **Backup**: Separazione tra struttura (DB) e contenuto (files)

## ⚙️ Stato dell'Implementazione

### ✅ Funzioni FUSE Implementate

| Funzione | Descrizione | Comandi Abilitati |
|----------|-------------|-------------------|
| `init` | Inizializzazione filesystem | Mount del filesystem |
| `destroy` | Cleanup al dismount | Unmount pulito |
| `lookup` | Risoluzione nomi file | `ls`, `cat`, `cd` |
| `getattr` | Ottenimento metadati | `ls -l`, `stat`, `file` |
| `setattr` | Modifica metadati | `chmod`, aggiornamento timestamp |
| `mknod` | Creazione file regolari | `touch` |
| `mkdir` | Creazione directory | `mkdir` |
| `unlink` | Rimozione file | `rm` |
| `rmdir` | Rimozione directory | `rmdir` |
| `rename` | Rinomina/sposta file | `mv` |
| `open` | Apertura file | Preparazione lettura/scrittura |
| `read` | Lettura contenuto file | `cat`, `head`, `tail` |
| `write` | Scrittura contenuto file | `echo >`, editing file |
| `readdir` | Lettura contenuto directory | `ls` |
| `opendir` | Apertura directory | Preparazione `ls` |
| `release` | Chiusura file | Cleanup automatico |
| `releasedir` | Chiusura directory | Cleanup automatico |
| `flush` | Sincronizzazione | Sync automatico |
| `statfs` | Statistiche filesystem | `df` |
| `access` | Controllo permessi | Test accesso file |
| `create` | Creazione + apertura atomica | Creazione efficiente |

### ❌ Funzioni FUSE Non Implementate

| Funzione | Descrizione | Motivo |
|----------|-------------|--------|
| `link` | Hard link | Non richiesto dalla specifica |
| `symlink` | Link simbolici | Non richiesto dalla specifica |
| `readlink` | Lettura link simbolici | Non richiesto dalla specifica |
| Extended attributes | Attributi estesi | Non supportati dal server |
| File locking | Lock di file | Non necessario per caso d'uso |
| `fsync` | Sincronizzazione forzata | Gestita automaticamente |
| `fsyncdir` | Sincronizzazione directory | Gestita automaticamente |

### 🚀 **Sistema di Cache**

Il client implementa un sistema di cache LRU (Least Recently Used) per ottimizzare le performance:

- **Cache Metadati**: Memorizza informazioni sui file (dimensioni, permessi, timestamp)
- **Cache Path-to-Inode**: Velocizza la risoluzione dei percorsi
- **Cache Directory Listing**: Riduce le chiamate per `ls` ripetuti
- **Invalidazione Intelligente**: La cache viene invalidata automaticamente dopo operazioni di modifica

**Benefici:**
- Riduzione significativa della latenza per operazioni ripetute
- Minor carico sul server
- Migliore responsività dell'interfaccia utente

## 🚨 Importante

- **Tutti i comandi devono essere eseguiti in WSL** se lavori su Windows
- Il server deve essere avviato da WSL per comunicare correttamente con il client
- Per unmount: il filesystem si smonta automaticamente quando il client termina

## 🛑 Unmount manuale

Se il filesystem rimane montato:
```bash
fusermount -u /tmp/remote-fs
# oppure
sudo umount /tmp/remote-fs
```

## 🎯 Stato Attuale del Progetto

### ✅ **COMPLETATO - Funzionalità Core**
- **Client FUSE**: Implementazione completa con tutte le funzioni essenziali
- **Server HTTP**: API REST con streaming per file di grandi dimensioni
- **Cache System**: Cache LRU integrata per ottimizzazione performance
- **Persistenza Ibrida**: Database SQLite per metadati + file fisici per contenuti
- **Operazioni Complete**: Lettura, scrittura, creazione, eliminazione, rinomina
- **Streaming**: Supporto nativo per file di qualunque dimensione
- **Gestione Errori**: Error handling robusto e logging completo

### ✅ **FUNZIONALITÀ COMPLETAMENTE OPERATIVE**
```bash
# Navigazione e informazioni
ls, cd, pwd, stat                ✅ Con cache ottimizzata
ls -la                          ✅ Metadati completi

# Operazioni sui file  
cat, head, tail                 ✅ Streaming efficiente
touch file.txt                  ✅ Creazione atomica
echo "text" > file.txt          ✅ Scrittura con streaming
rm file.txt                     ✅ Eliminazione sicura

# Operazioni directory
mkdir newdir                    ✅ Creazione con metadati
rmdir newdir                    ✅ Rimozione con controlli

# Operazioni avanzate
cp file newfile                 ✅ Copia via streaming
mv file newfile                 ✅ Rinomina/sposta atomica
chmod 755 file                  ✅ Modifica permessi
```

### 🏆 **ARCHITETTURA FINALE**

Il sistema ha raggiunto una architettura matura e scalabile:

- **Performance**: Cache LRU riduce latenza su operazioni ripetute
- **Scalabilità**: Streaming nativo gestisce file fino a GB senza limiti di memoria
- **Affidabilità**: Database transazionale garantisce consistenza dei metadati
- **Manutenibilità**: Separazione chiara tra metadati (DB) e contenuti (filesystem)

### 🔧 **Possibili Estensioni Future**

#### **1. Supporto Multi-Piattaforma (Dalla Specifica Originale)**
- **⚪ macOS Support**: Integrazione con macFUSE per supporto macOS nativo
  ```bash
  # Target: Supporto macOS con macFUSE
  brew install macfuse
  cargo build --target x86_64-apple-darwin
  ```
- **⚪ Windows Support**: Implementazione con WinFSP o Dokany per Windows
  ```rust
  // Target: Supporto Windows filesystem
  #[cfg(target_os = "windows")]
  use winfsp_rs::*;  // o dokany-rs
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

### 🧪 **Testing Strategy Future**
```bash
# Possibili test aggiuntivi da implementare:
tests/
├── stress/         # Test di carico estremo (1000+ file, GB di dati)
├── network/        # Test con latenza/disconnessioni simulate
├── concurrent/     # Test multi-client simultanei
└── benchmark/      # Confronto performance con filesystem locali
```

---

**Sviluppatori:** Davide Carletto & Michele Carena  