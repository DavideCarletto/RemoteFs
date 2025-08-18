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




// List directory content endpoint
app.get('/list', (req, res) => {
  console.log('[LISTDIR] Received request:', req.query);
  
  const path = req.query.path as string;

  if (!path) {
    console.error('[LISTDIR] Missing path parameter');
    return res.status(400).json({ error: 'Path parameter is required' });
  }

  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      console.error('[LISTDIR] Directory not found:', path);
      return res.status(404).json({ error: 'Directory not found' });
    }

    if (metadata.file_type !== 'Directory') {
      console.error('[LISTDIR] Path is not a directory:', path, 'type:', metadata.file_type);
      return res.status(400).json({ error: 'Path is not a directory' });
    }

    const entries = sqliteBackend.listDirectory(path);
    console.log(`[LISTDIR] Directory ${path} contains ${entries.length} entries`);
    
    res.json({ entries });
  } catch (err) {
    console.error('[LISTDIR] Error:', err);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Read file content endpoint (unified for all file sizes)
app.get('/files', (req, res) => {
  console.log('[READ] Received request:', JSON.stringify(req.body, null, 2));
  
  const { path, file_handle, offset, size, chunk_index } = req.body;

  if (!path || file_handle === undefined) {
    console.error('[READ] Missing required parameters:', { path, file_handle });
    return res.status(400).json({ error: 'Path and file handle are required' });
  }

  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      console.error('[READ] File not found:', path);
      return res.status(404).json({ error: 'File not found' });
    }

    if (metadata.file_type !== 'RegularFile') {
      console.error('[READ] Cannot read non-regular file:', path, 'type:', metadata.file_type);
      return res.status(400).json({ error: 'Cannot read directory or special file' });
    }

    const startOffset = Math.max(0, offset || 0);
    const requestedSize = size || (metadata.size - startOffset);
    
    const content = sqliteBackend.readFile(path, startOffset, requestedSize);
    
    if (chunk_index !== undefined) {
      console.log(`[READ] Chunk ${chunk_index}: ${content.length} bytes from offset ${startOffset} (file size: ${metadata.size})`);
    } else {
      console.log(`[READ] Reading ${content.length} bytes from ${path} (offset: ${startOffset}, file size: ${metadata.size})`);
    }

    res.set('Content-Type', 'application/octet-stream');
    res.send(content);
  } catch (err) {
    console.error('[READ] Error:', err);
    res.status(500).json({ error: 'Internal server error' });
  }
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

  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      console.error('[WRITE] File not found:', path);
      return res.status(404).json({ error: 'File not found' });
    }

    if (metadata.file_type !== 'RegularFile') {
      console.error('[WRITE] Cannot write to non-regular file:', path, 'type:', metadata.file_type);
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
      try {
        const bytesWritten = sqliteBackend.writeFile(path, data, offset);
        const updatedMetadata = sqliteBackend.getFileMetadataByPath(path);
        
        console.log(`[WRITE] Completed streaming write: ${bytesWritten} bytes to ${path} at offset ${offset}, new size: ${updatedMetadata?.size}`);
        
        res.json({ 
          bytes_written: bytesWritten,
          new_size: updatedMetadata?.size,
          message: 'Streaming write successful'
        });
      } catch (err) {
        console.error('[WRITE] Error writing file:', err);
        res.status(500).json({ error: 'Error writing file data' });
      }
    });

    req.on('error', (error) => {
      console.error('[WRITE] Error during streaming write:', error);
      res.status(500).json({ error: 'Error processing streaming file data' });
    });
  } catch (err) {
    console.error('[WRITE] Error:', err);
    res.status(500).json({ error: 'Internal server error' });
  }
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

  try {
    // Check if file already exists
    const existingFile = sqliteBackend.getFileMetadataByPath(path);
    if (existingFile) {
      console.log(`❌ File già esistente: ${path}`);
      return res.status(409).json({ error: "File già esistente" });
    }
    
    // Get parent directory's inode
    const parentPath = path.substring(0, path.lastIndexOf('/')) || '/';
    const parentIno = sqliteBackend.getInodeByPath(parentPath);
    if (!parentIno) {
      console.log(`❌ Directory padre non trovata: ${parentPath}`);
      return res.status(404).json({ error: "Directory padre non trovata" });
    }
    
    const name = path.split('/').pop() || path;
    const ino = sqliteBackend.createFile({
      path,
      parent_ino: parentIno,
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

// Endpoint per creare directory
app.post("/mkdir", (req, res) => {
  const { path, file_type, mode, uid, gid, rdev, umask } = req.body;
  
  console.log(`📝 Richiesta creazione directory: ${path}, tipo: ${file_type}`);
  console.log(`   - Mode: ${mode}, UID: ${uid}, GID: ${gid}, rdev: ${rdev}, umask: ${umask}`);
  
  // Validazione input
  if (!path || !file_type) {
    console.log(`❌ Parametri mancanti: path=${path}, file_type=${file_type}`);
    return res.status(400).json({ error: "Path e file_type sono richiesti" });
  }
  
  // Solo per directory in questo endpoint
  if (file_type !== "Directory") {
    console.log(`❌ Tipo file non supportato in /mkdir: ${file_type}`);
    return res.status(400).json({ error: "Questo endpoint supporta solo Directory" });
  }

  try {
    // Check if directory already exists
    const existingDir = sqliteBackend.getFileMetadataByPath(path);
    if (existingDir) {
      console.log(`❌ Directory già esistente: ${path}`);
      return res.status(409).json({ error: "Directory già esistente" });
    }
    
    // Get parent directory's inode
    const parentPath = path.substring(0, path.lastIndexOf('/')) || '/';
    const parentIno = sqliteBackend.getInodeByPath(parentPath);
    if (!parentIno) {
      console.log(`❌ Directory padre non trovata: ${parentPath}`);
      return res.status(404).json({ error: "Directory padre non trovata" });
    }
    
    const name = path.split('/').pop() || path;
    const ino = sqliteBackend.createDirectory({
      path,
      parent_ino: parentIno,
      name,
      mode,
      uid,
      gid
    });
    
    const metadata = sqliteBackend.getFileMetadataByIno(ino);
    res.status(201).json(metadata);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    res.status(500).json({ error: "Errore creazione directory", details: message });
  }
});

// Endpoint per rimuovere file/directory
app.delete("/files", (req, res) => {
  const path = req.query.path as string;
  const isDirectory = req.query.is_directory === 'true';
  
  console.log(`🗑️ Richiesta rimozione: ${path} (directory: ${isDirectory})`);
  
  // Validazione input
  if (!path) {
    console.log(`❌ Path mancante nella richiesta di rimozione`);
    return res.status(400).json({ error: "Path richiesto" });
  }
  
  try {
    const fileToRemove = sqliteBackend.getFileMetadataByPath(path);
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
    
    const result = sqliteBackend.deleteNode(path, isDirectory);
    if (!result.success) {
      switch (result.error) {
        case 'no_such_file_or_directory':
          console.log(`❌ File non trovato per rimozione: ${path}`);
          return res.status(404).json({ error: "File non trovato" });
        case 'not_a_directory':
          console.log(`❌ Path non è una directory: ${path}`);
          return res.status(400).json({ error: "Non è una directory" });
        case 'is_a_directory':
          console.log(`❌ Path è una directory: ${path}`);
          return res.status(400).json({ error: "È una directory, usa rmdir" });
        case 'directory_not_empty':
          console.log(`❌ Directory non vuota: ${path}`);
          return res.status(409).json({ error: "Directory non vuota" });
        default:
          console.error(`❌ Errore durante la rimozione:`, result.error);
          return res.status(500).json({ error: "Errore durante la rimozione" });
      }
    }
    
    console.log(`✅ Filesystem object rimosso: ${path} (inode ${fileToRemove.ino})`);
    res.status(200).json({ message: "Rimosso con successo" });
  } catch (err) {
    console.error(`❌ Errore durante la rimozione:`, err);
    res.status(500).json({ error: "Errore durante la rimozione" });
  }
});

// Endpoint per aprire file
app.post("/open", (req, res) => {
  const { path, flags } = req.body;
  
  console.log(`📂 Richiesta apertura file: ${path}, flags: ${flags}`);
  
  // Validazione input
  if (!path) {
    console.log(`❌ Path mancante nella richiesta di apertura`);
    return res.status(400).json({ error: "Path richiesto" });
  }
  
  try {
    const file = sqliteBackend.getFileMetadataByPath(path);
    if (!file) {
      console.log(`❌ File non trovato per apertura: ${path}`);
      return res.status(404).json({ error: "File non trovato" });
    }
    
    // Controlla che non sia una directory (a meno che non sia opendir)
    if (file.file_type === "Directory") {
      console.log(`❌ Tentativo di open su directory: ${path}`);
      return res.status(400).json({ error: "È una directory, usa opendir" });
    }
    
    const fileHandle = sqliteBackend.openFile({ path, flags });
    console.log(`✅ File aperto: ${path} -> file handle ${fileHandle.file_handle}`);
    res.status(200).json(fileHandle);
  } catch (err) {
    console.error(`❌ Errore durante l'apertura:`, err);
    res.status(500).json({ error: "Errore durante l'apertura del file" });
  }
});

// Endpoint per rinominare/spostare file e directory
app.post('/rename', (req, res) => {
  console.log('[RENAME] Received request:', JSON.stringify(req.body, null, 2));
  
  const { old_path, new_path } = req.body;
  
  if (!old_path || !new_path) {
    console.error('[RENAME] Missing required parameters:', { old_path, new_path });
    return res.status(400).json({ error: 'Both old_path and new_path are required' });
  }

  try {
    const sourceFile = sqliteBackend.getFileMetadataByPath(old_path);
    if (!sourceFile) {
      console.error(`[RENAME] Source file not found: ${old_path}`);
      return res.status(404).json({ error: 'Source file not found' });
    }

    const destFile = sqliteBackend.getFileMetadataByPath(new_path);
    if (destFile) {
      console.error(`[RENAME] Destination already exists: ${new_path}`);
      return res.status(409).json({ error: 'Destination already exists' });
    }

    const newParentPath = new_path.substring(0, new_path.lastIndexOf('/')) || '/';
    const parentIno = sqliteBackend.getInodeByPath(newParentPath);
    if (!parentIno) {
      console.error(`[RENAME] Parent directory not found: ${newParentPath}`);
      return res.status(404).json({ error: 'Parent directory not found' });
    }

    if (sourceFile.file_type === 'Directory' && new_path.startsWith(old_path + '/')) {
      console.error(`[RENAME] Cannot move directory into itself: ${old_path} -> ${new_path}`);
      return res.status(400).json({ error: 'Cannot move directory into itself' });
    }

    sqliteBackend.renameNode(old_path, new_path);
    const updatedMetadata = sqliteBackend.getFileMetadataByPath(new_path);

    console.log(`[RENAME] Successfully moved: ${old_path} -> ${new_path}`);
    res.json({ 
      message: 'File renamed successfully',
      old_path,
      new_path,
      metadata: updatedMetadata
    });
  } catch (err) {
    console.error('[RENAME] Error:', err);
    res.status(500).json({ error: 'Internal server error' });
  }
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
  
  try {
    const path = sqliteBackend.resolveInode(ino);
    if (!path) {
      console.log(`❌ Inode ${ino} non trovato`);
      return res.status(404).json({ error: "Inode non trovato" });
    }
    
    console.log(`✅ Inode ${ino} risolto in: ${path}`);
    res.send(path);
  } catch (err) {
    console.error(`❌ Errore durante la risoluzione dell'inode:`, err);
    res.status(500).json({ error: "Errore durante la risoluzione dell'inode" });
  }
});

// Endpoint per ottenere metadati di un file
app.get("/metadata", (req, res) => {
  const path = req.query.path as string;
  
  console.log(`📋 Richiesta metadati per: ${path}`);
  
  if (!path) {
    console.log(`❌ Path mancante nella richiesta`);
    return res.status(400).json({ error: "Path richiesto" });
  }
  
  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      console.log(`❌ File non trovato: ${path}`);
      return res.status(404).json({ error: "File non trovato" });
    }
    
    console.log(`✅ Metadati trovati per ${path}: inode ${metadata.ino}, tipo ${metadata.file_type}`);
    res.json(metadata);
  } catch (err) {
    console.error(`❌ Errore durante il recupero dei metadati:`, err);
    res.status(500).json({ error: "Errore durante il recupero dei metadati" });
  }
});

// Endpoint per aggiornare metadati di un file
app.patch("/metadata", (req, res) => {
  console.log('[PATCH METADATA] Received request:', JSON.stringify(req.body, null, 2));
  
  const path = req.query.path as string;
  if (!path) {
    console.error('[PATCH METADATA] Missing path parameter');
    return res.status(400).json({ error: "Path parameter is required" });
  }

  try {
    const updatedMetadata = sqliteBackend.updateMetadata(path, req.body);
    if (!updatedMetadata) {
      console.error(`[PATCH METADATA] File not found: ${path}`);
      return res.status(404).json({ error: "File not found" });
    }

    console.log(`[PATCH METADATA] Updated metadata for: ${path}`);
    res.json(updatedMetadata);
  } catch (err) {
    console.error('[PATCH METADATA] Error:', err);
    res.status(500).json({ error: 'Internal server error' });
  }
});


app.listen(PORT, '0.0.0.0', () => {
  console.log(`🚀 Server avviato su http://localhost:${PORT}`);
  console.log(` Endpoint disponibili:`);
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

export { app };
  