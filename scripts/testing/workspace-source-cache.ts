import { createHash, randomUUID } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import ts from 'typescript';
import { parseWorkspaceSource, type WorkspaceSource } from './workspace-imports';

const parser = createHash('sha256').update(ts.version).update(parseWorkspaceSource.toString()).digest('hex');
const maximumEntries = 50_000, maximumBytes = 64 * 1024 * 1024;
function valid(value: unknown): value is WorkspaceSource {
  if (!value || typeof value !== 'object') return false;
  const row = value as WorkspaceSource;
  return Array.isArray(row.modules) && row.modules.every(item => item && typeof item.specifier === 'string' && typeof item.runtime === 'boolean') &&
    [row.strings, row.paths, row.urls].every(list => Array.isArray(list) && list.every(item => typeof item === 'string'));
}
/** Content-addressed syntax only: exports, files, ownership and selections are never cached. */
export function workspaceSourceCache(root: string, cacheRoot = root) {
  const cacheRelative=relative(resolve(root),resolve(cacheRoot));
  if(isAbsolute(cacheRelative)||cacheRelative==='..'||cacheRelative.startsWith('..'+sep)||cacheRelative&&cacheRelative.split(sep)[0]!=='.scratch')throw new Error('WORKSPACE_CACHE_ROOT_OUTSIDE_REPOSITORY');
  const entries = new Map<string, WorkspaceSource>();
  const stats = { hits: 0, misses: 0, persisted: false, loaded: false };
  let file: string | undefined;
  try {
    let directory = resolve(root);
    for (const part of [...cacheRelative.split(sep).filter(Boolean), '.scratch', 'dev', 'workspace-graph']) {
      directory = resolve(directory, part);
      if (!existsSync(directory)) mkdirSync(directory, { mode: 0o700 });
      const stat = lstatSync(directory); if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error('UNOWNED_GRAPH_CACHE_DIRECTORY');
    }
    file = resolve(directory, 'source-syntax.json');
    if (existsSync(file)) {
      const stat = lstatSync(file);
      if (!stat.isFile() || stat.isSymbolicLink() || stat.size > maximumBytes) throw new Error('INVALID_GRAPH_CACHE_FILE');
      let cache; try { cache=JSON.parse(readFileSync(file,'utf8')); } catch { cache=null; }
      if (cache?.schema === 'erpx-workspace-source-syntax/v1' && cache.parser === parser && Array.isArray(cache.entries) && cache.entries.length <= maximumEntries && createHash('sha256').update(JSON.stringify(cache.entries)).digest('hex')===cache.digest) {
        for (const pair of cache.entries) if (Array.isArray(pair) && pair.length === 2 && /^[a-f0-9]{64}$/.test(pair[0]) && valid(pair[1])) entries.set(pair[0], pair[1]);
        stats.loaded=entries.size>0;
      }
    }
  } catch { file = undefined; }
  return {
    stats,
    parse(name: string, source: string) {
      const key = createHash('sha256').update(name).update('\0').update(source).digest('hex');
      const previous = entries.get(key);
      if (previous) { stats.hits++; return previous; }
      stats.misses++; const parsed = parseWorkspaceSource(name, source); entries.set(key, parsed); return parsed;
    },
    flush() {
      if (!file || !stats.misses) return;
      const temporary = `${file}.${process.pid}.${randomUUID()}.tmp`;
      try {
        const rows=[...entries].slice(-maximumEntries);
        const content = JSON.stringify({ schema: 'erpx-workspace-source-syntax/v1', parser, entries: rows, digest:createHash('sha256').update(JSON.stringify(rows)).digest('hex') });
        if (Buffer.byteLength(content) > maximumBytes) return;
        writeFileSync(temporary, content, { mode: 0o600, flag: 'wx' }); renameSync(temporary, file); stats.persisted = true;
      } catch { try { unlinkSync(temporary); } catch { /* A cache miss still reparses real source. */ } }
    },
  };
}
