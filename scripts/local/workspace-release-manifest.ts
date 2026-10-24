import { createHash } from 'node:crypto';
import { lstat, readFile, readdir, realpath } from 'node:fs/promises';
import { isAbsolute, join, resolve, sep } from 'node:path';
import { appDefinitions } from '@erpx/contracts/apps';
import { verifyAppArtifact } from './app-artifact-cache';

export type ReleaseArtifact = { path: string; receiptSha256: string; runtimeFingerprint: string; entry: string };
export type WorkspaceReleaseManifest = {
  schema: 'erpx-workspace-release/v2'; candidate: string; sourceFingerprint: string;
  productionCandidateFrozen: boolean; acceptance: false; acceptanceAdapter: null;
  node: '26.11.1'; bun: '1.4.3'; postgres: '18.6'; createdAt: string;
  apps: Record<string, ReleaseArtifact>; gateway: ReleaseArtifact; jobs: ReleaseArtifact;
};
const digest = (bytes: string | Buffer) => createHash('sha256').update(bytes).digest('hex');
const hash = /^[a-f0-9]{64}$/;
function localPath(root: string, path: string) {
  if (!path || isAbsolute(path) || path.includes('\\') || path.split('/').some(part => !part || part === '.' || part === '..')) throw new Error('RELEASE_ARTIFACT_PATH_INVALID');
  const result = resolve(root, path);
  if (!result.startsWith(root + sep)) throw new Error('RELEASE_ARTIFACT_PATH_INVALID');
  return result;
}
export async function releaseArtifactReference(root: string, path: string, expected: { app?: string; role?: 'gateway' | 'jobs'; runtimeFingerprint?: string }): Promise<ReleaseArtifact> {
  root = await realpath(root);
  const directory = localPath(root, path);
  if ((await lstat(directory)).isSymbolicLink() || !(await realpath(directory)).startsWith(root + sep)) throw new Error('RELEASE_ARTIFACT_ESCAPES_ROOT');
  const receiptPath=join(directory,'artifact.json');
  if (!(await lstat(receiptPath)).isFile()) throw new Error('RELEASE_RECEIPT_MUST_BE_FILE');
  const bytes = await readFile(receiptPath), receipt = JSON.parse(bytes.toString());
  if (receipt.schema !== (expected.app ? 'erpx-app-artifact/v2' : 'erpx-tool-artifact/v1') || receipt.app !== expected.app || receipt.role !== expected.role
    || receipt.acceptance !== false || receipt.node !== '26.11.1' || receipt.bun !== '1.4.3' || !hash.test(receipt.runtimeFingerprint)
    || expected.runtimeFingerprint && receipt.runtimeFingerprint !== expected.runtimeFingerprint) throw new Error('RELEASE_ARTIFACT_IDENTITY_MISMATCH');
  const entry = localPath(directory, receipt.entry);
  if (!receipt.files?.[receipt.entry] || !(await lstat(entry)).isFile() || !await verifyAppArtifact(directory, receipt)) throw new Error('RELEASE_ARTIFACT_INVENTORY_MISMATCH');
  return { path, receiptSha256: digest(bytes), runtimeFingerprint: receipt.runtimeFingerprint, entry: receipt.entry };
}
/** Verify every byte/link from the relocated manifest, with no source checkout. */
export async function verifyWorkspaceRelease(root: string, options: { candidate?: string; frozen?: boolean } = {}): Promise<WorkspaceReleaseManifest> {
  root = await realpath(root);
  if (!(await lstat(join(root,'release.json'))).isFile()) throw new Error('RELEASE_RECEIPT_MUST_BE_FILE');
  const top=(await readdir(root)).sort();
  if(JSON.stringify(top)!==JSON.stringify(['apps','gateway','jobs','release.json']))throw new Error('RELEASE_TREE_MISMATCH');
  const manifest = JSON.parse(await readFile(join(root, 'release.json'), 'utf8')) as WorkspaceReleaseManifest;
  if (manifest.schema !== 'erpx-workspace-release/v2' || !/^[a-f0-9]{40}$/.test(manifest.candidate) || !hash.test(manifest.sourceFingerprint)
    || manifest.acceptance !== false || manifest.acceptanceAdapter !== null || typeof manifest.productionCandidateFrozen !== 'boolean'
    || manifest.node !== '26.11.1' || manifest.bun !== '1.4.3' || manifest.postgres !== '18.6'
    || options.candidate && manifest.candidate !== options.candidate || options.frozen && !manifest.productionCandidateFrozen) throw new Error('EXACT_COMBINED_RELEASE_REQUIRED');
  const ids = appDefinitions.map(app => app.id).sort(), actual = Object.keys(manifest.apps ?? {}).sort();
  if (JSON.stringify(ids) !== JSON.stringify(actual) || !manifest.gateway || !manifest.jobs) throw new Error('EXACT_COMBINED_RELEASE_REQUIRED');
  if((await lstat(join(root,'apps'))).isSymbolicLink() || JSON.stringify((await readdir(join(root,'apps'))).sort())!==JSON.stringify(ids))throw new Error('RELEASE_TREE_MISMATCH');
  const paths = new Set<string>();
  for (const [id, reference] of [...ids.map(id => [id, manifest.apps[id]] as const), ['gateway',manifest.gateway] as const, ['jobs',manifest.jobs] as const]) {
    if (!reference || paths.has(reference.path)) throw new Error('RELEASE_ARTIFACT_IDENTITY_MISMATCH');
    paths.add(reference.path);
    const tool = id === 'gateway' || id === 'jobs';
    const checked = await releaseArtifactReference(root, reference.path, tool ? {role: id, runtimeFingerprint: manifest.sourceFingerprint} : {app: id});
    if (checked.path!==reference.path || checked.receiptSha256!==reference.receiptSha256 || checked.runtimeFingerprint!==reference.runtimeFingerprint || checked.entry!==reference.entry) throw new Error('RELEASE_ARTIFACT_REFERENCE_MISMATCH');
  }
  return manifest;
}
