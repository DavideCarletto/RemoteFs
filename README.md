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

| Funzione | Descrizione | Status |
|----------|-------------|--------|
| `rename` | Rinomina/sposta file | ⚠️ Implementata ma non testata |
| `link` | Hard link | ❌ Non implementata |
| `symlink` | Link simbolici | ❌ Non implementata |
| `readlink` | Lettura link simbolici | ❌ Non implementata |
| `create` | Creazione + apertura atomica | ✅ **IMPLEMENTATA** |
| `access` | Controllo permessi | ✅ **IMPLEMENTATA** |
| `flush` | Sincronizzazione | ✅ **IMPLEMENTATA** (no-op) |
| `fsync` | Sincronizzazione forzata | ✅ **IMPLEMENTATA** (no-op) |
| `fsyncdir` | Sincronizzazione directory | ✅ **IMPLEMENTATA** (no-op) |
| Extended attributes | Attributi estesi | ✅ **IMPLEMENTATA** (non supportati) |
| File locking | Lock di file | ❌ Non implementata |

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
- **Client FUSE**: Completamente implementato con tutte le funzioni essenziali
- **Server HTTP**: API REST funzionante con 5 endpoint spec-compliant + endpoint FUSE support
- **Comunicazione**: Protocollo HTTP ben definito e stabile tra client e server
- **Operazioni Supportate**: Creazione, lettura, scrittura, eliminazione di file e directory
- **Streaming**: Supporto per file di grandi dimensioni con chunking (64KB)
- **Logging**: Sistema di log completo e configurabile
- **Gestione Errori**: Error handling robusto su client e server

### ✅ **FUNZIONALITÀ COMPLETAMENTE FUNZIONANTI**
```bash
# Navigazione
ls, cd, pwd                    ✅ Funzionante
ls -la                        ✅ Metadati completi
stat file.txt                 ✅ Informazioni complete

# File Operations  
cat, head, tail               ✅ Lettura file con streaming
touch file.txt                ✅ Creazione file
echo "text" > file.txt        ✅ Scrittura file con streaming
rm file.txt                   ✅ Eliminazione file

# Directory Operations
mkdir newdir                  ✅ Creazione directory
rmdir newdir                  ✅ Eliminazione directory

# Advanced Operations
cp file newfile               ✅ Funziona (via read+write)
```

### 🎯 **PROSSIMI SVILUPPI PIANIFICATI**

#### 🔥 **1. Sistema di Cache (Priorità Alta)**
- **Obiettivo**: Migliorare performance riducendo chiamate HTTP ripetute
- **Implementazione Proposta**:
  ```rust
  struct ClientCache {
      metadata_cache: LRU<String, FileMetadata>,    // Cache metadati file
      content_cache: LRU<String, Vec<u8>>,          // Cache contenuto file piccoli
      directory_cache: LRU<String, Vec<DirEntry>>,  // Cache listing directory
      inode_path_cache: LRU<u64, String>,          // Cache inode->path mapping
      ttl: Duration,                                 // Time-to-live configurabile
  }
  ```
- **Benefici Attesi**: 
  - Riduzione latenza su operazioni ripetute (es. `ls` multipli)
  - Minore carico sul server
  - Migliore esperienza utente
- **Strategie**: Cache LRU con TTL configurabile e invalidazione intelligente

#### 🗄️ **2. Persistenza Server (Priorità Alta)**
- **Problema Attuale**: Tutti i dati sono in-memory e si perdono al restart del server
- **Soluzioni Possibili**:
  ```typescript
  // Opzione 1: File System Backend
  class FileSystemBackend {
    private basePath: string;
    saveFile(path: string, content: Buffer): Promise<void>
    loadFile(path: string): Promise<Buffer>
    saveMetadata(path: string, metadata: INode): Promise<void>
  }

  // Opzione 2: SQLite Backend (Raccomandato)
  class SQLiteBackend {
    // Schema veloce da implementare, perfetto per prototipo
    // Supporta transazioni e concorrenza
  }

  // Opzione 3: PostgreSQL Backend (Per produzione)
  class PostgreSQLBackend {
    // Massima scalabilità e robustezza
  }
  ```
- **Implementazione Suggerita**: Iniziare con SQLite per semplicità

