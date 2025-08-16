import request from 'supertest';
import {app, mockFilesystem, inodeToPath } from '../src/server';

// Test suite
describe('Remote FS API Tests', () => {
  
  describe('Health Check', () => {
    test('GET /health should return status ok', async () => {
      const response = await request(app)
        .get('/health')
        .expect(200);
      
      expect(response.body).toEqual({ status: "ok" });
    });
  });

  describe('Inode Resolution', () => {
    test('GET /resolve-inode/1 should return root path', async () => {
      const response = await request(app)
        .get('/resolve-inode/1')
        .expect(200);
      
      expect(response.text).toBe('/');
    });

    test('GET /resolve-inode/2 should return /test.txt path', async () => {
      const response = await request(app)
        .get('/resolve-inode/2')
        .expect(200);
      
      expect(response.text).toBe('/test.txt');
    });

    test('GET /resolve-inode/999 should return 404 for non-existent inode', async () => {
      const response = await request(app)
        .get('/resolve-inode/999')
        .expect(404);
      
      expect(response.body).toEqual({ error: "Inode non trovato" });
    });

    test('GET /resolve-inode/invalid should return 400 for invalid inode', async () => {
      const response = await request(app)
        .get('/resolve-inode/invalid')
        .expect(400);
      
      expect(response.body).toEqual({ error: "Inode non valido" });
    });
  });

  describe('File Metadata', () => {
    test('GET /metadata?path=/ should return root directory metadata', async () => {
      const response = await request(app)
        .get('/metadata?path=/')
        .expect(200);
      
      expect(response.body).toEqual(expect.objectContaining({
        ino: 1,
        path: "/",
        file_type: "Directory",
        permissions: 0o755,
        size: 4096
      }));
    });

    test('GET /metadata?path=/test.txt should return file metadata', async () => {
      const response = await request(app)
        .get('/metadata?path=/test.txt')
        .expect(200);
      
      expect(response.body).toEqual(expect.objectContaining({
        ino: 2,
        path: "/test.txt",
        file_type: "RegularFile",
        permissions: 0o644,
        size: 12
      }));
    });

    test('GET /metadata?path=/nonexistent should return 404', async () => {
      const response = await request(app)
        .get('/metadata?path=/nonexistent')
        .expect(404);
      
      expect(response.body).toEqual({ error: "File non trovato" });
    });

    test('GET /metadata without path should return 400', async () => {
      const response = await request(app)
        .get('/metadata')
        .expect(400);
      
      expect(response.body).toEqual({ error: "Path richiesto" });
    });
  });

  describe('Filesystem Lookup Simulation', () => {
    test('Should simulate complete lookup flow: parent=1, name="test.txt"', async () => {
      // Step 1: Resolve parent inode to path
      const parentResponse = await request(app)
        .get('/resolve-inode/1')
        .expect(200);
      
      expect(parentResponse.text).toBe('/');
      
      // Step 2: Build full path
      const parentPath = parentResponse.text;
      const fileName = 'test.txt';
      const fullPath = parentPath === '/' ? `/${fileName}` : `${parentPath}/${fileName}`;
      
      expect(fullPath).toBe('/test.txt');
      
      // Step 3: Get file metadata
      const metadataResponse = await request(app)
        .get(`/metadata?path=${encodeURIComponent(fullPath)}`)
        .expect(200);
      
      expect(metadataResponse.body).toEqual(expect.objectContaining({
        ino: 2,
        path: "/test.txt",
        file_type: "RegularFile",
        size: 12
      }));
    });

    test('Should handle lookup for non-existent file', async () => {
      // Step 1: Resolve parent inode
      const parentResponse = await request(app)
        .get('/resolve-inode/1')
        .expect(200);
      
      // Step 2: Build path for non-existent file
      const fullPath = '/nonexistent.txt';
      
      // Step 3: Should get 404 for metadata
      await request(app)
        .get(`/metadata?path=${encodeURIComponent(fullPath)}`)
        .expect(404);
    });
  });

  describe('Debug Endpoints', () => {
    test('GET /debug/files should return filesystem structure', async () => {
      const response = await request(app)
        .get('/debug/files')
        .expect(200);
      
      expect(response.body).toHaveProperty('filesystem');
      expect(response.body).toHaveProperty('inodeMap');
      expect(response.body.filesystem).toEqual(mockFilesystem);
      expect(response.body.inodeMap).toEqual(inodeToPath);
    });
  });

});

