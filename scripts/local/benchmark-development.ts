import { createHash } from 'node:crypto';
import { spawn, type ChildProcess } from 'node:child_process';
import { open, readFile, readdir, mkdir, writeFile, cp, rm, rename, realpath, lstat, statfs } from 'node:fs/promises';
import { constants, existsSync } from 'node:fs';
import { resolve, join, relative, dirname } from 'node:path';

// Read-only by default. A run requires an empty 3100: an existing service is
// inspected and reported, never stopped. All children use the normal dev env.
const root = process.cwd();
const args = process.argv.slice(2);
const options = new Map<string, string>();
let run = false;
let snapshotRequested = false;
for (let index = 0; index < args.length; index++) {
  const key = args[index];
  if (key === '--run') run = true;
  else if (key === '--snapshot') snapshotRequested = true;
  else if (key === '--inspect') continue;
  else if (key === '--help') {
    console.log('bun scripts/local/benchmark-development.ts [--inspect | --run] [--snapshot | --reuse-snapshot PRIVATE_APP_DIR] [--variants webpack:4096,turbopack:8192,turbopack:4096,webpack:8192] [--paths /api/health,/login] [--cookie-file PRIVATE_FILE] [--cold-rounds 3] [--warm-rounds 20] [--restore-bundler webpack] [--restore-heap 8192]');
    process.exit(0);
  } else if (['--variants', '--paths', '--cookie-file', '--cold-rounds', '--warm-rounds', '--restore-bundler', '--restore-heap', '--timeout-ms', '--reuse-snapshot'].includes(key) && args[index + 1]) {
    options.set(key, args[++index]);
  } else throw new Error('UNKNOWN_BENCHMARK_OPTION');
}
const reusedSnapshot = options.get('--reuse-snapshot');
if (snapshotRequested && reusedSnapshot) throw new Error('SNAPSHOT_OPTIONS_ARE_MUTUALLY_EXCLUSIVE');
const frozenSource = snapshotRequested || !!reusedSnapshot;
if (Bun.version !== '1.2.17') throw new Error('BUN_1_2_17_REQUIRED');
const node = resolve(process.env.ERPX_NODE_BIN ?? '.runtime/node/node-v24.20.0-darwin-arm64/bin/node');
if (Bun.spawnSync([node, '--version']).stdout.toString().trim() !== 'v24.20.0') throw new Error('NODE_24_20_0_REQUIRED');
if (process.env.ERPX_ENVIRONMENT === 'staging' || process.env.NODE_ENV === 'production') throw new Error('DEVELOPMENT_ENVIRONMENT_REQUIRED');
const fixedOrigin = 'https://console-dev-erpx.jesadakorn.com';
if (process.env.ERPX_PUBLIC_ORIGIN && process.env.ERPX_PUBLIC_ORIGIN !== fixedOrigin) throw new Error('NORMAL_DEVELOPMENT_ORIGIN_REQUIRED');
type Variant = { bundler: 'webpack' | 'turbopack'; heapMiB: 4096 | 8192 };
function variant(input: string): Variant {
  const [bundler, heap] = input.split(':');
  if (!['webpack', 'turbopack'].includes(bundler) || !['4096', '8192'].includes(heap)) throw new Error('INVALID_BENCHMARK_VARIANT');
  return { bundler: bundler as Variant['bundler'], heapMiB: Number(heap) as Variant['heapMiB'] };
}
const variants = (options.get('--variants') ?? 'webpack:4096,turbopack:8192,turbopack:4096,webpack:8192').split(',').map(variant);
const restoredVariant = variant(`${options.get('--restore-bundler') ?? 'webpack'}:${options.get('--restore-heap') ?? '8192'}`);
function positive(name: string, fallback: number) {
  const value = Number(options.get(name) ?? fallback);
  if (!Number.isInteger(value) || value < 1) throw new Error('INVALID_BENCHMARK_COUNT');
  return value;
}
const coldRounds = positive('--cold-rounds', 3), warmRounds = positive('--warm-rounds', 20);
const timeoutMs = positive('--timeout-ms', 30_000);
const paths = (options.get('--paths') ?? '/api/health,/login').split(',');
for (const path of paths) {
  if (!/^\/(?:api\/health|api\/v1\/[^\s?#]+|login|holdings\/[^\s?#]+|front\/[^\s?#]+)(?:\?[^\s#]*)?$/.test(path) || path.includes('..')) throw new Error('INVALID_DEVELOPMENT_BENCHMARK_PATH');
}
function targetSelection(path: string) {
  const target = new URL(path, fixedOrigin);
  const selectionQuery: Record<string, string> = {};
  let queryParametersOmitted = false;
  for (const [key, value] of target.searchParams) {
    const allowed = (key === 'paged' && ['1', 'true'].includes(value)) || (key === 'view' && value === 'dashboard');
    if (allowed && target.searchParams.getAll(key).length === 1) selectionQuery[key] = value;
    else queryParametersOmitted = true;
  }
  return { path: target.pathname, selectionQuery, queryParametersOmitted };
}
function safeTargetPath(path: string) {
  const target = targetSelection(path), query = new URLSearchParams(target.selectionQuery).toString();
  return target.path + (query ? `?${query}` : '');
}
const cookiePath = options.get('--cookie-file');
const cookie = cookiePath ? (await readFile(resolve(cookiePath), 'utf8')).trim() : undefined;
if (cookie?.includes('\n') || cookie?.includes('\r')) throw new Error('COOKIE_HEADER_FILE_REQUIRED');

function command(argv: string[]) { return Bun.spawnSync(argv).stdout.toString().trim(); }
function listeners() {
  return command(['lsof', '-nP', '-iTCP:3100', '-sTCP:LISTEN', '-Fp']).split('\n').filter(line => /^p\d+$/.test(line)).map(line => Number(line.slice(1)));
}
function cwd(pid: number) { return command(['lsof', '-a', '-p', String(pid), '-d', 'cwd', '-Fn']).split('\n').find(line => line.startsWith('n'))?.slice(1); }
const sourceFiles = (await readdir(join(root, 'src'), { recursive: true, withFileTypes: true })).filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name)).sort();
async function sourceHash(base = root) {
  const hash = createHash('sha256');
  const currentFiles = (await readdir(join(base, 'src'), { recursive: true, withFileTypes: true })).filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name)).sort();
  for (const file of [...currentFiles, ...['bun.lock', 'package.json', 'next.config.ts', 'tsconfig.json', 'postcss.config.mjs'].map(name => join(base, name))]) {
    hash.update(file.slice(base.length + 1)).update('\0').update(await readFile(file)).update('\0');
  }
  return hash.digest('hex');
}
const initialHash = await sourceHash();
const dependencies = [];
const sourceText = await Promise.all(sourceFiles.filter(file => /\.[cm]?[jt]sx?$/.test(file)).map(async file => ({ file: file.slice(root.length + 1), text: await readFile(file, 'utf8') })));
for (const name of ['@tiptap', 'pdfjs-dist', 'pdf-lib', '@aws-sdk', 'lucide-react', 'better-auth', 'drizzle-orm']) {
  const size = Number(command(['du', '-sk', join(root, 'node_modules', name)]).split(/\s+/)[0]);
  dependencies.push({ name, installedKiB: size, referencingFiles: sourceText.filter(source => source.text.includes(`"${name}`) || source.text.includes(`'${name}`)).map(source => source.file) });
}
const inspection = {
  event: 'development_benchmark_inspection', sourceSha256: initialHash, node: '24.20.0', bun: Bun.version,
  listeners: listeners().map(pid => ({ pid, cwd: cwd(pid) })), variants, coldRounds, warmRounds,
  paths: paths.map(safeTargetPath), targetSelections: paths.map(targetSelection), authenticatedCookieSupplied: !!cookie, readRequestMetricsEnabled: true, snapshotRequested, reusedSnapshot: reusedSnapshot ? relative(root, resolve(reusedSnapshot)) : null, dependencies,
  limitations: ['Installed dependency size and static references identify candidates, not measured compilation cost.', 'HTTP document requests are not browser client navigation.', 'The HMR probe measures server hot compilation, not browser Fast Refresh.', 'heapMiB is the old-space budget per Node process; aggregate RSS includes child processes and native memory.', 'Browser traffic, other processes and development database writes can still affect results even with fixed snapshot code.', ...(frozenSource ? ['Snapshot contents are verified stable over copying; this does not establish business consistency of another task\'s in-progress edits.', 'The snapshot clones installed dependencies without downloading or installing; the pinned lockfile is checked, but installed package contents are assumed unchanged during the experiment.', 'Cold rounds use fresh compiler output; source copying and prior rounds can warm operating-system file caches.', 'Final restored development runs the latest original workspace source, which may differ from the benchmark snapshot.'] : []), ...(!cookie ? ['Protected routes have no authenticated coverage in this launcher.'] : [])],
};
console.log(JSON.stringify(inspection));
if (!run) process.exit(0);
if (inspection.listeners.length) throw new Error('PORT_3100_OCCUPIED_EXISTING_SERVICE_WAS_NOT_STOPPED');
if (!frozenSource && existsSync(join(root, '.next'))) throw new Error('COLD_OUTPUT_NOT_EMPTY_USE_SNAPSHOT');

