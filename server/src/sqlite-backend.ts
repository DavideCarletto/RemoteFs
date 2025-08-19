import Database from 'better-sqlite3';
import {INode} from './server'
import * as fs from 'fs';
import * as path from 'path';

export interface FileHandle {
  file_handle: number;  //id file aperto
  flags: number;   //flag apertura
  path: string;   //percorso del file
}

export class SQLiteBackend {
  private db: Database.Database;
  private openFiles: Map<number, FileHandle>;  //classe mantiene una mappa dei file aperti

  constructor(dbPath: string) {
    const dbDir = path.dirname(dbPath);
    if (!fs.existsSync(dbDir)) {
      fs.mkdirSync(dbDir, { recursive: true });
    }
    
    this.db = new Database(dbPath);
    this.openFiles = new Map();
    this.initializeDatabase();
    this.initializeDefaultFiles();
  }

  private initializeDatabase(){
    this.db.exec(`CREATE TABLE IF NOT EXISTS fs_nodes (
                ino INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                parent_ino INTEGER REFERENCES fs_nodes(ino),
                name TEXT NOT NULL,
                file_type TEXT NOT NULL CHECK (file_type IN ('RegularFile', 'Directory')),
                size INTEGER DEFAULT 0,
                permissions INTEGER DEFAULT 644,
                uid INTEGER DEFAULT 1000,
                gid INTEGER DEFAULT 1000,
                atime INTEGER DEFAULT (strftime('%s','now')),
                mtime INTEGER DEFAULT (strftime('%s','now')),
                ctime INTEGER DEFAULT (strftime('%s','now')),
                crtime INTEGER DEFAULT (strftime('%s','now')),
                blocks INTEGER DEFAULT 0,
                blksize INTEGER DEFAULT 512,
                nlink INTEGER DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_fs_nodes_path ON fs_nodes(path);
            CREATE INDEX IF NOT EXISTS idx_fs_nodes_parent ON fs_nodes(parent_ino);
            CREATE INDEX IF NOT EXISTS idx_fs_nodes_name ON fs_nodes(name);

            CREATE TABLE IF NOT EXISTS fs_content (
                ino INTEGER PRIMARY KEY REFERENCES fs_nodes(ino) ON DELETE CASCADE,
                content BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS fs_chunks (
                ino INTEGER REFERENCES fs_nodes(ino) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL,
                chunk_size INTEGER NOT NULL,
                chunk_data BLOB NOT NULL,
                created_at INTEGER DEFAULT (strftime('%s','now')),
                PRIMARY KEY (ino, chunk_index)
            );
        `);
  }

  //crea struttura base del filesystem (directory root (/), file test.txt di esempio e directory documents)
  initializeDefaultFiles() {
  // Crea la directory root se non esiste
  const rootStmt = this.db.prepare(`
    INSERT OR IGNORE INTO fs_nodes (
      ino, path, parent_ino, name, file_type, size, permissions, uid, gid,
      atime, mtime, ctime, crtime, blocks, blksize, nlink
    ) VALUES (
      1, '/', NULL, '', 'Directory', 4096, 755, 1000, 1000,
      strftime('%s','now'), strftime('%s','now'), strftime('%s','now'), strftime('%s','now'),
      8, 512, 2
    )
  `);
  rootStmt.run();

  // Crea test.txt di default
  const testFileStmt = this.db.prepare(`
    INSERT OR IGNORE INTO fs_nodes (
      ino, path, parent_ino, name, file_type, size, permissions, uid, gid,
      atime, mtime, ctime, crtime, blocks, blksize, nlink
    ) VALUES (
      2, '/test.txt', 1, 'test.txt', 'RegularFile', ?, 644, 1000, 1000,
      strftime('%s','now'), strftime('%s','now'), strftime('%s','now'), strftime('%s','now'),
      ?, 512, 1
    )
  `);
  const testContent = Buffer.from("Questo è il contenuto del file test.txt\nSeconda riga di esempio\n");
  testFileStmt.run(testContent.length, Math.ceil(testContent.length / 512));

  // Crea la directory documents
  const documentsStmt = this.db.prepare(`
    INSERT OR IGNORE INTO fs_nodes (
      ino, path, parent_ino, name, file_type, size, permissions, uid, gid,
      atime, mtime, ctime, crtime, blocks, blksize, nlink
    ) VALUES (
      3, '/documents', 1, 'documents', 'Directory', 4096, 755, 1000, 1000,
      strftime('%s','now'), strftime('%s','now'), strftime('%s','now'), strftime('%s','now'),
      8, 512, 2
    )
  `);
  documentsStmt.run();
  }

