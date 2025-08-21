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
  private openFiles: Map<number, FileHandle>;
  private filesystemDir: string;

  constructor(dbPath: string) {
    const dbDir = path.dirname(dbPath);
    if (!fs.existsSync(dbDir)) {
      fs.mkdirSync(dbDir, { recursive: true });
    }
    
    this.filesystemDir = path.resolve(dbDir, '../fs');
    if (!fs.existsSync(this.filesystemDir)) {
      fs.mkdirSync(this.filesystemDir, { recursive: true });
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
        `);
  }

  //crea struttura base del filesystem (directory root (/), file test.txt di esempio e directory documents)
  initializeDefaultFiles() {
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
  const baseText = "Non si intrometta! No, aspetti, mi porga l'indice; ";
  // Crea un file di test da ~10MB
  const targetSizeMB = 10;
  const targetSizeBytes = targetSizeMB * 1024 * 1024; // 10MB in bytes
  const repeatCount = Math.ceil(targetSizeBytes / baseText.length);
  const testContent = Buffer.from(baseText.repeat(repeatCount));
  
  testFileStmt.run(testContent.length, Math.ceil(testContent.length / 512));

  // Crea il file fisico per test.txt
  const testFilePath = `${this.filesystemDir}/2`;
  if (!fs.existsSync(testFilePath)) {
    fs.writeFileSync(testFilePath, testContent);
  }

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
    
    const dirIno = path === '/' ? 1 : this.getInodeByPath(path);
    
    if (dirIno !== undefined) {
      
      const stmt = this.db.prepare(`
        SELECT ino, name, file_type 
        FROM fs_nodes 
        WHERE parent_ino = ?
        ORDER BY name
      `);
      
      try {
        entries = stmt.all(dirIno) as Array<{ ino: number; name: string; file_type: string }>;
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
    
    const checkStmt = this.db.prepare('SELECT ino, file_type FROM fs_nodes WHERE path = ?');
    const node = checkStmt.get(path) as { ino: number; file_type: string } | undefined;
    
    if (!node) {
      return { success: false, error: 'no_such_file_or_directory' };
    }

    if (isDirectory !== undefined) {
      if (isDirectory && node.file_type !== 'Directory') {
        return { success: false, error: 'not_a_directory' };
      }
      if (!isDirectory && node.file_type === 'Directory') {
        return { success: false, error: 'is_a_directory' };
      }
    }

    if (node.file_type === 'Directory') {
      const checkEmptyStmt = this.db.prepare('SELECT COUNT(*) as count FROM fs_nodes WHERE parent_ino = ?');
      const result = checkEmptyStmt.get(node.ino) as { count: number };
      if (result.count > 0) {
        return { success: false, error: 'directory_not_empty' };
      }
    }

    try {
      this.db.transaction(() => {
        if (node.file_type === 'RegularFile') {
          const filePath = `${this.filesystemDir}/${node.ino}`;
          if (fs.existsSync(filePath)) {
            fs.unlinkSync(filePath);
          }
        }

        const nodeStmt = this.db.prepare('DELETE FROM fs_nodes WHERE ino = ?');
        nodeStmt.run(node.ino);
      })();

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

  // Streaming di lettura
  readFile(path: string, options?: { start?: number; end?: number }): fs.ReadStream | null {
    const stmt = this.db.prepare('SELECT ino FROM fs_nodes WHERE path = ?');
    const result = stmt.get(path) as { ino: number } | undefined;
    
    if (!result) {
      return null;
    }

    const filePath = `${this.filesystemDir}/${result.ino}`;
    
    try {
      if (!fs.existsSync(filePath)) {
        return null;
      }
      
      return fs.createReadStream(filePath, options);
    } catch (error) {
      console.error(`[FS] Error creating read stream for ${filePath}:`, error);
      return null;
    }
  }

  // Streaming di scrittura
  writeFile(path: string, options?: { start?: number }): fs.WriteStream | null {
    const stmt = this.db.prepare('SELECT ino FROM fs_nodes WHERE path = ?');
    const result = stmt.get(path) as { ino: number } | undefined;
    
    if (!result) {
      return null;
    }

    const filePath = `${this.filesystemDir}/${result.ino}`;
    
    try {
      return fs.createWriteStream(filePath, options);
    } catch (error) {
      console.error(`[FS] Error creating write stream for ${filePath}:`, error);
      return null;
    }
  }

  // Aggiorna dimensione file dopo streaming
  updateFileSize(path: string): void {
    const stmt = this.db.prepare('SELECT ino FROM fs_nodes WHERE path = ?');
    const result = stmt.get(path) as { ino: number } | undefined;
    
    if (!result) {
      return;
    }

    const filePath = `${this.filesystemDir}/${result.ino}`;
    
    try {
      if (fs.existsSync(filePath)) {
        const stats = fs.statSync(filePath);
        const updateStmt = this.db.prepare(`
          UPDATE fs_nodes 
          SET size = ?, 
              mtime = ?,
              blocks = ?
          WHERE ino = ?
        `);
        
        const now = Math.floor(Date.now() / 1000);
        updateStmt.run(stats.size, now, Math.ceil(stats.size / 512), result.ino);
      }
    } catch (error) {
      console.error(`[FS] Error updating file size for ${filePath}:`, error);
    }
  }

  //rinomina/sposta file e directory
  renameNode(oldPath: string, newPath: string): void {
    const getNodeStmt = this.db.prepare('SELECT ino FROM fs_nodes WHERE path = ?');
    const node = getNodeStmt.get(oldPath) as { ino: number } | undefined;

    if (!node) {
      throw new Error('Source path not found');
    }

    this.db.transaction(() => {
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

    if (updates.mode != null) {
      setFields.push('permissions = @mode');
      params.mode = updates.mode;
    }
    if (updates.uid != null) {
      setFields.push('uid = @uid');
      params.uid = updates.uid;
    }
    if (updates.gid != null) {
      setFields.push('gid = @gid');
      params.gid = updates.gid;
    }
    if (updates.size != null) {
      setFields.push('size = @size');
      params.size = updates.size;
      params.blocks = Math.ceil(updates.size / 512);
      setFields.push('blocks = @blocks');
    }

    if (updates.atime != null) {
      setFields.push('atime = @atime');
      params.atime = updates.atime;
    }
    if (updates.mtime != null) {
      setFields.push('mtime = @mtime');
      params.mtime = updates.mtime;
    }
    if (updates.ctime != null) {
      setFields.push('ctime = @ctime');
      params.ctime = updates.ctime;
    }
    if (updates.crtime != null) {
      setFields.push('crtime = @crtime');
      params.crtime = updates.crtime;
    }

    if (setFields.length > 0) {
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
