/** Explicit source build into a NEW user-owned Linux prefix. Never apt/brew/sudo. */
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, writeFileSync, writeSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import sources from '../../fixtures/files/native-toolchain/sources.json';
import { inspectNativeWorkerPrerequisites, nativeBuildEnvironment, nativeTool, runNativeBuildChild } from './native-worker-toolchain';

type Artifact = { file: string; url: string; bytes: number; sha256: string };
export function nativeProvisionPrefix(input: readonly string[]): string {
  const args = input[0] === '--' ? input.slice(1) : [...input];
  if (args.length !== 1 || !args[0].startsWith('--prefix=')) throw Error('NATIVE_WORKER_EXPLICIT_NEW_PREFIX_REQUIRED');
  const prefix = args[0].slice('--prefix='.length);
  if (!isAbsolute(prefix) || prefix.length > 4096 || /[\0\r\n]/.test(prefix) || resolve(prefix) === '/') throw Error('NATIVE_WORKER_ABSOLUTE_NEW_PREFIX_REQUIRED');
  return resolve(prefix);
}
export function verifyNativeArtifact(bytes: Uint8Array, artifact: Artifact) {
  if (bytes.length !== artifact.bytes || createHash('sha256').update(bytes).digest('hex') !== artifact.sha256) throw Error('NATIVE_WORKER_SOURCE_HASH_MISMATCH');
}
async function download(artifact: Artifact, directory: string) {
  if (!/^https:\/\/(github\.com|files\.pythonhosted\.org)\//.test(artifact.url) || !/^[a-zA-Z0-9_.-]+$/.test(artifact.file) || !/^[a-f0-9]{64}$/.test(artifact.sha256) || artifact.bytes > 32 * 1024 * 1024) throw Error('NATIVE_WORKER_PINNED_SOURCE_REQUIRED');
  const response = await fetch(artifact.url, { signal: AbortSignal.timeout(120_000) });
  if (!response.ok || !response.body) throw Error('NATIVE_WORKER_SOURCE_DOWNLOAD_FAILED');
  const fd = openSync(join(directory, artifact.file), 'wx', 0o600), hash = createHash('sha256');
  let bytes = 0;
  try {
    for await (const value of response.body) {
      bytes += value.length;
      if (bytes > artifact.bytes) throw Error('NATIVE_WORKER_SOURCE_DOWNLOAD_LIMIT');
      hash.update(value); let offset = 0;
      while (offset < value.length) offset += writeSync(fd, value, offset, value.length - offset);
    }
  } finally { closeSync(fd); }
  if (bytes !== artifact.bytes || hash.digest('hex') !== artifact.sha256) throw Error('NATIVE_WORKER_SOURCE_HASH_MISMATCH');
}
function observation(command: string, argv: string[], environment: NodeJS.ProcessEnv, root: string) {
  return execFileSync(command, argv, { cwd: root, env: environment, encoding: 'utf8', timeout: 15_000, maxBuffer: 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}
export async function provisionNativeWorker(prefix: string, root: string, environment = process.env) {
  if (process.platform !== 'linux' || !['x64', 'arm64'].includes(process.arch)) throw Error('NATIVE_WORKER_LINUX_NATIVE_BUILD_REQUIRED');
  if (!isAbsolute(prefix) || existsSync(prefix)) throw Error('NATIVE_WORKER_FRESH_PREFIX_REQUIRED');
  const selected = nativeBuildEnvironment(environment, prefix);
  const python = nativeTool('python3', undefined, selected), pkgConfig = nativeTool('pkg-config', environment.ERPX_NATIVE_PKG_CONFIG_BIN, selected);
  const tar = nativeTool('tar', undefined, selected), cc = nativeTool('cc', undefined, selected);
  selected.PKG_CONFIG = pkgConfig;
  // Build packages must already be installed by the explicit CI setup/operator.
  // Missing development closure fails here; no installer or package fallback.
  observation(pkgConfig, ['--exists', 'glib-2.0', 'gio-2.0', 'gobject-2.0', 'expat', 'libjpeg', 'libpng', 'libwebp', 'libwebpmux', 'libwebpdemux', 'libheif', 'libexif', 'lcms2', 'zlib'], selected, root);
  const compiler = observation(cc, ['--version'], selected, root), pythonVersion = observation(python, ['--version'], selected, root);
  const packageVersions = observation(nativeTool('dpkg-query', undefined, selected), ['-W', '-f=${Package}\t${Version}\n', ...sources.aptBuildDependencies], selected, root);
  const work = mkdtempSync(join(dirname(prefix), '.erpx-vips-build-'));
  mkdirSync(prefix, { mode: 0o700 });
  const receiptDirectory = join(root, '.scratch/dev/test-suites/native-worker'); mkdirSync(receiptDirectory, { recursive: true, mode: 0o700 });
  const receiptFile = join(receiptDirectory, 'provision.json');
  const ninja = sources.ninja[process.arch as 'x64' | 'arm64'];
  const artifacts = [sources.vips, sources.meson, ninja];
  const receipt: Record<string, unknown> = { version: 1, platform: process.platform, architecture: process.arch, prefix, work, status: 'prepared', compiler, pythonVersion, packageVersions, artifacts, mesonOptions: sources.mesonOptions, jobs: sources.buildJobs, runtimeCodecQualified: false, aptVersionsAreObservedNotArtifactPinned: true };
  const save = () => writeFileSync(receiptFile, JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 }); save();
  const execute = async (command: string, argv: string[]) => { if (await runNativeBuildChild(command, argv, selected, root) !== 0) throw Error('NATIVE_WORKER_PROVISION_COMMAND_FAILED'); };
  try {
    for (const artifact of artifacts) await download(artifact, work);
    receipt.sourceHashesVerified = true; save();
    const archive = join(work, sources.vips.file);
    const names = observation(tar, ['-tJf', archive], selected, root).split('\n');
    if (!names.length || names.some(name => !name.startsWith('vips-8.18.7/') || name.split('/').includes('..') || name.startsWith('/'))) throw Error('NATIVE_WORKER_ARCHIVE_LAYOUT_INVALID');
    await execute(tar, ['-xJf', archive, '--no-same-owner', '--no-same-permissions', '-C', work]);
    const venv = join(work, 'build-tools'); await execute(python, ['-m', 'venv', venv]);
    const pinnedRequirements = join(work, 'requirements.txt');
    writeFileSync(pinnedRequirements, `meson==1.12.1 --hash=sha256:${sources.meson.sha256}\nninja==1.13.2 --hash=sha256:${ninja.sha256}\n`, { mode: 0o600 });
    await execute(join(venv, 'bin/python'), ['-m', 'pip', 'install', '--no-index', '--no-deps', '--require-hashes', '--find-links', work, '-r', pinnedRequirements]);
    selected.PATH = `${join(venv, 'bin')}:${selected.PATH ?? ''}`;
    const meson = join(venv, 'bin/meson'), build = join(work, 'build');
    await execute(meson, ['setup', build, join(work, 'vips-8.18.7'), '--prefix', prefix, '--wrap-mode=nodownload', ...sources.mesonOptions]);
    await execute(meson, ['compile', '-C', build, '-j', String(sources.buildJobs)]);
    await execute(meson, ['install', '-C', build, '--no-rebuild']);
    receipt.buildOptions = JSON.parse(observation(meson, ['introspect', build, '--buildoptions'], selected, root));
    const verified = inspectNativeWorkerPrerequisites({ ...selected, ERPX_NATIVE_VIPS_PREFIX: prefix, ERPX_NATIVE_PKG_CONFIG_BIN: pkgConfig, ERPX_CARGO_BIN: environment.ERPX_CARGO_BIN, ERPX_RUSTC_BIN: environment.ERPX_RUSTC_BIN }, root);
    receipt.observation = verified.observation; receipt.status = 'succeeded';
    // Source/archive verification belongs to this build, never to a later probe.
    writeFileSync(join(prefix, 'erpx-native-build.json'), JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 }); save();
  } catch (error) { receipt.status = 'failed'; receipt.failure = error instanceof Error && /^NATIVE_WORKER_[A-Z_]+$/.test(error.message) ? error.message : 'NATIVE_WORKER_PROVISION_FAILED'; save(); throw error; }
}
if (import.meta.main) {
  try { await provisionNativeWorker(nativeProvisionPrefix(process.argv.slice(2)), resolve(dirname(fileURLToPath(import.meta.url)), '../..')); }
  catch (error) { console.error(error instanceof Error && /^NATIVE_WORKER_[A-Z_]+$/.test(error.message) ? error.message : 'NATIVE_WORKER_PROVISION_FAILED'); process.exitCode = 1; }
}