  //creazione nuovo file nel filesystem
  createFile(params: {
    path: string;
    parent_ino: number;
    name: string;
    mode?: number;
    uid?: number;
    gid?: number;
  }): number {
    const now = Math.floor(Date.now() / 1000);
    const stmt = this.db.prepare(`
        INSERT INTO fs_nodes (
          path, parent_ino, name, file_type, size, permissions, uid, gid,
          atime, mtime, ctime, crtime, blocks, blksize, nlink
        ) VALUES (
         @path, @parent_ino, @name, 'RegularFile', 0, @permissions, @uid, @gid,
         @now, @now, @now, @now, 0, 512, 1
         )
        `);
    const info = stmt.run({
        path: params.path,
        parent_ino: params.parent_ino,
        name: params.name,
        permissions: params.mode ?? 0o644,
        uid: params.uid ?? 1000,
        gid: params.gid ?? 1000,
        now
    });
    return info.lastInsertRowid as number;
  }

  //ottiene metadati di un file dall'inode
  getFileMetadataByIno(ino: number): INode | undefined {
    const stmt = this.db.prepare('SELECT * FROM fs_nodes WHERE ino = ?');
    return stmt.get(ino) as INode | undefined;
  }

  //converte un path in inode
  getInodeByPath(path: string): number | undefined {
    const stmt = this.db.prepare('SELECT ino FROM fs_nodes WHERE path = ?');
    const result = stmt.get(path) as { ino: number } | undefined;
    return result?.ino;
  }

  //elenca contenuto directory
  listDirectory(path: string): Array<{ ino: number; name: string; file_type: string }> {
    let entries: Array<{ ino: number; name: string; file_type: string }> = [];
    
    // Ottieni l'inode della directory
    const dirIno = path === '/' ? 1 : this.getInodeByPath(path);
    
    if (dirIno !== undefined) {
      console.log(`[LISTDIR] Listing directory with inode ${dirIno}`);
      
      const stmt = this.db.prepare(`
        SELECT ino, name, file_type 
        FROM fs_nodes 
        WHERE parent_ino = ?
        ORDER BY name
      `);
      
      try {
        entries = stmt.all(dirIno) as Array<{ ino: number; name: string; file_type: string }>;
        console.log(`[LISTDIR] Found ${entries.length} entries:`, entries.map(e => e.name).join(', '));
      } catch (error) {
        console.error(`[LISTDIR] Error listing directory ${path}:`, error);
      }
    } else {
      console.error(`[LISTDIR] Directory not found: ${path}`);
    }
    
    return entries;
  }

  //creazione nuova directory
  createDirectory(params: {
    path: string;
    parent_ino: number;
    name: string;
    mode?: number;
    uid?: number;
    gid?: number;
  }): number {
    const now = Math.floor(Date.now() / 1000);
    const stmt = this.db.prepare(`
      INSERT INTO fs_nodes (
        path, parent_ino, name, file_type, size, permissions, uid, gid,
        atime, mtime, ctime, crtime, blocks, blksize, nlink
      ) VALUES (
        @path, @parent_ino, @name, 'Directory', 4096, @permissions, @uid, @gid,
        @now, @now, @now, @now, 8, 512, 2
      )
    `);
    
    const info = stmt.run({
      path: params.path,
      parent_ino: params.parent_ino,
      name: params.name,
      permissions: params.mode ?? 0o755,
      uid: params.uid ?? 1000,
      gid: params.gid ?? 1000,
      now
    });
    
    return info.lastInsertRowid as number;
  }

