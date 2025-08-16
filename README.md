Ciao michi

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
ts-node src/server.ts
# npm run dev  # Modalità sviluppo con auto-reload, per ora non usare perchè fa casini con wsl
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
ts-node src/server.ts
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

#### ✅ Navigazione e Listagem
```bash
ls                    # Lista contenuto directory corrente
ls -la                # Lista dettagliata con permessi
pwd                   # Mostra directory corrente
cd documents          # Cambia directory
cd ..                 # Torna indietro
```

#### ✅ Lettura File
```bash
cat test.txt          # Legge contenuto di un file
file test.txt         # Mostra tipo di file
stat test.txt         # Mostra metadati dettagliati
head test.txt         # Prime righe del file
tail test.txt         # Ultime righe del file
```

#### ✅ Creazione File e Directory
```bash
touch newfile.txt     # Crea nuovo file vuoto
mkdir newdir          # Crea nuova directory
```

#### ✅ Rimozione
```bash
rm filename           # Rimuove un file
rmdir dirname         # Rimuove una directory vuota
```

#### ✅ Controllo Permessi
```bash
ls -la                # Mostra permessi dettagliati
```

#### ❌ Comandi NON Ancora Supportati
```bash
echo "text" > file    # Scrittura file (write non implementata)
cp file newfile       # Copia file
mv file newname       # Rinomina/sposta file
ln -s target link     # Link simbolici
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

Il server espone i seguenti endpoint (Da modificare in fase finale per avere solo quelli richiesti):

| Endpoint | Metodo | Descrizione | Status |
|----------|--------|-------------|--------|
| `/health` | GET | Health check del server | ✅ |
| `/resolve-inode/:ino` | GET | Risolve inode in path | ✅ |
| `/metadata?path=<path>` | GET | Ottiene metadati di un file | ✅ |
| `/create` | POST | Crea nuovo file/directory | ✅ |
| `/open` | POST | Apre un file e restituisce file handle | ✅ |
| `/read` | POST | Legge contenuto di un file | ✅ |
| `/listdir?path=<path>` | GET | Lista contenuto di una directory | ✅ |
| `/remove?path=<path>&is_directory=<bool>` | DELETE | Rimuove file/directory | ✅ |
| `/debug/files` | GET | Lista tutti i file mock (debug) | ✅ |

## 🏗️ Architettura

```
┌─────────────────┐    HTTP     ┌─────────────────┐
│   FUSE Client   │ ◄────────► │   REST Server   │
│     (Rust)      │   API       │ (Node.js/TS)    │
└─────────────────┘             └─────────────────┘
         │                               │
         │ FUSE                         │ Mock FileSystem
         ▼                               ▼
┌─────────────────┐             ┌─────────────────┐
│  Mount Point    │             │  In-Memory      │
│ /tmp/remote-fs  │             │  Data Store     │
└─────────────────┘             └─────────────────┘
```

## ⚙️ Stato dell'Implementazione

### ✅ Funzioni FUSE Implementate

| Funzione | Descrizione | Comandi Abilitati |
|----------|-------------|-------------------|
| `init` | Inizializzazione filesystem | Mount del filesystem |
| `destroy` | Cleanup al dismount | Unmount pulito |
| `lookup` | Risoluzione nomi file | `ls`, `cat`, `cd` |
| `getattr` | Ottenimento metadati | `ls -l`, `stat`, `file` |
| `setattr` | Modifica metadati (parziale) | Aggiornamento permessi |
| `mknod` | Creazione file regolari | `touch` |
| `mkdir` | Creazione directory | `mkdir` |
| `unlink` | Rimozione file | `rm` |
| `rmdir` | Rimozione directory | `rmdir` |
| `open` | Apertura file | Preparazione lettura |
| `read` | Lettura contenuto file | `cat`, `head`, `tail` |
| `readdir` | Lettura contenuto directory | `ls` |
| `opendir` | Apertura directory | Preparazione `ls` |
| `release` | Chiusura file | Cleanup automatico |
| `releasedir` | Chiusura directory | Cleanup automatico |
| `statfs` | Statistiche filesystem | `df` |

### ❌ Funzioni FUSE Non Implementate

| Funzione | Descrizione | Comandi Mancanti |
|----------|-------------|------------------|
| `write` | Scrittura file | `echo > file`, `cp`, editor |
| `flush` | Sincronizzazione | Scrittura sicura |
| `fsync` | Sincronizzazione forzata | `sync` |
| `rename` | Rinomina/sposta file | `mv` |
| `link` | Hard link | `ln` |
| `symlink` | Link simbolici | `ln -s` |
| `readlink` | Lettura link simbolici | Risoluzione symlink |
| `create` | Creazione + apertura atomica | Ottimizzazione |
| `access` | Controllo permessi | Controlli avanzati |
| Extended attributes | Attributi estesi | `setfattr`, `getfattr` |
| File locking | Lock di file | Accesso concorrente |

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

## 📋 TODO / Roadmap

### 🔥 Priorità Alta
- [ ] Implementare funzione `write` per supportare scrittura file
- [ ] Implementare funzione `flush` per sincronizzazione
- [ ] Implementare funzione `rename` per `mv` e rinomina
- [ ] Migliorare gestione errori e logging

### ⚠️ Implementazioni Parziali
- **`setattr`**: Attualmente implementato ma ignora gli aggiornamenti timestamp
  - ✅ `touch nuovo_file.txt` funziona (crea il file via `mknod`)
  - ✅ `touch file_esistente.txt` funziona (ma non aggiorna timestamp)
  - [ ] **Opzionale**: Implementare supporto completo timestamp (atime/mtime) in `setattr`

### 🚀 Miglioramenti
- [ ] Implementare cache locale per performance
- [ ] Aggiungere supporto per link simbolici (`symlink`, `readlink`)
- [ ] Implementare `create` per creazione + apertura atomica
- [ ] Aggiungere supporto per attributi estesi
- [ ] Implementare file locking per accesso concorrente

### 🔧 Produzione
- [ ] Rimuovere modalità daemon opzionale (sempre non-daemon in produzione)
- [ ] Cambiare logging da `truncate` ad `append`
- [ ] Cambiare livello log default da `debug` a `info`
- [ ] Aggiungere autenticazione e sicurezza per le API
- [ ] Implementare persistenza su database invece di in-memory
- [ ] Aggiungere configurazione via file config
- [ ] Implementare healthcheck e monitoring

### 🧪 Testing
- [ ] Aggiungere test unitari per tutte le funzioni FUSE
- [ ] Aggiungere test di integrazione client-server
- [ ] Aggiungere test di performance e stress
- [ ] Aggiungere test di recovery e error handling

**Sviluppatori:** Davide Carletto & Michele Carena