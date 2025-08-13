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
> Questo comando decommented la riga `user_allow_other` in `/etc/fuse.conf` per permettere l'auto-unmounting.

## 🛠️ Installazione e Utilizzo

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

| Endpoint | Metodo | Descrizione |
|----------|--------|-------------|
| `/health` | GET | Health check del server |
| `/resolve-inode/:ino` | GET | Risolve inode in path |
| `/metadata?path=<path>` | GET | Ottiene metadati di un file |

## 🏗️ Architettura

```
┌─────────────────┐    HTTP     ┌─────────────────┐
│   FUSE Client   │ ◄────────► │   REST Server   │
│     (Rust)      │   API       │ (Node.js/TS)    │
└─────────────────┘             └─────────────────┘
         │                               │
         │ FUSE                         │ File System
         ▼                               ▼
┌─────────────────┐             ┌─────────────────┐
│  Mount Point    │             │  Remote Storage │
│ /tmp/remote-fs  │             │   (Simulated)   │
└─────────────────┘             └─────────────────┘
```

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

## 📋 TODO / Note di sviluppo

- [ ] Implementare funzione `getattr` per supportare `ls` e `stat`
- [ ] Implementare funzione `readdir` per listare contenuti directory  
- [ ] Implementare funzioni di lettura/scrittura file
- [ ] Una volta finito lo sviluppo, rimuovere l'opzione `--daemon` (dovrebbe partire sempre come daemon)
- [ ] Per il logging, cambiare da `truncate` ad `append` in produzione
- [ ] Cambiare livello di log default da `debug` a `info` in produzione
- [ ] Aggiungere autenticazione e sicurezza per le API
- [ ] Implementare cache locale per le operazioni frequenti

**Sviluppatori:** Davide Carletto & Michele Carena