  //elimina un file o directory
  deleteNode(path: string, isDirectory?: boolean): { success: boolean; error?: string } {
    console.log(`[DELETE] Tentativo di rimozione: ${path} (isDirectory: ${isDirectory})`);
    
    // Verifica se il nodo esiste
    const checkStmt = this.db.prepare('SELECT ino, file_type FROM fs_nodes WHERE path = ?');
    const node = checkStmt.get(path) as { ino: number; file_type: string } | undefined;
    
    if (!node) {
      console.log(`[DELETE] Nodo non trovato: ${path}`);
      return { success: false, error: 'no_such_file_or_directory' };
    }

    // Controlla che il tipo di file sia corretto se specificato
    if (isDirectory !== undefined) {
      if (isDirectory && node.file_type !== 'Directory') {
        return { success: false, error: 'not_a_directory' };
      }
      if (!isDirectory && node.file_type === 'Directory') {
        return { success: false, error: 'is_a_directory' };
      }
    }

    // Se è una directory, verifica che sia vuota
    if (node.file_type === 'Directory') {
      const checkEmptyStmt = this.db.prepare('SELECT COUNT(*) as count FROM fs_nodes WHERE parent_ino = ?');
      const result = checkEmptyStmt.get(node.ino) as { count: number };
      if (result.count > 0) {
        console.log(`[DELETE] Directory non vuota: ${path}`);
        return { success: false, error: 'directory_not_empty' };
      }
    }

    try {
      this.db.transaction(() => {
        // Rimuovi prima il contenuto del file se esiste
        const contentStmt = this.db.prepare('DELETE FROM fs_content WHERE ino = ?');
        contentStmt.run(node.ino);

        // Rimuovi eventuali chunks
        const chunksStmt = this.db.prepare('DELETE FROM fs_chunks WHERE ino = ?');
        chunksStmt.run(node.ino);

        // Rimuovi il nodo
        const nodeStmt = this.db.prepare('DELETE FROM fs_nodes WHERE ino = ?');
        nodeStmt.run(node.ino);
      })();

      console.log(`[DELETE] Nodo rimosso con successo: ${path}`);
      return { success: true };
    } catch (error) {
      console.error(`[DELETE] Errore durante la rimozione di ${path}:`, error);
      return { success: false, error: 'internal_error' };
    }
  }

  openFile(params: { path: string; flags: number }): FileHandle {
    const fileHandle = Date.now() + Math.floor(Math.random() * 1000);
    const handle: FileHandle = {
      file_handle: fileHandle,
      flags: params.flags,
      path: params.path
    };
    this.openFiles.set(fileHandle, handle);
    return handle;
  }

  readFile(path: string, offset: number, size: number): Buffer {
    const stmt = this.db.prepare('SELECT content FROM fs_content WHERE ino = (SELECT ino FROM fs_nodes WHERE path = ?)');
    const result = stmt.get(path) as { content: Buffer } | undefined;
    
    if (!result) {
      return Buffer.alloc(0);
    }

    return result.content.subarray(offset, offset + size);
  }

  writeFile(path: string, data: Buffer, offset: number): number {
    const nodeStmt = this.db.prepare('SELECT ino, size FROM fs_nodes WHERE path = ?');
    const node = nodeStmt.get(path) as { ino: number; size: number };

    if (!node) {
      throw new Error('File not found');
    }

    const contentStmt = this.db.prepare(`
      INSERT OR REPLACE INTO fs_content (ino, content)
      VALUES (?, ?)
    `);

    // If this is the first write, or we're writing at offset 0
    if (offset === 0) {
      contentStmt.run(node.ino, data);
    } else {
      // For appending or writing at an offset, we need to read existing content
      const existingContent = this.readFile(path, 0, node.size);
      const newSize = Math.max(existingContent.length, offset + data.length);
      const newContent = Buffer.alloc(newSize);
      
      existingContent.copy(newContent, 0);
      data.copy(newContent, offset);
      contentStmt.run(node.ino, newContent);
    }

    // Update file size and timestamps
    const updateStmt = this.db.prepare(`
      UPDATE fs_nodes 
      SET size = ?, 
          mtime = ?,
          blocks = ?
      WHERE ino = ?
    `);
    
    const newSize = Math.max(node.size, offset + data.length);
    const now = Math.floor(Date.now() / 1000);
    updateStmt.run(newSize, now, Math.ceil(newSize / 512), node.ino);

    return data.length;
  }

