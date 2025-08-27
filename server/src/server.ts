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

  const path = req.query.path as string;

  if (!path) {
    return res.status(400).json({ error: 'Path parameter is required' });
  }

  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      return res.status(404).json({ error: 'Directory not found' });
    }

    if (metadata.file_type !== 'Directory') {
      return res.status(400).json({ error: 'Path is not a directory' });
    }

    const entries = sqliteBackend.listDirectory(path);

    res.json({ entries });
  } catch (err) {
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Read file content endpoint
app.get('/files', (req, res) => {
  const path = req.query.path as string;
  const file_handle = req.query.file_handle as string;
  const offset = parseInt(req.query.offset as string) || 0;
  const size = parseInt(req.query.size as string) || 0;
  const chunk_index = parseInt(req.query.chunk_index as string) || 0;

  if (!path || !file_handle) {
    return res.status(400).json({ error: 'Path and file handle are required' });
  }

  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      return res.status(404).json({ error: 'File not found' });
    }

    if (metadata.file_type !== 'RegularFile') {
      return res.status(400).json({ error: 'Cannot read directory or special file' });
    }

    const startOffset = Math.max(0, offset);
    const requestedSize = size || (metadata.size - startOffset);

    // Calcola la dimensione effettiva da leggere (non può essere più grande del file)
    const actualEndOffset = Math.min(startOffset + requestedSize - 1, metadata.size - 1);
    const actualSize = Math.max(0, actualEndOffset - startOffset + 1);


    const readStream = sqliteBackend.readFile(path, {
      start: startOffset,
      end: actualEndOffset
    });

    if (!readStream) {
      return res.status(500).json({ error: 'Failed to create read stream' });
    }

    res.set('Content-Type', 'application/octet-stream');
    res.set('Content-Length', actualSize.toString());

    readStream.on('error', (error: Error) => {
      console.error(`[READ ERROR] ${path}:`, error);
      if (!res.headersSent) {
        res.status(500).json({ error: 'Stream read error' });
      }
    });

    readStream.pipe(res);
  } catch (err) {
    console.error(`[READ CATCH ERROR] ${path}:`, err);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Write file content endpoint
app.put('/files', (req, res) => {
  const path = req.headers['x-path'] as string;
  const file_handle = req.headers['x-file-handle'] as string;
  const offset = parseInt(req.headers['x-offset'] as string);
  const expectedSize = parseInt(req.headers['content-length'] as string) || 0;

  if (!path || !file_handle || isNaN(offset)) {
    return res.status(400).json({ error: 'Required headers: x-path, x-file-handle, x-offset' });
  }

  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      return res.status(404).json({ error: 'File not found' });
    }

    if (metadata.file_type !== 'RegularFile') {
      return res.status(400).json({ error: 'Cannot write to directory or special file' });
    }

    const writeStream = sqliteBackend.writeFile(path, { start: offset });
    if (!writeStream) {
      return res.status(500).json({ error: 'Failed to create write stream' });
    }

    let totalBytesWritten = 0;

    writeStream.on('error', (error) => {
      console.error(`[WRITE ERROR] ${path}:`, error);
      if (!res.headersSent) {
        res.status(500).json({ error: 'Stream write error' });
      }
    });

    writeStream.on('finish', () => {
      try {
        sqliteBackend.updateFileSize(path);
        const updates = {
          mtime: Math.floor(Date.now() / 1000)
        };
        sqliteBackend.updateMetadata(path, updates);

        const updatedMetadata = sqliteBackend.getFileMetadataByPath(path);

        if (!res.headersSent) {
          res.json({
            bytes_written: totalBytesWritten,
            new_size: updatedMetadata?.size,
            offset_written: offset,
            message: 'Write successful'
          });
        }
      } catch (err) {
        console.error(`[WRITE FINISH ERROR] ${path}:`, err);
        if (!res.headersSent) {
          res.status(500).json({ error: 'Error updating file metadata' });
        }
      }
    });

    req.on('data', (chunk: Buffer) => {
      totalBytesWritten += chunk.length;
      // Log dei primi 16 byte di ogni chunk ricevuto dal server
      const preview = chunk.subarray(0, 16);
      const hexPreview = Array.from(preview).map(b => b.toString(16).padStart(2, '0')).join(' ');
    });

    req.on('error', (error) => {
      console.error(`[WRITE REQ ERROR] ${path}:`, error);
      if (!res.headersSent) {
        res.status(500).json({ error: 'Error processing request' });
      }
    });

    req.pipe(writeStream);
  } catch (err) {
    console.error(`[WRITE CATCH ERROR] ${path}:`, err);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Endpoint per creare file regolari
app.post('/files', (req, res) => {
  const { path, file_type, mode, uid, gid, rdev, umask } = req.body;

  if (!path || !file_type) {
    return res.status(400).json({ error: "Path e file_type sono richiesti" });
  }

  if (file_type !== "RegularFile") {
    return res.status(400).json({ error: "Questo endpoint supporta solo RegularFile" });
  }

  try {
    const existingFile = sqliteBackend.getFileMetadataByPath(path);
    if (existingFile) {
      return res.status(409).json({ error: "File già esistente" });
    }

    const parentPath = path.substring(0, path.lastIndexOf('/')) || '/';
    const parentIno = sqliteBackend.getInodeByPath(parentPath);
    if (!parentIno) {
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

  if (!path || !file_type) {
    return res.status(400).json({ error: "Path e file_type sono richiesti" });
  }

  if (file_type !== "Directory") {
    return res.status(400).json({ error: "Questo endpoint supporta solo Directory" });
  }

  try {
    const existingDir = sqliteBackend.getFileMetadataByPath(path);
    if (existingDir) {
      return res.status(409).json({ error: "Directory già esistente" });
    }

    const parentPath = path.substring(0, path.lastIndexOf('/')) || '/';
    const parentIno = sqliteBackend.getInodeByPath(parentPath);
    if (!parentIno) {
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

  if (!path) {
    return res.status(400).json({ error: "Path richiesto" });
  }

  try {
    const fileToRemove = sqliteBackend.getFileMetadataByPath(path);
    if (!fileToRemove) {
      return res.status(404).json({ error: "File non trovato" });
    }

    const isActuallyDirectory = fileToRemove.file_type === "Directory";
    if (isDirectory && !isActuallyDirectory) {
      return res.status(400).json({ error: "Non è una directory" });
    }

    if (!isDirectory && isActuallyDirectory) {
      return res.status(400).json({ error: "È una directory, usa rmdir" });
    }

    const result = sqliteBackend.deleteNode(path, isDirectory);
    if (!result.success) {
      switch (result.error) {
        case 'no_such_file_or_directory':
          return res.status(404).json({ error: "File non trovato" });
        case 'not_a_directory':
          return res.status(400).json({ error: "Non è una directory" });
        case 'is_a_directory':
          return res.status(400).json({ error: "È una directory, usa rmdir" });
        case 'directory_not_empty':
          return res.status(409).json({ error: "Directory non vuota" });
        default:
          return res.status(500).json({ error: "Errore durante la rimozione" });
      }
    }

    res.status(200).json({ message: "Rimosso con successo" });
  } catch (err) {
    res.status(500).json({ error: "Errore durante la rimozione" });
  }
});

// Endpoint per aprire file
app.post("/open", (req, res) => {
  const { path, flags } = req.body;

  if (!path) {
    return res.status(400).json({ error: "Path richiesto" });
  }

  try {
    const file = sqliteBackend.getFileMetadataByPath(path);
    if (!file) {
      return res.status(404).json({ error: "File non trovato" });
    }

    // Controlla che non sia una directory (a meno che non sia opendir)
    if (file.file_type === "Directory") {
      return res.status(400).json({ error: "È una directory, usa opendir" });
    }

    const fileHandle = sqliteBackend.openFile({ path, flags });
    res.status(200).json(fileHandle);
  } catch (err) {
    res.status(500).json({ error: "Errore durante l'apertura del file" });
  }
});

// Endpoint per rinominare/spostare file e directory
app.post('/rename', (req, res) => {

  const { old_path, new_path } = req.body;

  if (!old_path || !new_path) {
    return res.status(400).json({ error: 'Both old_path and new_path are required' });
  }

  try {
    const sourceFile = sqliteBackend.getFileMetadataByPath(old_path);
    if (!sourceFile) {
      return res.status(404).json({ error: 'Source file not found' });
    }

    const destFile = sqliteBackend.getFileMetadataByPath(new_path);
    if (destFile) {
      return res.status(409).json({ error: 'Destination already exists' });
    }

    const newParentPath = new_path.substring(0, new_path.lastIndexOf('/')) || '/';
    const parentIno = sqliteBackend.getInodeByPath(newParentPath);
    if (!parentIno) {
      return res.status(404).json({ error: 'Parent directory not found' });
    }

    if (sourceFile.file_type === 'Directory' && new_path.startsWith(old_path + '/')) {
      return res.status(400).json({ error: 'Cannot move directory into itself' });
    }

    sqliteBackend.renameNode(old_path, new_path);
    const updatedMetadata = sqliteBackend.getFileMetadataByPath(new_path);

    res.json({
      message: 'File renamed successfully',
      old_path,
      new_path,
      metadata: updatedMetadata
    });
  } catch (err) {
    res.status(500).json({ error: 'Internal server error' });
  }
});

app.get("/health", (req, res) => {
  res.json({ status: "ok" });
});

// Endpoint per risolvere inode -> path
app.get("/resolve-inode/:ino", (req, res) => {
  const ino = parseInt(req.params.ino);
  if (isNaN(ino)) {
    return res.status(400).json({ error: "Inode non valido" });
  }
  try {
    const path = sqliteBackend.resolveInode(ino);
    if (!path) {
      return res.status(404).json({ error: "Inode non trovato" });
    }
    res.send(path);
  } catch (err) {
    res.status(500).json({ error: "Errore durante la risoluzione dell'inode" });
  }
});

// Endpoint per ottenere metadati di un file
app.get("/metadata", (req, res) => {
  const path = req.query.path as string;
  if (!path) {
    return res.status(400).json({ error: "Path richiesto" });
  }

  try {
    const metadata = sqliteBackend.getFileMetadataByPath(path);
    if (!metadata) {
      return res.status(404).json({ error: "File non trovato" });
    }
    res.json(metadata);
  } catch (err) {
    res.status(500).json({ error: "Errore durante il recupero dei metadati" });
  }
});

// Endpoint per aggiornare metadati di un file
app.patch("/metadata", (req, res) => {
  const path = req.query.path as string;
  if (!path) {
    return res.status(400).json({ error: "Path parameter is required" });
  }
  try {
    const updatedMetadata = sqliteBackend.updateMetadata(path, req.body);
    if (!updatedMetadata) {
      return res.status(404).json({ error: "File not found" });
    }
    res.json(updatedMetadata);
  } catch (err) {
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
