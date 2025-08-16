import express from "express";

process.on('uncaughtException', err => console.error('Uncaught Exception:', err));
process.on('unhandledRejection', err => console.error('Unhandled Rejection:', err));

const app = express();
const PORT = 3000;

app.use(express.json());

// Mock data structure per simulare il filesystem
interface MockFile {
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

// Mock filesystem con alcuni file di test
const mockFilesystem: { [path: string]: MockFile } = {
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
    size: 12,
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
Object.values(mockFilesystem).forEach(file => {
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
  
  const file = mockFilesystem[path];
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
    filesystem: mockFilesystem,
    inodeMap: inodeToPath
  });
});


app.listen(PORT, () => {
  console.log(`🚀 Server avviato su http://localhost:${PORT}`);
  console.log(`📁 Mock filesystem caricato con ${Object.keys(mockFilesystem).length} file`);
  console.log(`🔧 Endpoint disponibili:`);
  console.log(`   - GET /health - Health check`);
  console.log(`   - GET /resolve-inode/:ino - Risolve inode in path`);
  console.log(`   - GET /metadata?path=... - Ottiene metadati file`);
  console.log(`   - GET /debug/files - Lista tutti i file mock`);
});

export {app, mockFilesystem, inodeToPath };
  