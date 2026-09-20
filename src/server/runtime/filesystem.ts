import { createHash, randomUUID } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, readFile, writeFile, rename, link, unlink, rm, stat, open, readdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { Readable } from 'node:stream';
import type { FileMetadata, FileStore, StoredFile } from './types';
import type {PostgresDatabase} from './postgres';

type Manifest = Omit<FileMetadata, 'uploaded'> & { uploaded: string; version: string };
const missing = (error: unknown) => (error as NodeJS.ErrnoException).code === 'ENOENT';
export class LocalFileStore implements FileStore {
  readonly root: string;
  constructor(root: string, private readonly preservationDb?: PostgresDatabase) { this.root = resolve(root); }
  private location(key: string) {
    if (!key || key.includes('\0') || key.split(/[\\/]/).some(p => p === '..' || p === '.')) throw new Error('INVALID_OBJECT_KEY');
    // Object keys never become filesystem paths. Even absolute keys cannot escape.
    return join(this.root, createHash('sha256').update(key).digest('hex'));
  }
  private async manifest(key: string): Promise<Manifest | null> {
    try { const value = JSON.parse(await readFile(join(this.location(key), 'current.json'), 'utf8')) as Manifest;
      if (value.key !== key || !/^[a-f0-9-]{36}$/.test(value.version)) throw new Error('INVALID_OBJECT_MANIFEST');
      return value;
    } catch (error) { if (missing(error)) return null; throw error; }
  }
  private metadata(value: Manifest): FileMetadata { const { version: _version, ...metadata } = value; void _version; return { ...metadata, uploaded: new Date(value.uploaded) }; }
  async head(key: string) { const value = await this.manifest(key); return value ? this.metadata(value) : null; }
  async put(key: string, value: Parameters<FileStore['put']>[1], options: Parameters<FileStore['put']>[2] = {}) {
    if(this.preservationDb&&!options.onlyIf&&!key.startsWith('legal/'))return this.protect([key],()=>this.putUnchecked(key,value,options));
    return this.putUnchecked(key,value,options);
  }
  private async putUnchecked(key: string, value: Parameters<FileStore['put']>[1], options: Parameters<FileStore['put']>[2] = {}) {
    if (key.startsWith('legal/') && options.onlyIf?.etagDoesNotMatch !== '*') throw new Error('LEGAL_DOCUMENT_IMMUTABLE');
    if (options.onlyIf && options.onlyIf.etagDoesNotMatch !== '*') throw new Error('UNSUPPORTED_OBJECT_CONDITION');
    const directory = this.location(key), version = randomUUID();
    await mkdir(directory, { recursive: true, mode: 0o700 });
    const body = join(directory, version), manifest = join(directory, `${version}.json`);
    const hash = createHash('sha256'); let size = 0;
    const file = await open(body, 'wx', 0o600);
    try {
      const chunks = typeof value === 'string' ? [Buffer.from(value)]
        : value instanceof Blob ? Readable.fromWeb(value.stream() as never)
        : value instanceof ReadableStream ? Readable.fromWeb(value as never)
        : [ArrayBuffer.isView(value) ? Buffer.from(value.buffer, value.byteOffset, value.byteLength) : Buffer.from(value)];
      for await (const chunk of chunks) {
        const bytes = Buffer.from(chunk); size += bytes.length; hash.update(bytes);
        // FileHandle.write can write fewer bytes than requested.
        let offset = 0;
        while (offset < bytes.length) offset += (await file.write(bytes, offset, bytes.length - offset)).bytesWritten;
      }
      await file.sync();
    } catch (error) { await unlink(body).catch(() => {}); throw error; }
    finally { await file.close(); }
    const etag = hash.digest('hex');
    const data: Manifest = { key, version, size, etag, httpEtag: `"${etag}"`, uploaded: new Date().toISOString(), httpMetadata: options.httpMetadata ?? {}, customMetadata: options.customMetadata ?? {} };
    try {
      await writeFile(manifest, JSON.stringify(data), { flag: 'wx', mode: 0o600 });
      const handle = await open(manifest, 'r'); try { await handle.sync(); } finally { await handle.close(); }
      if (options.onlyIf) await link(manifest, join(directory, 'current.json'));
      else await rename(manifest, join(directory, 'current.json'));
      const handleDirectory = await open(directory, 'r'); try { await handleDirectory.sync(); } finally { await handleDirectory.close(); }
      return this.metadata(data);
    } catch (error) {
      if (options.onlyIf && (error as NodeJS.ErrnoException).code === 'EEXIST') { await unlink(body).catch(() => {}); return null; }
      throw error;
    } finally { await unlink(manifest).catch(() => {}); }
  }
  async get(key: string, options: Parameters<FileStore['get']>[1] = {}): Promise<StoredFile | null> {
    const data = await this.manifest(key); if (!data) return null;
    const path = join(this.location(key), data.version);
    await stat(path); // An existing manifest with missing bytes is a storage fault.
    let offset = 0, length = data.size;
    if (options.range) {
      const range = options.range;
      offset = range.suffix === undefined ? range.offset ?? 0 : Math.max(0, data.size - range.suffix);
      length = Math.min(range.length ?? data.size - offset, data.size - offset);
      if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 1 || offset >= data.size) throw new RangeError('INVALID_RANGE');
    }
    const stream = () => length === 0 ? new ReadableStream<Uint8Array>({ start(c) { c.close(); } })
      : Readable.toWeb(createReadStream(path, { start: offset, end: offset + length - 1 })) as unknown as ReadableStream<Uint8Array>;
    return { ...this.metadata(data), ...(options.range ? { range: { offset, length } } : {}), body: stream(),
      arrayBuffer: () => new Response(stream()).arrayBuffer(), text: () => new Response(stream()).text() };
  }
  async delete(key: string | string[]) {
    if ((Array.isArray(key) ? key : [key]).some(item => item.startsWith('legal/'))) throw new Error('LEGAL_DOCUMENT_PRESERVATION_REQUIRED');
    const keys=Array.isArray(key)?key:[key];
    await this.protect(keys,async()=>{for (const item of keys) await rm(this.location(item), { recursive: true, force: true });});
  }
  private async protect<T>(keys:string[],operation:()=>Promise<T>):Promise<T>{
    if(!this.preservationDb)return operation();
    return this.preservationDb.transaction(async tx=>{
      await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run();
      for(const key of keys)if(await tx.prepare("SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets t ON t.item_id=p.item_id JOIN legal_holds h ON h.id=t.hold_id WHERE h.state!='released' AND (p.object_key=? OR (p.source_type='attachment' AND EXISTS(SELECT 1 FROM attachment_derivatives d WHERE d.attachment_id=p.source_id AND d.object_key=?)))").bind(key,key).first())throw new Error('LEGAL_HOLD_BLOCKS_FILE_MUTATION');
      return operation();
    });
  }
  async destroyPreserved(keys:string[],authorization:Parameters<NonNullable<FileStore['destroyPreserved']>>[1]){
    const tx=authorization.database;
    const row=await tx.prepare("SELECT x.manifest FROM legal_destructions x WHERE x.id=? AND x.state='executing' AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=x.item_id AND h.state!='released')").bind(authorization.destructionId).first<{manifest:string}>();
    if(!row)throw new Error('LEGAL_DESTRUCTION_NOT_AUTHORIZED');
    const manifest=typeof row.manifest==='string'?JSON.parse(row.manifest):row.manifest;
    for(const key of keys){if(!key.startsWith('legal/')||!manifest.keys.includes(key))throw new Error('LEGAL_DESTRUCTION_SCOPE');await rm(this.location(key),{recursive:true,force:true});}
  }
  async list(options: { prefix?: string; cursor?: string; limit?: number } = {}) {
    const objects: FileMetadata[] = [];
    let directories: string[];
    try { directories=await readdir(this.root); } catch(error) { if(missing(error))return {objects,truncated:false}; throw error; }
    for(const directory of directories) {
      if(!/^[a-f0-9]{64}$/.test(directory))continue;
      try {
        const manifest=JSON.parse(await readFile(join(this.root,directory,'current.json'),'utf8')) as Manifest;
        if(manifest.key.startsWith(options.prefix??'') && (!options.cursor || manifest.key>options.cursor))objects.push(this.metadata(manifest));
      } catch(error) {if(!missing(error))throw error;}
    }
    objects.sort((a,b)=>a.key<b.key?-1:a.key>b.key?1:0);
    const limit=Math.max(1,Math.min(1000,options.limit??1000)),truncated=objects.length>limit;
    const page=objects.slice(0,limit);
    return {objects:page,truncated,...(truncated?{cursor:page.at(-1)!.key}:{})};
  }
}
