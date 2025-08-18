import express from "express";
import { SQLiteBackend } from "./sqlite-backend";

process.on('uncaughtException', err => console.error('Uncaught Exception:', err));
process.on('unhandledRejection', err => console.error('Unhandled Rejection:', err));

const app = express();
const PORT = 3000;
const sqliteBackend = new SQLiteBackend("./data/fs.sqlite");

app.use(express.json());

export interface INode {
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

const testContent = Buffer.from("Questo è il contenuto del file test.txt\nSeconda riga di esempio\n");
const readmeContent = Buffer.from(`# Remote Filesystem
    
Questo è un esempio di file markdown nel filesystem remoto.

## Caratteristiche
- Lettura e scrittura streaming
- Gestione di file di qualsiasi dimensione
- API HTTP RESTful

## Esempio di utilizzo
\`\`\`bash
mount -t fuse ./remote_fs /tmp/remote-fs
\`\`\`
`);

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
    size: testContent.length,
    file_type: "RegularFile",
    permissions: 0o644,
    nlink: 1,
    uid: 1000,
    gid: 1000,
    atime: Math.floor(Date.now() / 1000),
    mtime: Math.floor(Date.now() / 1000),
    ctime: Math.floor(Date.now() / 1000),
    blocks: Math.ceil(testContent.length / 512),
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
    size: readmeContent.length,
    file_type: "RegularFile",
    permissions: 0o644,
    nlink: 1,
    uid: 1000,
    gid: 1000,
    atime: Math.floor(Date.now() / 1000),
    mtime: Math.floor(Date.now() / 1000),
    ctime: Math.floor(Date.now() / 1000),
    blocks: 0,
    blksize: 512
  }
};

// Mappa inversa: inode -> path
const inodeToPath: { [ino: number]: string } = {};
Object.values(fileSystem).forEach(file => {
  inodeToPath[file.ino] = file.path;
});