  //rinomina/sposta file e directory
  renameNode(oldPath: string, newPath: string): void {
    const getNodeStmt = this.db.prepare('SELECT ino FROM fs_nodes WHERE path = ?');
    const node = getNodeStmt.get(oldPath) as { ino: number } | undefined;

    if (!node) {
      throw new Error('Source path not found');
    }

    this.db.transaction(() => {
      // Update the main node
      const updateStmt = this.db.prepare(`
        UPDATE fs_nodes 
        SET path = ?,
            name = ?,
            mtime = ?,
            ctime = ?
        WHERE ino = ?
      `);
      
      const now = Math.floor(Date.now() / 1000);
      const newName = newPath.split('/').pop() || '';
      updateStmt.run(newPath, newName, now, now, node.ino);

      // Update all child paths if it's a directory
      const updateChildrenStmt = this.db.prepare(`
        UPDATE fs_nodes 
        SET path = replace(path, ?, ?)
        WHERE path LIKE ?
      `);
      
      updateChildrenStmt.run(oldPath + '/', newPath + '/', oldPath + '/%');
    })();
  }

  //gestione metadati (aggiorna metadati di un file)
  updateMetadata(path: string, updates: {
    mode?: number;
    uid?: number;
    gid?: number;
    size?: number;
    atime?: number;
    mtime?: number;
    ctime?: number;
    crtime?: number;
  }): INode | undefined {
    const setFields: string[] = [];
    const params: any = { path };

    if (updates.mode !== undefined) {
      setFields.push('permissions = @mode');
      params.mode = updates.mode;
    }
    if (updates.uid !== undefined) {
      setFields.push('uid = @uid');
      params.uid = updates.uid;
    }
    if (updates.gid !== undefined) {
      setFields.push('gid = @gid');
      params.gid = updates.gid;
    }
    if (updates.size !== undefined) {
      setFields.push('size = @size');
      params.size = updates.size;
      params.blocks = Math.ceil(updates.size / 512);
      setFields.push('blocks = @blocks');
    }

    // Gestione esplicita dei timestamp
    if (updates.atime !== undefined) {
      setFields.push('atime = @atime');
      params.atime = updates.atime;
    }
    if (updates.mtime !== undefined) {
      setFields.push('mtime = @mtime');
      params.mtime = updates.mtime;
    }
    if (updates.ctime !== undefined) {
      setFields.push('ctime = @ctime');
      params.ctime = updates.ctime;
    }
    if (updates.crtime !== undefined) {
      setFields.push('crtime = @crtime');
      params.crtime = updates.crtime;
    }

    if (setFields.length > 0) {
      // Se non sono stati forniti timestamp specifici, aggiorna mtime
      if (!updates.mtime) {
        setFields.push('mtime = @now');
        params.now = Math.floor(Date.now() / 1000);
      }

      const stmt = this.db.prepare(`
        UPDATE fs_nodes 
        SET ${setFields.join(', ')}
        WHERE path = @path
        RETURNING *
      `);
      
      return stmt.get(params) as INode;
    }

    return this.getFileMetadataByPath(path);
  }

  //ottiene metadati di un file dal path
  getFileMetadataByPath(path: string): INode | undefined {
    const stmt = this.db.prepare('SELECT * FROM fs_nodes WHERE path = ?');
    return stmt.get(path) as INode | undefined;
  }

  //converte un inode in path
  resolveInode(ino: number): string | undefined {
    const stmt = this.db.prepare('SELECT path FROM fs_nodes WHERE ino = ?');
    const result = stmt.get(ino) as { path: string } | undefined;
    return result?.path;
  }

  closeFile(fileHandle: number): void {
    this.openFiles.delete(fileHandle);
  }
}
