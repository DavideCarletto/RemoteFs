import Database from 'better-sqlite3';
import {INode} from './server'

export class SQLiteBackend {
  private db: Database.Database;

  constructor(dbPath: string) {
    this.db = new Database(dbPath);
    this.initializeDatabase();
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

  getFileMetadataByIno(ino: number): INode | undefined {
    const stmt = this.db.prepare('SELECT * FROM fs_nodes WHERE ino = ?');
    return stmt.get(ino) as INode | undefined;
  }
}