// List directory content endpoint
app.get('/list', (req, res) => {
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

// Read file content endpoint (unified for all file sizes)
app.get('/files', (req, res) => {
  console.log('[READ] Received request:', JSON.stringify(req.body, null, 2));
  
  const { path, file_handle, offset, size, chunk_index } = req.body;

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

  const startOffset = Math.max(0, offset || 0);
  
  // Genera contenuto mock basato sui metadati del file
  // In futuro questo sarà sostituito dalla lettura dal filesystem fisico
  let mockContent = Buffer.from("Questo è il contenuto del file\nSeconda riga di esempio\n");

  const actualSize = Math.min(mockContent.length, node.size);
  const requestedSize = size || (actualSize - startOffset);
  const endOffset = Math.min(actualSize, startOffset + requestedSize);
  const bytesToRead = Math.max(0, endOffset - startOffset);

  if (chunk_index !== undefined) {
    console.log(`[READ] Chunk ${chunk_index}: ${bytesToRead} bytes from offset ${startOffset} (file size: ${actualSize}, mock)`);
  } else {
    console.log(`[READ] Reading ${bytesToRead} bytes from ${path} (offset: ${startOffset}, file size: ${actualSize}, mock)`);
  }

  const content = mockContent.subarray(startOffset, endOffset);
  res.set('Content-Type', 'application/octet-stream');
  res.send(content);

});

// Write file content endpoint (unified streaming for all file sizes)
app.put('/files', (req, res) => {
  console.log('[WRITE] Received streaming write request');
  
  const path = req.headers['x-path'] as string;
  const file_handle = req.headers['x-file-handle'] as string;
  const offset = parseInt(req.headers['x-offset'] as string);

  if (!path || !file_handle || isNaN(offset)) {
    console.error('[WRITE] Missing required headers:', { path, file_handle, offset });
    return res.status(400).json({ error: 'Required headers: x-path, x-file-handle, x-offset' });
  }

  const node = fileSystem[path];
  if (!node) {
    console.error('[WRITE] File not found:', path);
    return res.status(404).json({ error: 'File not found' });
  }

  if (node.file_type !== 'RegularFile') {
    console.error('[WRITE] Cannot write to non-regular file:', path, 'type:', node.file_type);
    return res.status(400).json({ error: 'Cannot write to directory or special file' });
  }

  const chunks: Buffer[] = [];
  let totalBytesReceived = 0;
  
  req.on('data', (chunk: Buffer) => {
    chunks.push(chunk);
    totalBytesReceived += chunk.length;
    
    if (totalBytesReceived % (10 * 1024 * 1024) === 0) {
      console.log(`[WRITE] Received ${Math.round(totalBytesReceived / 1024 / 1024)}MB for ${path}`);
    }
  });

  req.on('end', () => {
    const data = Buffer.concat(chunks);
    const newSize = Math.max(node.size, offset + data.length);
    
    // Aggiorna metadati del file
    node.size = newSize;
    node.mtime = Math.floor(Date.now() / 1000);
    node.blocks = Math.ceil(newSize / 512);

    console.log(`[WRITE] Completed streaming write: ${data.length} bytes to ${path} at offset ${offset}, new size: ${newSize}`);
    
    res.json({ 
      bytes_written: data.length,
      new_size: newSize,
      message: 'Streaming write successful'
    });
  });

  req.on('error', (error) => {
    console.error('[WRITE] Error during streaming write:', error);
    res.status(500).json({ error: 'Error processing streaming file data' });
  });
});

// Endpoint per creare file regolari
app.post('/files', (req, res) => {
  const { path, file_type, mode, uid, gid, rdev, umask } = req.body;
  
  console.log(`📝 Richiesta creazione file: ${path}, tipo: ${file_type}`);
  console.log(`   - Mode: ${mode}, UID: ${uid}, GID: ${gid}, rdev: ${rdev}, umask: ${umask}`);
  
  // Validazione input
  if (!path || !file_type) {
    console.log(`❌ Parametri mancanti: path=${path}, file_type=${file_type}`);
    return res.status(400).json({ error: "Path e file_type sono richiesti" });
  }
  
  // Solo per file regolari in questo endpoint
  if (file_type !== "RegularFile") {
    console.log(`❌ Tipo file non supportato in /api/files: ${file_type}`);
    return res.status(400).json({ error: "Questo endpoint supporta solo RegularFile" });
  }
  
  // Controlla se il file esiste già
  if (fileSystem[path]) {
    console.log(`❌ File già esistente: ${path}`);
    return res.status(409).json({ error: "File già esistente" });
  }
  
  // Controlla se la directory padre esiste
  const parentPath = path.substring(0, path.lastIndexOf('/')) || '/';
  if (parentPath !== '/' && !fileSystem[parentPath]) {
    console.log(`❌ Directory padre non trovata: ${parentPath}`);
    return res.status(404).json({ error: "Directory padre non trovata" });
  }
  
  const name = path.split('/').pop() || path;
  // Genera nuovo inode
  //const newIno = Math.max(...Object.values(fileSystem).map(f => f.ino)) + 1;
  let parent_ino = 1; //root  //bisogna implementare getinodeByPath
  try {
    const ino = sqliteBackend.createFile({
      path,
      parent_ino,
      name,
      mode,
      uid,
      gid
    });
     const metadata = sqliteBackend.getFileMetadataByIno(ino);
    res.status(201).json(metadata);
  } catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  res.status(500).json({ error: "Errore creazione file", details: message });
}
  
});

// Endpoint mock per creare directory
app.post("/mkdir", (req, res) => {
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
  
  // I file regolari avranno contenuto fisico sul server (non qui nei metadati)
  
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
app.delete("/files", (req, res) => {
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


// Endpoint per rinominare/spostare file e directory
app.post('/rename', (req, res) => {
  console.log('[RENAME] Received request:', JSON.stringify(req.body, null, 2));
  
  const { old_path, new_path } = req.body;
  
  if (!old_path || !new_path) {
    console.error('[RENAME] Missing required parameters:', { old_path, new_path });
    return res.status(400).json({ error: 'Both old_path and new_path are required' });
  }

  const sourceFile = fileSystem[old_path];
  if (!sourceFile) {
    console.error(`[RENAME] Source file not found: ${old_path}`);
    return res.status(404).json({ error: 'Source file not found' });
  }

  if (fileSystem[new_path]) {
    console.error(`[RENAME] Destination already exists: ${new_path}`);
    return res.status(409).json({ error: 'Destination already exists' });
  }

  const newParentPath = new_path.substring(0, new_path.lastIndexOf('/')) || '/';
  if (newParentPath !== '/' && !fileSystem[newParentPath]) {
    console.error(`[RENAME] Parent directory not found: ${newParentPath}`);
    return res.status(404).json({ error: 'Parent directory not found' });
  }

  if (sourceFile.file_type === 'Directory' && new_path.startsWith(old_path + '/')) {
    console.error(`[RENAME] Cannot move directory into itself: ${old_path} -> ${new_path}`);
    return res.status(400).json({ error: 'Cannot move directory into itself' });
  }

  const updatedFile = { ...sourceFile, path: new_path };
  fileSystem[new_path] = updatedFile;
  delete fileSystem[old_path];
  
  inodeToPath[sourceFile.ino] = new_path;

  if (sourceFile.file_type === 'Directory') {
    const childrenToMove = [];
    for (const [path, node] of Object.entries(fileSystem)) {
      if (path.startsWith(old_path + '/')) {
        childrenToMove.push({ oldPath: path, node });
      }
    }
    
    for (const { oldPath, node } of childrenToMove) {
      const newChildPath = new_path + oldPath.slice(old_path.length);
      const updatedChild = { ...node, path: newChildPath };
      fileSystem[newChildPath] = updatedChild;
      delete fileSystem[oldPath];
      inodeToPath[node.ino] = newChildPath;
    }
    
    console.log(`[RENAME] Moved directory with ${childrenToMove.length} children`);
  }

  // Aggiorna timestamp
  updatedFile.mtime = Math.floor(Date.now() / 1000);
  updatedFile.ctime = updatedFile.mtime;

  console.log(`[RENAME] Successfully moved: ${old_path} -> ${new_path}`);
  res.json({ 
    message: 'File renamed successfully',
    old_path,
    new_path,
    metadata: updatedFile
  });
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

// Endpoint per aggiornare metadati di un file
app.patch("/metadata", (req, res) => {
  console.log('[PATCH METADATA] Received request:', JSON.stringify(req.body, null, 2));
  
  const path = req.query.path as string;
  if (!path) {
    console.error('[PATCH METADATA] Missing path parameter');
    return res.status(400).json({ error: "Path parameter is required" });
  }

  const file = fileSystem[path];
  if (!file) {
    console.error(`[PATCH METADATA] File not found: ${path}`);
    return res.status(404).json({ error: "File not found" });
  }

  // Update the provided fields
  const updates = req.body;
  if (updates.mode !== undefined && updates.mode !== null) {
    file.permissions = updates.mode;
  }
  if (updates.uid !== undefined && updates.uid !== null) {
    file.uid = updates.uid;
  }
  if (updates.gid !== undefined && updates.gid !== null) {
    file.gid = updates.gid;
  }
  if (updates.size !== undefined && updates.size !== null) {
    file.size = updates.size;
  }

  // Always update mtime when attributes change
  file.mtime = Math.floor(Date.now() / 1000);

  console.log(`[PATCH METADATA] Updated metadata for: ${path}`);
  res.json(file);
});


app.listen(PORT, () => {
  console.log(`🚀 Server avviato su http://localhost:${PORT}`);
  console.log(`📁 Mock filesystem caricato con ${Object.keys(fileSystem).length} file`);
  console.log(`🔧 Endpoint disponibili:`);
  console.log(`   - GET /list/ - Elenca il contenuto di una directory`);
  console.log(`   - GET /files/ - Legge il contenuto di un file`);
  console.log(`   - PUT /files/ - Scrive il contenuto di un file`);
  console.log(`   - POST /files/ - Crea un file regolare`);
  console.log(`   - POST /mkdir/ - Crea una directory`);
  console.log(`   - DELETE /files/ Cancella un file o una directory`);
  console.log(`   - GET /health - Health check`);
  console.log(`   - GET /resolve-inode/:ino - Risolve inode in path`);
  console.log(`   - GET /metadata - Ottiene metadati file`);
  console.log(`   - PATCH /metadata - Aggiorna metadati file`);
  console.log(`   - POST /open - Apre un file`);
  console.log(`   - POST /rename - Rinomina/sposta file/directory`);
});

export {app, fileSystem as mockFilesystem, inodeToPath };
  