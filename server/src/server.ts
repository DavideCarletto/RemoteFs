import express from "express";

process.on('uncaughtException', err => console.error('Uncaught Exception:', err));
process.on('unhandledRejection', err => console.error('Unhandled Rejection:', err));

const app = express();
const PORT = 3000;

app.use(express.json());

interface INode {
  ino: number;
  path: string;
  size: number;
  file_type: string;
  permissions: number;
  nlink: number;
  uid: number;
  gid: number;
  atime: number;
  mtime: number;
  ctime: number;
  blocks: number;
  blksize: number;
}

const fileSystem: { [path: string]: INode } = {
  "/": {
    ino: 1,
    path: "/",
    size: 4096,
    file_type: "Directory",
    permissions: 0o755,
    nlink: 2,
    uid: 1000,
    gid: 1000,
    atime: Math.floor(Date.now() / 1000),
    mtime: Math.floor(Date.now() / 1000),
    ctime: Math.floor(Date.now() / 1000),
    blocks: 8,
    blksize: 512
  },
  "/test.txt": {
    ino: 2,
    path: "/test.txt",
    size: 70,
    file_type: "RegularFile",
    permissions: 0o644,
    nlink: 1,
    uid: 1000,
    gid: 1000,
    atime: Math.floor(Date.now() / 1000),
    mtime: Math.floor(Date.now() / 1000),
    ctime: Math.floor(Date.now() / 1000),
    blocks: 1,
    blksize: 512
  },
  "/documents": {
    ino: 3,
    path: "/documents",
    size: 4096,
    file_type: "Directory", 
    permissions: 0o755,
    nlink: 2,
    uid: 1000,
    gid: 1000,
    atime: Math.floor(Date.now() / 1000),
    mtime: Math.floor(Date.now() / 1000),
    ctime: Math.floor(Date.now() / 1000),
    blocks: 8,
    blksize: 512
  },
  "/documents/readme.md": {
    ino: 4,
    path: "/documents/readme.md",
    size: 256,
    file_type: "RegularFile",
    permissions: 0o644,
    nlink: 1,
    uid: 1000,
    gid: 1000,
    atime: Math.floor(Date.now() / 1000),
    mtime: Math.floor(Date.now() / 1000),
    ctime: Math.floor(Date.now() / 1000),
    blocks: 1,
    blksize: 512
  }
};

// Mappa inversa: inode -> path
const inodeToPath: { [ino: number]: string } = {};
Object.values(fileSystem).forEach(file => {
  inodeToPath[file.ino] = file.path;
});

app.get("/health", (req, res) => {
  res.json({ status: "ok" });
});

// Endpoint per risolvere inode -> path
app.get("/resolve-inode/:ino", (req, res) => {
  const ino = parseInt(req.params.ino);
  
  console.log(`🔍 Richiesta risoluzione inode: ${ino}`);
  
  if (isNaN(ino)) {
    console.log(`❌ Inode non valido: ${req.params.ino}`);
    return res.status(400).json({ error: "Inode non valido" });
  }
  
  const path = inodeToPath[ino];
  if (!path) {
    console.log(`❌ Inode ${ino} non trovato`);
    return res.status(404).json({ error: "Inode non trovato" });
  }
  
  console.log(`✅ Inode ${ino} risolto in: ${path}`);
  res.send(path);
});

// Endpoint per ottenere metadati di un file
app.get("/metadata", (req, res) => {
  const path = req.query.path as string;
  
  console.log(`📋 Richiesta metadati per: ${path}`);
  
  if (!path) {
    console.log(`❌ Path mancante nella richiesta`);
    return res.status(400).json({ error: "Path richiesto" });
  }
  
  const file = fileSystem[path];
  if (!file) {
    console.log(`❌ File non trovato: ${path}`);
    return res.status(404).json({ error: "File non trovato" });
  }
  
  console.log(`✅ Metadati trovati per ${path}: inode ${file.ino}, tipo ${file.file_type}`);
  res.json(file);
});

// Endpoint per debug: lista tutti i file mock
app.get("/debug/files", (req, res) => {
  res.json({
    filesystem: fileSystem,
    inodeMap: inodeToPath
  });
});