const directory = join(root, '.scratch/dev', `bundler-${new Date().toISOString().replace(/[:.]/g, '-')}`);
await mkdir(directory, { recursive: true, mode: 0o700 });
const evidence: Record<string, unknown> = { ...inspection, startedAt: new Date().toISOString(), rounds: [], hmr: [], resources: [] };
const results = evidence.rounds as unknown[], hmrResults = evidence.hmr as unknown[], resourceResults = evidence.resources as unknown[];
async function save() {
  const temporary = join(directory, 'evidence.json.tmp');
  await writeFile(temporary, JSON.stringify(evidence, null, 2) + '\n', { mode: 0o600 });
  await rename(temporary, join(directory, 'evidence.json'));
}
async function diskPreflight(phase: string, minimumGiB = 4) {
  const disk = await statfs(root), availableBytes = disk.bavail * disk.bsize;
  const samples = (evidence.diskSpace ??= []) as unknown[];
  samples.push({ phase, availableBytes, minimumBytes: minimumGiB * 1024 ** 3, observedAt: new Date().toISOString() });
  await save();
  if (availableBytes < minimumGiB * 1024 ** 3) throw new Error('INSUFFICIENT_DISK_HEADROOM');
}
const pause = (ms: number) => new Promise<void>(resolvePause => setTimeout(resolvePause, ms));
type Owned = { child: ChildProcess; pid: number; cwd: string; started: number; log: string; sample: { peakRssKiB: number; peakPsCpuPercent: number; samples: number } };
let active: Owned | undefined;
let sampling = false;
let benchmarkRoot = root;
let benchmarkHash = initialHash;
const snapshotTrees = ['src', 'public', 'scripts', 'ops/macos', 'migrations/postgres'];
const snapshotFiles = ['AGENTS.md', 'package.json', 'bun.lock', 'next.config.ts', 'tsconfig.json', 'postcss.config.mjs'];
type SnapshotManifest = { sha256: string; files: { path: string; sha256: string; bytes: number }[] };
async function snapshotManifest(base: string): Promise<SnapshotManifest> {
  const files = [...snapshotFiles];
  for (const tree of snapshotTrees) {
    const entries = await readdir(join(base, tree), { recursive: true, withFileTypes: true });
    files.push(...entries.filter(entry => entry.isFile()).map(entry => relative(base, join(entry.parentPath, entry.name))));
  }
  const hash = createHash('sha256'), manifest: SnapshotManifest['files'] = [];
  for (const path of files.sort()) {
    const bytes = await readFile(join(base, path)), digest = createHash('sha256').update(bytes).digest('hex');
    hash.update(path).update('\0').update(digest).update('\0');
    manifest.push({ path, sha256: digest, bytes: bytes.byteLength });
  }
  return { sha256: hash.digest('hex'), files: manifest };
}
async function prepareSnapshot() {
  await diskPreflight('snapshot-preparation', 6);
  const app = join(directory, 'app');
  for (let attempt = 1; attempt <= 3; attempt++) {
    const before = await snapshotManifest(root);
    await rm(app, { recursive: true, force: true });
    await mkdir(app, { recursive: true, mode: 0o700 });
    for (const path of [...snapshotTrees, ...snapshotFiles]) {
      await cp(join(root, path), join(app, path), { recursive: true, mode: constants.COPYFILE_FICLONE, dereference: true });
    }
    const [after, copied] = await Promise.all([snapshotManifest(root), snapshotManifest(app)]);
    if (before.sha256 !== after.sha256 || before.sha256 !== copied.sha256) {
      console.log(JSON.stringify({ event: 'development_snapshot_retry', attempt, reason: 'source_changed_during_copy' }));
      continue;
    }
    const dependenciesStarted = performance.now();
    // APFS cloning avoids out-of-root dependency symlinks. Turbopack can keep the
    // snapshot as its project root, so live workspace edits do not invalidate it.
    await cp(join(root, 'node_modules'), join(app, 'node_modules'), {
      recursive: true, mode: constants.COPYFILE_FICLONE, dereference: true,
    });
    const lock = await readFile(join(root, 'bun.lock'));
    if (createHash('sha256').update(lock).digest('hex') !== copied.files.find(file => file.path === 'bun.lock')?.sha256) {
      throw new Error('DEPENDENCY_LOCK_CHANGED_DURING_SNAPSHOT');
    }
    await writeFile(join(directory, 'snapshot-manifest.json'), JSON.stringify(copied, null, 2) + '\n', { mode: 0o600 });
    benchmarkRoot = app;
    benchmarkHash = await sourceHash(app);
    evidence.originalSourceSha256AtInspection = initialHash;
    evidence.sourceSha256 = benchmarkHash;
    evidence.snapshot = { cwd: app, sourceSha256: benchmarkHash, inputManifestSha256: copied.sha256,
      files: copied.files.length, nodeModulesCopyMs: performance.now() - dependenciesStarted,
      dependencyCopy: 'COPYFILE_FICLONE-with-filesystem-fallback', runtimeNode: 'absolute-original-pinned-node',
      canonicalDevelopmentData: true, preparationAttempt: attempt };
    await save();
    console.log(JSON.stringify({ event: 'development_snapshot_ready', ...evidence.snapshot as object }));
    return;
  }
  throw new Error('SOURCE_DID_NOT_STABILIZE_FOR_SNAPSHOT');
}
async function reuseSnapshot(path: string) {
  const app = await realpath(resolve(path));
  if (!/^\.scratch\/dev\/bundler-[^/]+\/app$/.test(relative(root, app))) throw new Error('OWNED_SNAPSHOT_DIRECTORY_REQUIRED');
  const previousDirectory = dirname(app);
  const previous = JSON.parse(await readFile(join(previousDirectory, 'evidence.json'), 'utf8')) as {
    snapshot?: { cwd: string; sourceSha256: string; inputManifestSha256: string };
  };
  const manifest = JSON.parse(await readFile(join(previousDirectory, 'snapshot-manifest.json'), 'utf8')) as SnapshotManifest;
  if (previous.snapshot?.cwd !== app || previous.snapshot.inputManifestSha256 !== manifest.sha256) throw new Error('SNAPSHOT_EVIDENCE_MISMATCH');
  const [copied, copiedHash, dependencies] = await Promise.all([snapshotManifest(app), sourceHash(app), lstat(join(app, 'node_modules'))]);
  if (copied.sha256 !== manifest.sha256 || copiedHash !== previous.snapshot.sourceSha256) throw new Error('REUSED_SNAPSHOT_SOURCE_CHANGED');
  if (!dependencies.isDirectory() || dependencies.isSymbolicLink()) throw new Error('CONTAINED_SNAPSHOT_DEPENDENCIES_REQUIRED');
  if (command(['lsof', '-t', '-a', '-d', 'cwd', app])) throw new Error('SNAPSHOT_PROCESS_STILL_RUNNING');
  // A prior failed compiler's output is disposable only in this validated,
  // stopped snapshot. Original workspace output is never cleared here.
  await rm(join(app, '.next'), { recursive: true, force: true });
  benchmarkRoot = app;
  benchmarkHash = copiedHash;
  evidence.originalSourceSha256AtInspection = initialHash;
  evidence.sourceSha256 = copiedHash;
  evidence.continuedFrom = relative(root, join(previousDirectory, 'evidence.json'));
  evidence.snapshot = { ...previous.snapshot, reused: true, files: copied.files.length,
    runtimeNode: 'absolute-original-pinned-node', canonicalDevelopmentData: true };
  await save();
  console.log(JSON.stringify({ event: 'development_snapshot_reused', ...evidence.snapshot as object }));
}
function processRows() {
  return command(['ps', '-axo', 'pid=,ppid=,rss=,%cpu=']).split('\n').map(line => line.trim().split(/\s+/).map(Number)).filter(row => row.length === 4);
}
function descendantPids(parent: number, rows = processRows()) {
  const included = new Set([parent]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const [pid, ppid] of rows) if (included.has(ppid) && !included.has(pid)) { included.add(pid); changed = true; }
  }
  return included;
}
function sample(owned: Owned) {
  const rows = processRows(), pids = descendantPids(owned.pid, rows);
  const members = rows.filter(row => pids.has(row[0]));
  owned.sample.peakRssKiB = Math.max(owned.sample.peakRssKiB, members.reduce((sum, row) => sum + row[2], 0));
  owned.sample.peakPsCpuPercent = Math.max(owned.sample.peakPsCpuPercent, members.reduce((sum, row) => sum + row[3], 0));
  owned.sample.samples++;
}
async function start(config: Variant, output: string, appRoot = benchmarkRoot): Promise<Owned> {
  if (listeners().length) throw new Error('PORT_3100_OCCUPIED');
  const log = join(directory, `${output}.log`), descriptor = await open(log, 'a', 0o600);
  // Installed Next's webpack watcher excludes .next by name, not an arbitrary
  // configured distDir. Keep canonical output to avoid watching its own writes.
  const buildDir = '.next';
  const configPath = appRoot === root ? join(directory, `${output}.tsconfig.json`) : join(appRoot, '.scratch/dev', `${output}.tsconfig.json`);
  await mkdir(dirname(configPath), { recursive: true, mode: 0o700 });
  const baseConfig = JSON.parse(await readFile(join(appRoot, 'tsconfig.json'), 'utf8')) as { include: string[]; exclude: string[] };
  const typeGlobs = [`${buildDir}/types/**/*.ts`, `${buildDir}/dev/types/**/*.ts`];
  await writeFile(configPath, JSON.stringify({
    extends: relative(dirname(configPath), join(appRoot, 'tsconfig.json')),
    compilerOptions: { baseUrl: appRoot },
    include: [...baseConfig.include.map(pattern => resolve(appRoot, pattern)), ...typeGlobs, ...typeGlobs.map(pattern => resolve(appRoot, pattern))],
    exclude: baseConfig.exclude.map(pattern => resolve(appRoot, pattern)),
  }, null, 2) + '\n', { mode: 0o600 });
  const nodeOptions = (process.env.NODE_OPTIONS ?? '').replace(/(?:^|\s)--max-old-space-size(?:=|\s+)\d+/g, '').trim();
  const started = performance.now();
  const child = spawn(process.execPath, ['run', 'dev', `--${config.bundler}`], {
    cwd: appRoot, detached: true, stdio: ['ignore', descriptor.fd, descriptor.fd],
    env: { ...process.env, NODE_ENV: 'development', ERPX_NODE_BIN: node, ERPX_PUBLIC_ORIGIN: fixedOrigin, ERPX_PERF_LOG: '1',
      PATH: `${dirname(process.execPath)}:${process.env.PATH ?? ''}`,
      ERPX_DATA_DIR: resolve(root, process.env.ERPX_DATA_DIR ?? '.runtime/local'),
      ERPX_BUILD_DIR: buildDir, ERPX_DEV_TSCONFIG: relative(appRoot, configPath),
      NODE_OPTIONS: `${nodeOptions} --max-old-space-size=${config.heapMiB}`.trim() },
  });
  await descriptor.close();
  if (!child.pid) throw new Error('DEVELOPMENT_LAUNCH_FAILED');
  const owned: Owned = { child, pid: child.pid, cwd: appRoot, started, log, sample: { peakRssKiB: 0, peakPsCpuPercent: 0, samples: 0 } };
  active = owned;
  child.unref();
  await writeFile(join(directory, 'active.pid'), String(owned.pid) + '\n', { mode: 0o600 });
  const deadline = Date.now() + 60_000;
  while (!listeners().length) {
    if (child.exitCode !== null || Date.now() > deadline) throw new Error('DEVELOPMENT_STARTUP_FAILED');
    sample(owned);
    await pause(250);
  }
  const ownedPids = descendantPids(owned.pid);
  if (listeners().some(pid => !ownedPids.has(pid) || cwd(pid) !== appRoot)) throw new Error('DEVELOPMENT_LISTENER_IDENTITY_MISMATCH');
  results.push({ ...config, output, cwd: appRoot, phase: 'tcp_ready', durationMs: performance.now() - started });
  return owned;
}
async function stop(owned: Owned) {
  // Only signal the detached group created during this run. Never adopt a PID
  // read from another launcher or from the port inventory.
  const pgid = Number(command(['ps', '-p', String(owned.pid), '-o', 'pgid=']));
  if (pgid && (pgid !== owned.pid || cwd(owned.pid) !== owned.cwd)) throw new Error('OWNED_LAUNCHER_IDENTITY_CHANGED');
  if (pgid) process.kill(-owned.pid, 'SIGTERM');
  const deadline = Date.now() + 15_000;
  const groupAlive = () => command(['ps', '-axo', 'pid=,pgid=']).split('\n').some(line => {
    const [pid, group] = line.trim().split(/\s+/).map(Number);
    return !!pid && group === owned.pid;
  });
  while (listeners().length || groupAlive()) {
    if (Date.now() > deadline) throw new Error('OWNED_DEVELOPMENT_DID_NOT_STOP');
    await pause(250);
  }
  resourceResults.push({ launcherPid: owned.pid, cwd: owned.cwd, log: owned.log.slice(root.length + 1), ...owned.sample });
  if (active === owned) active = undefined;
}
async function archiveOutput(owned: Owned, output: string) {
  if (active === owned) throw new Error('RUNNING_DEV_OUTPUT_MUST_NOT_BE_ARCHIVED');
  const source = join(owned.cwd, '.next');
  if (!existsSync(source)) throw new Error('OWNED_DEV_OUTPUT_MISSING');
  const destination = join(directory, 'archived_outputs', output, '.next');
  await mkdir(dirname(destination), { recursive: true, mode: 0o700 });
  await rename(source, destination);
  // Compiler caches are reproducible and can exhaust disk across a matrix.
  // Preserve generated manifests/chunks and all request evidence in the archive.
  for (const cache of ['cache', 'dev/cache']) await rm(join(destination, cache), { recursive: true, force: true });
  const archives = (evidence.archivedOutputs ??= []) as unknown[];
  archives.push({ output, path: relative(root, destination), launcherPid: owned.pid, cwd: owned.cwd, compilerCachesPruned: true });
}
async function request(origin: string, path: string, requestTimeoutMs = timeoutMs) {
  const started = performance.now();
  try {
    const response = await fetch(origin + path, { redirect: 'manual', signal: AbortSignal.timeout(requestTimeoutMs), headers: cookie ? { Cookie: cookie } : undefined });
    const body = await response.arrayBuffer(), bytes = body.byteLength;
    let validApi = true, streamingRedirect = false, rscFallbackError = false;
    if (path.startsWith('/api/v1/') && response.status === 200) {
      try { const data = JSON.parse(new TextDecoder().decode(body)); validApi = !!data && 'data' in data && !('error' in data); } catch { validApi = false; }
    } else if (!path.startsWith('/api/')) {
      const html = new TextDecoder().decode(body);
      streamingRedirect = html.includes('NEXT_REDIRECT;') || /<meta\b[^>]*http-equiv\s*=\s*["']refresh["']/i.test(html);
      const errorDocument = /<html\b[^>]*\bid\s*=\s*(?:"__next_error__"|'__next_error__')/i.test(html);
      const fallbackDigests = [...html.matchAll(/<template\b[^>]*\bdata-dgst\s*=\s*(?:"([^"]*)"|'([^']*)')/gi)];
      rscFallbackError = errorDocument || fallbackDigests.some(match => !['BAILOUT_TO_CLIENT_SIDE_RENDERING', 'NEXT_REDIRECT'].some(allowed => (match[1] ?? match[2]).startsWith(allowed)));
    }
    return { durationMs: performance.now() - started, status: response.status, bytes, validApi, streamingRedirect, rscFallbackError,
      success: response.status === 200 && validApi && !streamingRedirect && !rscFallbackError,
      error: streamingRedirect ? 'StreamedRedirect' : rscFallbackError ? 'ServerComponentFallback' : !validApi ? 'InvalidApiEnvelope' : null,
      serverTiming: response.headers.get('server-timing'), readRequestId: response.headers.get('x-erpx-read-id'), probe: response.headers.get('x-erpx-hmr-probe') };
  } catch (error) {
    return { durationMs: performance.now() - started, status: null, bytes: null, success: false, error: error instanceof Error ? error.name : 'Error', probe: null };
  }
}
async function hotEdit(config: Variant) {
  const healthSource = join(benchmarkRoot, 'src/app/api/health/route.ts');
  const original = await readFile(healthSource, 'utf8');
  const needle = 'headers: { "Cache-Control": "no-store" }';
  if (!original.includes(needle)) throw new Error('HMR_PROBE_SOURCE_SHAPE_CHANGED');
  const marker = `benchmark-${config.bundler}-${config.heapMiB}-${Date.now()}`;
  const edited = original.replace(needle, `headers: { "Cache-Control": "no-store", "X-ERPX-HMR-Probe": "${marker}" }`);
  const started = performance.now();
  let changed = false;
  try {
    await writeFile(healthSource, edited);
    changed = true;
    const deadline = Date.now() + 90_000;
    while (Date.now() < deadline) {
      const response = await request('http://127.0.0.1:3100', '/api/health', Math.max(1, Math.min(timeoutMs, deadline - Date.now())));
      if (response.probe === marker) { hmrResults.push({ ...config, phase: 'edit', durationMs: performance.now() - started, status: response.status }); return; }
      await pause(250);
    }
    hmrResults.push({ ...config, phase: 'edit', durationMs: performance.now() - started, failed: true });
  } finally {
    if (changed) {
      if (await readFile(healthSource, 'utf8') !== edited) throw new Error('HMR_SOURCE_CHANGED_BY_ANOTHER_WRITER_RESTORE_REQUIRED');
      await writeFile(healthSource, original);
      const restored = performance.now();
      const deadline = Date.now() + 90_000;
      let status: number | null = null, probeRemoved = false;
      while (Date.now() < deadline) {
        const response = await request('http://127.0.0.1:3100', '/api/health', Math.max(1, Math.min(timeoutMs, deadline - Date.now())));
        status = response.status;
        if (response.status === 200 && response.probe === null) { probeRemoved = true; break; }
        await pause(250);
      }
      hmrResults.push({ ...config, phase: 'restore', durationMs: performance.now() - restored, status, failed: !probeRemoved });
    }
  }
}
const timer = setInterval(() => { if (active && !sampling) { sampling = true; try { sample(active); } finally { sampling = false; } } }, 500);
timer.unref();
let interrupted = false;
for (const signal of ['SIGINT', 'SIGTERM'] as const) process.on(signal, () => {
  interrupted = true;
  const received = { signal, receivedAt: new Date().toISOString(), runnerPid: process.pid };
  ((evidence.receivedSignals ??= []) as unknown[]).push(received);
  console.log(JSON.stringify({ event: 'development_benchmark_signal', ...received }));
});
try {
  if (snapshotRequested) await prepareSnapshot();
  else if (reusedSnapshot) await reuseSnapshot(reusedSnapshot);
  for (const config of variants) {
    for (let cold = 1; cold <= coldRounds; cold++) {
      if (interrupted) throw new Error('BENCHMARK_INTERRUPTED');
      if (await sourceHash(benchmarkRoot) !== benchmarkHash) throw new Error('SOURCE_CHANGED_DURING_COMPARISON');
      const output = `${config.bundler}-${config.heapMiB}-cold-${cold}`;
      await diskPreflight(output);
      const owned = await start(config, output);
      for (const path of paths) results.push({ ...config, phase: 'cold', coldRound: cold, origin: 'local', ...targetSelection(path), ...await request('http://127.0.0.1:3100', path) });
      if (cold === coldRounds) {
        for (let warm = 1; warm <= warmRounds; warm++) {
          if (interrupted) throw new Error('BENCHMARK_INTERRUPTED');
          for (const path of paths) for (const [origin, url] of [['local', 'http://127.0.0.1:3100'], ['public', fixedOrigin]]) {
            results.push({ ...config, phase: 'warm', round: warm, origin, ...targetSelection(path), ...await request(url, path) });
          }
          await save();
        }
        await hotEdit(config);
      }
      await stop(owned);
      await archiveOutput(owned, output);
      await save();
      console.log(JSON.stringify({ event: 'development_benchmark_round_complete', ...config, coldRound: cold, evidence: join(directory, 'evidence.json') }));
    }
  }
} catch (error) {
  evidence.failure = error instanceof Error ? error.message : 'BENCHMARK_FAILED';
  process.exitCode = 1;
} finally {
  clearInterval(timer);
  try {
    if (active) await stop(active);
    const restored = await start(restoredVariant, 'restored-dev', root);
    const local = await request('http://127.0.0.1:3100', '/api/health');
    const publicHealth = await request(fixedOrigin, '/api/health');
    // Listener identity is rechecked after both startup and the first requests.
    const persistent = listeners().every(pid => descendantPids(restored.pid).has(pid) && cwd(pid) === root) && listeners().length > 0;
    evidence.restoredDevelopment = { ...restoredVariant, cwd: root, launcherPid: restored.pid, local, public: publicHealth, persistent };
    if (local.status !== 200 || publicHealth.status !== 200 || !persistent) process.exitCode = 1;
  } catch (error) {
    evidence.restoreFailure = error instanceof Error ? error.message : 'RESTORE_FAILED';
    process.exitCode = 1;
  }
  evidence.completedAt = new Date().toISOString();
  await save();
  console.log(JSON.stringify({ event: 'development_benchmark_complete', evidence: join(directory, 'evidence.json'), restoredDevelopment: evidence.restoredDevelopment, failure: evidence.failure, restoreFailure: evidence.restoreFailure }));
}
