/** Explicit development/CI compiler prerequisite. Never starts a worker/provider. */
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { accessSync, appendFileSync, closeSync, constants, fstatSync, mkdirSync, openSync, readSync, realpathSync, writeFileSync } from 'node:fs';
import { delimiter, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import sources from '../../fixtures/files/native-toolchain/sources.json';
import { nativeElfRunpath, nativeWorkerLinkFlags, qualifiedNativeLinkEnvironment } from './native-worker-link-flags';
import { qualifiedCargoBuildProfile } from './cargo-build-profile';

export type NativeWorkerMode = 'probe' | 'check' | 'clippy' | 'build' | 'export-env' | 'inspect-linked';
export function nativeWorkerMode(input: readonly string[]): NativeWorkerMode {
  const args = input[0] === '--' ? input.slice(1) : [...input];
  if (args.length !== 1 || !['probe', 'check', 'clippy', 'build', 'export-env', 'inspect-linked'].includes(args[0])) throw Error('NATIVE_WORKER_EXACT_MODE_REQUIRED');
  return args[0] as NativeWorkerMode;
}
export function nativeWorkerArguments(mode: NativeWorkerMode): string[] {
  if (['probe','export-env','inspect-linked'].includes(mode)) return [];
  if (mode === 'build') return ['build', '--locked', '-p', 'erpx-server-files', '--features', 'preview-provider', '--bin', 'erpx-render-preview'];
  return [mode, '--locked', '-p', 'erpx-service', '--all-targets', '--features', 'files-scan-worker,contracts', ...(mode === 'clippy' ? ['--', '-D', 'warnings'] : [])];
}
const childVariables = new Set(['PATH', 'HOME', 'USERPROFILE', 'TMPDIR', 'TMP', 'TEMP', 'LANG', 'LC_ALL', 'LC_CTYPE', 'TZ', 'CARGO_HOME', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN', 'CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY', 'http_proxy', 'https_proxy', 'all_proxy', 'no_proxy', 'SSL_CERT_FILE', 'SSL_CERT_DIR', 'CURL_CA_BUNDLE']);
export function nativeBuildEnvironment(environment: NodeJS.ProcessEnv, prefix: string): NodeJS.ProcessEnv {
  const buildProfile = qualifiedCargoBuildProfile(environment);
  const selected = Object.fromEntries(Object.entries(environment).filter(([key]) => childVariables.has(key)));
  const extra = environment.PKG_CONFIG_PATH ? environment.PKG_CONFIG_PATH.split(delimiter) : [];
  if (extra.some(value => !value || !isAbsolute(value) || /[\x00-\x1f\x7f]/.test(value) || value.length > 4096)) throw Error('NATIVE_WORKER_ABSOLUTE_PKG_CONFIG_PATH_REQUIRED');
  if (environment.PKG_CONFIG_SYSROOT_DIR || environment.PKG_CONFIG_ALLOW_CROSS || environment.PKG_CONFIG_LIBDIR) throw Error('NATIVE_WORKER_CROSS_OR_OVERRIDE_PKG_CONFIG_UNSUPPORTED');
  return { ...selected, ...buildProfile, NODE_ENV: environment.NODE_ENV ?? 'test', PKG_CONFIG_PATH: [...new Set([join(prefix, 'lib/pkgconfig'), ...extra])].join(delimiter), CARGO_BUILD_JOBS: String(sources.buildJobs) };
}
function absolute(value: string | undefined): string {
  if (!value || !isAbsolute(value) || value.length > 4096 || /[\x00-\x1f\x7f]/.test(value)) throw Error('NATIVE_WORKER_ABSOLUTE_PATH_REQUIRED');
  return resolve(value);
}
export function nativeTool(name: string, configured: string | undefined, environment: NodeJS.ProcessEnv): string {
  const candidates = configured ? [absolute(configured)] : (environment.PATH ?? '').split(delimiter).filter(isAbsolute).map(directory => join(directory, name));
  for (const file of candidates) {
    try { accessSync(file, constants.X_OK); return file; } catch { /* inspect the next explicit tool path */ }
  }
  throw Error('NATIVE_WORKER_EXECUTABLE_REQUIRED');
}
/** Read no more than the original bounded file size plus one detection byte. */
export function nativeFileObservation(file: string, ceiling: number) {
  const fd = openSync(file, constants.O_RDONLY);
  try {
    const before = fstatSync(fd);
    if (!before.isFile() || before.size < 1 || before.size > ceiling) throw Error('NATIVE_WORKER_FILE_LIMIT');
    const body = Buffer.alloc(before.size + 1); let length = 0, read: number;
    while ((read = readSync(fd, body, length, body.length - length, null)) > 0) { length += read; if (length === body.length) break; }
    const after = fstatSync(fd);
    if (length !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs || after.ino !== before.ino) throw Error('NATIVE_WORKER_FILE_CHANGED');
    return { path: realpathSync(file), bytes: length, sha256: createHash('sha256').update(body.subarray(0, length)).digest('hex'), text: () => body.subarray(0, length).toString('utf8') };
  } finally { closeSync(fd); }
}
function beneath(parent: string, child: string) { const path = relative(parent, child); return path === '' || (!path.startsWith('..') && !isAbsolute(path)); }
function probe(command: string, argv: string[], environment: NodeJS.ProcessEnv, root: string): string {
  try { return execFileSync(command, argv, { cwd: root, env: environment, encoding: 'utf8', timeout: 15_000, maxBuffer: 128 * 1024, stdio: ['ignore', 'pipe', 'pipe'] }).trim(); }
  catch { throw Error('NATIVE_WORKER_TOOL_PROBE_FAILED'); }
}
export function inspectNativeWorkerPrerequisites(environment: NodeJS.ProcessEnv = process.env, root = resolve('.')) {
  if (!['darwin', 'linux'].includes(process.platform) || !['arm64', 'x64'].includes(process.arch)) throw Error('NATIVE_WORKER_PLATFORM_UNSUPPORTED');
  if (!environment.ERPX_NATIVE_VIPS_PREFIX) throw Error('NATIVE_WORKER_VIPS_PREFIX_REQUIRED');
  const prefix = realpathSync(absolute(environment.ERPX_NATIVE_VIPS_PREFIX));
  const childEnvironment = nativeBuildEnvironment(environment, prefix);
  const pkgConfig = nativeTool('pkg-config', environment.ERPX_NATIVE_PKG_CONFIG_BIN, environment);
  const cargo = nativeTool('cargo', environment.ERPX_CARGO_BIN, environment);
  const rustc = nativeTool('rustc', environment.ERPX_RUSTC_BIN, environment);
  childEnvironment.PKG_CONFIG = pkgConfig;
  childEnvironment.RUSTC = rustc;
  const rust = probe(rustc, ['-vV'], childEnvironment, root);
  const host = /^host: (.+)$/m.exec(rust)?.[1], release = /^release: (.+)$/m.exec(rust)?.[1];
  const expectedHost = `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-${process.platform === 'darwin' ? 'apple-darwin' : 'unknown-linux-gnu'}`;
  if (release !== sources.rustVersion || host !== expectedHost || (environment.CARGO_BUILD_TARGET && environment.CARGO_BUILD_TARGET !== host)) throw Error('NATIVE_WORKER_EXACT_NATIVE_RUST_TARGET_REQUIRED');
  const cargoVersion = probe(cargo, ['--version'], childEnvironment, root);
  if (!cargoVersion.startsWith(`cargo ${sources.rustVersion} `)) throw Error('NATIVE_WORKER_PINNED_CARGO_REQUIRED');
  if (probe(pkgConfig, ['--modversion', 'vips'], childEnvironment, root) !== sources.vipsVersion) throw Error('NATIVE_WORKER_EXACT_VIPS_VERSION_REQUIRED');
  const pc = realpathSync(probe(pkgConfig, ['--variable=pcfiledir', 'vips'], childEnvironment, root));
  const selectedPrefix = realpathSync(probe(pkgConfig, ['--variable=prefix', 'vips'], childEnvironment, root));
  const lib = realpathSync(probe(pkgConfig, ['--variable=libdir', 'vips'], childEnvironment, root));
  const include = realpathSync(probe(pkgConfig, ['--variable=includedir', 'vips'], childEnvironment, root));
  if (selectedPrefix !== prefix || pc !== realpathSync(join(prefix, 'lib/pkgconfig')) || !beneath(prefix, lib) || !beneath(prefix, include)) throw Error('NATIVE_WORKER_PREFIX_MISMATCH');
  // Resolves the complete development Requires closure, not only a copied vips.pc.
  if (!probe(pkgConfig, ['--cflags', '--libs', 'vips'], childEnvironment, root)) throw Error('NATIVE_WORKER_DEVELOPMENT_METADATA_REQUIRED');
  const files = [nativeFileObservation(join(pc, 'vips.pc'), 64 * 1024), nativeFileObservation(join(include, 'vips/version.h'), 64 * 1024), nativeFileObservation(join(lib, process.platform === 'darwin' ? 'libvips.42.dylib' : 'libvips.so.42'), 64 * 1024 * 1024)];
  if (!files[1].text().includes(`#define VIPS_VERSION "${sources.vipsVersion}"`)) throw Error('NATIVE_WORKER_HEADER_VERSION_MISMATCH');
  if (!beneath(prefix, files[2].path)) throw Error('NATIVE_WORKER_LIBRARY_OUTSIDE_PREFIX');
  const encoded=nativeWorkerLinkFlags(process.platform,lib);
  if(environment.CARGO_ENCODED_RUSTFLAGS!==undefined&&environment.CARGO_ENCODED_RUSTFLAGS!==encoded)throw Error('NATIVE_WORKER_LINK_FLAGS_MISMATCH');
  Object.assign(childEnvironment,qualifiedNativeLinkEnvironment({ERPX_NATIVE_VIPS_PREFIX:prefix,ERPX_NATIVE_VIPS_LIBDIR:lib,CARGO_ENCODED_RUSTFLAGS:encoded}));
  const observation = { version: 1, cargoBuildProfile: qualifiedCargoBuildProfile(childEnvironment), platform: process.platform, architecture: process.arch, host, rustVersion: release, cargoVersion, vipsVersion: sources.vipsVersion, prefix, libdir:lib, pkgConfig, compilerLinkFlags:encoded?encoded.split('\u001f'):[], expectedSourceArchiveSha256: sources.vips.sha256, sourceArchiveVerifiedByThisProbe: false, runtimeCodecQualified: false, files: files.map(({ path, bytes, sha256 }) => ({ path, bytes, sha256 })) };
  return { observation, childEnvironment, cargo };
}
export async function runNativeBuildChild(command: string, argv: readonly string[], environment: NodeJS.ProcessEnv, root: string): Promise<number> {
  const child = spawn(command, [...argv], { cwd: root, env: environment, stdio: 'inherit', detached: true });
  let received: NodeJS.Signals | undefined;
  const signal = (value: NodeJS.Signals) => {
    received = value;
    if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
    try { process.kill(-child.pid, value); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; }
  };
  const interrupt = () => signal('SIGINT'), terminate = () => signal('SIGTERM');
  process.on('SIGINT', interrupt); process.on('SIGTERM', terminate);
  try {
    const result = await new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((accept, reject) => { child.once('error', reject); child.once('close', (code, signal) => accept({ code, signal })); });
    return received === 'SIGINT' || result.signal === 'SIGINT' ? 130 : received || result.signal ? 143 : result.code ?? 1;
  } finally { process.off('SIGINT', interrupt); process.off('SIGTERM', terminate); }
}
export async function runNativeWorker(input: readonly string[], environment = process.env, root = resolve('.')) {
  const mode = nativeWorkerMode(input), argv = nativeWorkerArguments(mode);
  const current = inspectNativeWorkerPrerequisites(environment, root);
  const directory = join(root, '.scratch/dev/test-suites/native-worker'); mkdirSync(directory, { recursive: true, mode: 0o700 });
  const receipt = { ...current.observation, mode, argv, status: 'prepared', exitCode: null as number | null };
  const output = join(directory, `${mode}.json`); writeFileSync(output, JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 });
  if(mode==='export-env'){
    if(environment.CI!=='true'||environment.GITHUB_ACTIONS!=='true'||!environment.GITHUB_ENV)throw Error('NATIVE_WORKER_EXPLICIT_GITHUB_ENV_REQUIRED');
    const exported={...qualifiedNativeLinkEnvironment(current.childEnvironment),...qualifiedCargoBuildProfile(current.childEnvironment),PKG_CONFIG_PATH:current.childEnvironment.PKG_CONFIG_PATH!};
    appendFileSync(environment.GITHUB_ENV,Object.entries(exported).map(([key,value])=>`${key}=${value}\n`).join(''));
  }
  if(mode==='inspect-linked')inspectLinkedNativeWorker(current,environment,root);
  const exit = argv.length ? await runNativeBuildChild(current.cargo, argv, current.childEnvironment, root) : 0;
  const after = inspectNativeWorkerPrerequisites(environment, root);
  if (JSON.stringify(after.observation) !== JSON.stringify(current.observation)) throw Error('NATIVE_WORKER_PREREQUISITES_CHANGED');
  receipt.status = exit === 0 ? 'succeeded' : 'failed'; receipt.exitCode = exit;
  writeFileSync(output, JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 });
  return exit;
}
/** Bun's POSIX env dictionary permits the exact minimal Rust child environment;
 * Next's Node-only ambient ProcessEnv shape is deliberately not used here. */
export function readOnlyNativeStartup(binary:string,root:string){
  const environment:Record<string,string>={LANG:'C',TZ:'UTC'};
  return Bun.spawnSync([binary],{cwd:root,env:environment,stdin:'ignore',stdout:'pipe',stderr:'pipe',timeout:5000,maxBuffer:64*1024});
}
function inspectLinkedNativeWorker(current:ReturnType<typeof inspectNativeWorkerPrerequisites>,environment:NodeJS.ProcessEnv,root:string){
  if(process.platform!=='linux')throw Error('NATIVE_WORKER_ELF_PLATFORM_REQUIRED');
  const target=resolve(environment.CARGO_TARGET_DIR??join(root,'target'));
  const binary=join(target,...(environment.CARGO_BUILD_TARGET?[current.observation.host!]:[]),'debug','erpx-render-preview');
  const before=nativeFileObservation(binary,128*1024*1024),readelf=nativeTool('readelf',undefined,environment);
  const dynamic=probe(readelf,['--dynamic','--wide',binary],{...current.childEnvironment,LANG:'C',LC_ALL:'C'},root);
  nativeElfRunpath(dynamic,current.observation.libdir);
  // argc!=3 reaches the real Rust entry point and exits before opening inputs.
  // Deliberately no Node or loader environment is given to this Rust child.
  const startup=readOnlyNativeStartup(binary,root);
  const after=nativeFileObservation(binary,128*1024*1024);
  if(before.sha256!==after.sha256)throw Error('NATIVE_WORKER_EXECUTABLE_CHANGED');
  if(startup.signalCode||startup.exitedDueToTimeout||startup.exitedDueToMaxBuffer||startup.exitCode!==1||startup.stdout.length||startup.stderr.length)throw Error('NATIVE_WORKER_CLEAN_STARTUP_REQUIRED');
  const directory=join(root,'.scratch/dev/test-suites/native-worker');
  writeFileSync(join(directory,'elf-startup.json'),JSON.stringify({version:1,binary:before.path,sha256:before.sha256,runpath:current.observation.libdir,needed:'libvips.so.42',startup:{argv:[],environmentKeys:['LANG','TZ'],exitCode:startup.exitCode,stdoutBytes:0,stderrBytes:0},scope:'ELF loader and argument-admission startup only',runtimeCodecQualified:false},null,2)+'\n',{mode:0o600});
}
if (import.meta.main) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  try { process.exitCode = await runNativeWorker(process.argv.slice(2), process.env, root); }
  catch (error) { console.error(error instanceof Error ? error.message : 'NATIVE_WORKER_FAILED'); process.exitCode = 1; }
}