// Endpoint mock per creare file
app.post("/create", (req, res) => {
  const { path, file_type, mode, uid, gid, rdev, umask } = req.body;
  
  console.log(`📝 Richiesta creazione file: ${path}, tipo: ${file_type}`);
  console.log(`   - Mode: ${mode}, UID: ${uid}, GID: ${gid}, rdev: ${rdev}, umask: ${umask}`);
  
  // Validazione input
  if (!path || !file_type) {
    console.log(`❌ Parametri mancanti: path=${path}, file_type=${file_type}`);
    return res.status(400).json({ error: "Path e file_type sono richiesti" });
  }
  
  // Controlla se il file esiste già
  if (fileSystem[path]) {
    console.log(`❌ File già esistente: ${path}`);
    return res.status(409).json({ error: "File già esistente" });
  }
  
  // Controlla se la directory padre esiste (semplicistico)
  const parentPath = path.substring(0, path.lastIndexOf('/')) || '/';
  if (parentPath !== '/' && !fileSystem[parentPath]) {
    console.log(`❌ Directory padre non trovata: ${parentPath}`);
    return res.status(404).json({ error: "Directory padre non trovata" });
  }
  
  // Genera nuovo inode (semplice incremento)
  const newIno = Math.max(...Object.values(fileSystem).map(f => f.ino)) + 1;
  
  // Determina la dimensione di default in base al tipo
  let defaultSize = 0;
  if (file_type === "Directory") {
    defaultSize = 4096;
  } else if (file_type === "RegularFile") {
    defaultSize = 0; // File vuoto
  }
  
  // Crea il nuovo file mock
  const newFile: INode = {
    ino: newIno,
    path: path,
    size: defaultSize,
    file_type: file_type,
    permissions: mode || 0o644,
    nlink: file_type === "Directory" ? 2 : 1,
    uid: uid || 1000,
    gid: gid || 1000,
    atime: Math.floor(Date.now() / 1000),
    mtime: Math.floor(Date.now() / 1000),
    ctime: Math.floor(Date.now() / 1000),
    blocks: Math.ceil(defaultSize / 512),
    blksize: 512
  };
  
  // Aggiungi al mock filesystem
  fileSystem[path] = newFile;
  inodeToPath[newIno] = path;
  
  console.log(`✅ File creato: ${path} -> inode ${newIno}, tipo ${file_type}`);
  
  // Ritorna i metadati del file creato (compatibile con FileMetadata Rust)
  res.status(201).json({
    ino: newFile.ino,
    size: newFile.size,
    blocks: newFile.blocks,
    atime: newFile.atime,
    mtime: newFile.mtime,
    ctime: newFile.ctime,
    crtime: newFile.ctime, // Creation time = change time per semplicità
    file_type: newFile.file_type,
    permissions: newFile.permissions,
    nlink: newFile.nlink,
    uid: newFile.uid,
    gid: newFile.gid,
    blksize: newFile.blksize,
    flags: null
  });
});

// Endpoint mock per rimuovere file/directory
app.delete("/remove", (req, res) => {
  const path = req.query.path as string;
  const isDirectory = req.query.is_directory === 'true';
  
  console.log(`🗑️ Richiesta rimozione: ${path} (directory: ${isDirectory})`);
  
  // Validazione input
  if (!path) {
    console.log(`❌ Path mancante nella richiesta di rimozione`);
    return res.status(400).json({ error: "Path richiesto" });
  }
  
  // Controlla se il file/directory esiste
  const fileToRemove = fileSystem[path];
  if (!fileToRemove) {
    console.log(`❌ File non trovato per rimozione: ${path}`);
    return res.status(404).json({ error: "File non trovato" });
  }
  
  // Verifica coerenza tipo (directory vs file)
  const isActuallyDirectory = fileToRemove.file_type === "Directory";
  if (isDirectory && !isActuallyDirectory) {
    console.log(`❌ Tentativo di rmdir su file normale: ${path}`);
    return res.status(400).json({ error: "Non è una directory" });
  }
  
  if (!isDirectory && isActuallyDirectory) {
    console.log(`❌ Tentativo di unlink su directory: ${path}`);
    return res.status(400).json({ error: "È una directory, usa rmdir" });
  }
  
  // Se è una directory, controlla che sia vuota
  if (isDirectory) {
    const hasChildren = Object.keys(fileSystem).some(p => 
      p !== path && p.startsWith(path + '/') && p.indexOf('/', path.length + 1) === -1
    );
    
    if (hasChildren) {
      console.log(`❌ Directory non vuota: ${path}`);
      return res.status(409).json({ error: "Directory non vuota" });
    }
  }
  
  // Rimuovi dal mock filesystem
  delete fileSystem[path];
  delete inodeToPath[fileToRemove.ino];
  
  console.log(`✅ Filesystem object rimosso: ${path} (inode ${fileToRemove.ino})`);
  
  res.status(200).json({ message: "Rimosso con successo" });
});