#### 🗃️ **3. Database per Metadati (Priorità Media)**
- **Obiettivo**: Gestione efficiente e scalabile dei metadati filesystem
- **Schema Database Proposto**:
  ```sql
  -- Tabella principale nodi filesystem
  CREATE TABLE fs_nodes (
      ino BIGINT PRIMARY KEY,
      path VARCHAR(4096) UNIQUE NOT NULL,
      parent_ino BIGINT REFERENCES fs_nodes(ino),
      name VARCHAR(255) NOT NULL,
      file_type VARCHAR(20) NOT NULL CHECK (file_type IN ('RegularFile', 'Directory')),
      size BIGINT DEFAULT 0,
      permissions INTEGER DEFAULT 644,
      uid INTEGER DEFAULT 1000,
      gid INTEGER DEFAULT 1000,
      atime TIMESTAMP DEFAULT NOW(),
      mtime TIMESTAMP DEFAULT NOW(),
      ctime TIMESTAMP DEFAULT NOW(),
      crtime TIMESTAMP DEFAULT NOW(),
      blocks BIGINT DEFAULT 0,
      blksize INTEGER DEFAULT 512,
      nlink INTEGER DEFAULT 1
  );

  -- Indici per performance
  CREATE INDEX idx_fs_nodes_path ON fs_nodes(path);
  CREATE INDEX idx_fs_nodes_parent ON fs_nodes(parent_ino);
  CREATE INDEX idx_fs_nodes_name ON fs_nodes(name);

  -- Tabella contenuto file (per file piccoli < 1MB)
  CREATE TABLE fs_content (
      ino BIGINT PRIMARY KEY REFERENCES fs_nodes(ino) ON DELETE CASCADE,
      content BYTEA NOT NULL
  );

  -- Tabella chunks (per file grandi >= 1MB)
  CREATE TABLE fs_chunks (
      ino BIGINT REFERENCES fs_nodes(ino) ON DELETE CASCADE,
      chunk_index INTEGER NOT NULL,
      chunk_size INTEGER NOT NULL,
      chunk_data BYTEA NOT NULL,
      created_at TIMESTAMP DEFAULT NOW(),
      PRIMARY KEY (ino, chunk_index)
  );
  ```

### 🔧 **Refactoring e Miglioramenti Tecnici**

#### **Server API - Possibili Miglioramenti**
```typescript
// Attuale: Mix di endpoint spec-compliant e FUSE-specific
// Futuro: Organizzazione più pulita

// Endpoint Spec-Compliant (da mantenere)
app.get('/list', ...)      // ✅ Spec requirement
app.get('/files', ...)     // ✅ Spec requirement  
app.put('/files', ...)     // ✅ Spec requirement
app.post('/files', ...)    // ✅ Nuovo per creazione file
app.post('/mkdir', ...)    // ✅ Spec requirement
app.delete('/files', ...)  // ✅ Spec requirement

// Endpoint FUSE Support (da raggruppare sotto /fs/)
app.get('/fs/metadata', ...)     // Era /metadata
app.patch('/fs/metadata', ...)   // Era /metadata  
app.post('/fs/open', ...)        // Era /open
app.post('/fs/rename', ...)      // Era /rename
app.get('/fs/resolve-inode/:ino', ...)  // Era /resolve-inode/:ino
```

#### **Client Cache Architecture**
```rust
// Architettura cache proposta
pub struct RemoteFsClient {
    api_url: String,
    cache: Option<ClientCache>,  // Cache opzionale configurabile
}

impl RemoteFsClient {
    pub fn with_cache(api_url: String, cache_config: CacheConfig) -> Self {
        let cache = ClientCache::new(cache_config);
        Self { api_url, cache: Some(cache) }
    }
    
    pub fn without_cache(api_url: String) -> Self {
        Self { api_url, cache: None }
    }
}
```

### 🧪 **Testing Strategy (Da Implementare)**
```bash
# Test Categories da aggiungere:
tests/
├── unit/           # Test singole funzioni FUSE
├── integration/    # Test client-server end-to-end  
├── performance/    # Test di carico e stress
└── compatibility/ # Test con vari tool Unix (cp, mv, etc.)
```

---

**Sviluppatori:** Davide Carletto & Michele Carena  