// Endpoint mock per aprire file
app.post("/open", (req, res) => {
  const { path, flags } = req.body;
  
  console.log(`📂 Richiesta apertura file: ${path}, flags: ${flags}`);
  
  // Validazione input
  if (!path) {
    console.log(`❌ Path mancante nella richiesta di apertura`);
    return res.status(400).json({ error: "Path richiesto" });
  }
  
  // Controlla se il file esiste
  const file = fileSystem[path];
  if (!file) {
    console.log(`❌ File non trovato per apertura: ${path}`);
    return res.status(404).json({ error: "File non trovato" });
  }
  
  // Controlla che non sia una directory (a meno che non sia opendir)
  if (file.file_type === "Directory") {
    console.log(`❌ Tentativo di open su directory: ${path}`);
    return res.status(400).json({ error: "È una directory, usa opendir" });
  }
  
  // Genera un file handle unico (semplice incremento basato su timestamp)
  const fileHandle = Date.now() + Math.floor(Math.random() * 1000);
  
  // In un'implementazione reale, qui salveresti lo stato del file aperto
  // Per ora simulo solo la risposta
  
  console.log(`✅ File aperto: ${path} -> file handle ${fileHandle}`);
  
  res.status(200).json({
    file_handle: fileHandle,
    flags: flags,
    path: path
  });
});

// Read file content endpoint  
app.post('/read', (req, res) => {
  console.log('[READ] Received request:', JSON.stringify(req.body, null, 2));
  
  const { path, file_handle, offset, size } = req.body;

  if (!path || file_handle === undefined) {
    console.error('[READ] Missing required parameters:', { path, file_handle });
    return res.status(400).json({ error: 'Path and file handle are required' });
  }

  const node = fileSystem[path];
  if (!node) {
    console.error('[READ] File not found:', path);
    return res.status(404).json({ error: 'File not found' });
  }

  if (node.file_type !== 'RegularFile') {
    console.error('[READ] Cannot read non-regular file:', path, 'type:', node.file_type);
    return res.status(400).json({ error: 'Cannot read directory or special file' });
  }

  const fileContent = Buffer.from(`Content of file: ${path}\nThis is line 2\nThis is line 3\nEnd of file.\n`);
  
  const startOffset = Math.max(0, offset);
  const endOffset = Math.min(fileContent.length, startOffset + size);
  const data = fileContent.slice(startOffset, endOffset);

  console.log(`[READ] Reading ${data.length} bytes from ${path} (offset: ${offset}, size: ${size})`);
  
  res.set('Content-Type', 'application/octet-stream');
  res.send(data);
});

// List directory content endpoint
app.get('/listdir', (req, res) => {
  console.log('[LISTDIR] Received request:', req.query);
  
  const path = req.query.path as string;

  if (!path) {
    console.error('[LISTDIR] Missing path parameter');
    return res.status(400).json({ error: 'Path parameter is required' });
  }

  const node = fileSystem[path];
  if (!node) {
    console.error('[LISTDIR] Directory not found:', path);
    return res.status(404).json({ error: 'Directory not found' });
  }

  if (node.file_type !== 'Directory') {
    console.error('[LISTDIR] Path is not a directory:', path, 'type:', node.file_type);
    return res.status(400).json({ error: 'Path is not a directory' });
  }

  const entries = [];
  const searchPrefix = path === '/' ? '/' : path + '/';
  
  for (const [childPath, childNode] of Object.entries(fileSystem)) {
    if (childPath === path) continue;
    
    if (path === '/') {
      if (childPath.startsWith('/') && childPath !== '/' && !childPath.slice(1).includes('/')) {
        entries.push({
          name: childPath.slice(1),
          ino: childNode.ino,
          file_type: childNode.file_type
        });
      }
    } else {
      if (childPath.startsWith(searchPrefix)) {
        const relativePath = childPath.slice(searchPrefix.length);
        if (!relativePath.includes('/')) {
          entries.push({
            name: relativePath,
            ino: childNode.ino,
            file_type: childNode.file_type
          });
        }
      }
    }
  }

  console.log(`[LISTDIR] Directory ${path} contains ${entries.length} entries`);
  
  res.json({ entries });
});

app.listen(PORT, () => {
  console.log(`🚀 Server avviato su http://localhost:${PORT}`);
  console.log(`📁 Mock filesystem caricato con ${Object.keys(fileSystem).length} file`);
  console.log(`🔧 Endpoint disponibili:`);
  console.log(`   - GET /health - Health check`);
  console.log(`   - GET /resolve-inode/:ino - Risolve inode in path`);
  console.log(`   - GET /metadata?path=... - Ottiene metadati file`);
  console.log(`   - POST /create - Crea nuovo file/directory`);
  console.log(`   - POST /open - Apre un file`);
  console.log(`   - POST /read - Legge contenuto file`);
  console.log(`   - GET /listdir?path=... - Lista contenuto directory`);
  console.log(`   - DELETE /remove?path=...&is_directory=... - Rimuove file/directory`);
  console.log(`   - GET /debug/files - Lista tutti i file mock`);
});

export {app, fileSystem as mockFilesystem, inodeToPath };
  