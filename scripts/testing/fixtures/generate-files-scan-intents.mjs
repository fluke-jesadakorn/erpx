// Development-only source-bound oracle. Executes the unchanged B0 function;
// ports record effects, not SQL acceptance, current authority or file safety.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { Script, createContext } from 'node:vm';
import { execFileSync } from 'node:child_process';

const root = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: new URL('.', import.meta.url), encoding: 'utf8' }).trim();
if (process.argv.length > 3) throw new Error('ORACLE_EXPECTS_OPTIONAL_OUTPUT_PATH');
const output = process.argv[2] ?? `${root}/fixtures/files/scan-lifecycle/b0-scan-intents.json`;
const baseline = 'a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const sourcePath = 'packages/server-files/src/application/attachments/service.ts';
const legacyPath = 'packages/server-files/src/domain/attachments/legacy-policy.ts';
const original = execFileSync('git', ['show', `${baseline}:${sourcePath}`], { cwd: root, maxBuffer: 1024 * 1024 }).toString();
const legacyOriginal = execFileSync('git', ['show', `${baseline}:${legacyPath}`], { cwd: root, maxBuffer: 1024 * 1024 }).toString();
const current = await readFile(`${root}/${sourcePath}`, 'utf8');
const currentLegacy = await readFile(`${root}/${legacyPath}`, 'utf8');
if (original !== current || legacyOriginal !== currentLegacy) throw new Error('B0_SOURCE_CHANGED');
const hash = value => createHash('sha256').update(value).digest('hex');
const sourceFunction = (start, end) => original.slice(original.indexOf(start), original.indexOf(end, original.indexOf(start)));
const functions = [
  sourceFunction('async function requireScannerReady(', '\nasync function setRow('),
  sourceFunction('async function drainAttachmentScans(', '\nasync function scanAttachmentNow('),
  sourceFunction('function safeJson(value:', '\n\nfunction safeUploadErrorCode('),
].join('\n');
const transpile = text => ts.transpileModule(text, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText;
const normalize = value => JSON.parse(JSON.stringify(value, (_key, item) => item instanceof Uint8Array ? { bytes: item.byteLength, sha256: hash(item) } : item));
const cases = [];
async function capture(name, input = {}) {
  const events = [];
  const payload = Buffer.from('owned synthetic quarantine input');
  const source = { id: 'attachment-owned', upload_id: 'upload-owned', company_id: 'company-owned', object_key: input.objectKey ?? 'quarantine/owned/original', media_type: 'text/csv', checksum_sha256: hash(payload), ...(input.source ?? {}) };
  const now = 1_800_000_000_000;
  let sequence = 0;
  const ports = {
    clock: { now: () => now }, identifiers: { next: () => `id-${++sequence}` },
    environment: () => ({ ERPX_ATTACHMENTS_V2: input.enabled === false ? 'false' : 'true', marker: 'owned' }),
    error: (_status, code) => new Error(code),
    digest: async bytes => hash(new Uint8Array(bytes)),
  };
  const providers = {
    async attachmentScannerReadiness(environment) { events.push(['scannerReadiness', environment]); return { enabled: !input.scannerDisabled, ready: !input.scannerUnavailable }; },
    async attachmentPreviewReadiness(environment) { events.push(['previewReadiness', environment]); return { ready: !input.previewUnavailable }; },
    detectedAttachmentMediaType(bytes, declared) { events.push(['detect', { size: bytes.length, declared }]); return input.mediaMismatch ? null : declared; },
    async scanAttachmentBytes(bytes, environment) { events.push(['scan', { size: bytes.length, environment }]); if (input.scanFailure) throw new Error(input.scanFailure); return { clean: !input.infected, signature: input.infected ? 'Owned.Test.Signature' : null, engine: 'ClamAV captured', definitionUpdatedAt: now - 1000 }; },
    async sanitizedAttachmentPreview(bytes, media) { events.push(['preview', { size: bytes.length, media }]); return input.preview ? { bytes: new Uint8Array([1, 3, 5, 7]), mediaType: input.preview } : null; },
  };
  const effectNames = new Set(['stepAttachmentScanAttempts109', 'stepAttachments110', 'stepAttachmentEvents111', 'stepAttachmentScanAttempts112', 'stepAttachments113', 'stepUploadIntents114', 'stepAttachmentLinks115', 'stepAttachmentEvents116', 'stepAttachmentStorageLedger117', 'stepAttachmentScanAttempts118', 'stepAttachments119', 'stepAttachmentEvents120', 'stepAttachmentScanAttempts121']);
  const db = new Proxy({
    async readAttachmentScanAttempts106(facts) { events.push(['claim', facts]); return input.noJob ? null : { id: 'scan-owned', attachment_id: source.id, attempt: input.attempt ?? 1 }; },
    async readAttachments107(facts) { events.push(['source', facts]); return input.sourceMissing ? null : source; },
    async writeAttachments108(facts) { events.push(['markScanning', facts]); },
    async batch(effects) { events.push(['atomicEffects', effects]); return []; },
  }, { get(target, name) { if (name in target) return target[name]; if (effectNames.has(name)) return facts => [name, facts]; throw new Error(`UNDECLARED_ORACLE_DB_PORT:${String(name)}`); } });
  const files = {
    async get(key) { events.push(['getOriginal', key]); return input.objectMissing ? null : { async arrayBuffer() { const bytes = input.oversize ? Buffer.alloc(10 * 1024 * 1024 + 1) : payload; return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength); } }; },
    async put(key, bytes, options) { events.push(['putImmutable', { key, bytes: normalize(bytes), options }]); return null; },
  };
  const context = createContext({ ports, ...providers, Promise, Uint8Array, attachmentsV2Enabled: () => input.enabled !== false, attachmentUploadsFrozen: () => !!input.uploadsFrozen, ATTACHMENT_DRAFT_MS: 7 * 24 * 60 * 60_000 });
  new Script(transpile(functions) + '\nthis.drain = drainAttachmentScans;').runInContext(context, { timeout: 1000 });
  let result;
  try { result = { value: await context.drain(db, files, input.filter, { offlineMigration: !!input.offlineMigration }) }; }
  catch (error) { result = { error: error.message }; }
  cases.push({ name, input, result, events: normalize(events) });
}
for (const [name, input] of [
  ['no-job-readiness-before-claim', { noJob: true }],
  ['source-missing-is-not-settled', { sourceMissing: true }],
  ['clean-without-preview', {}],
  ['clean-webp-content-addressed-preview', { preview: 'image/webp' }],
  ['clean-jpeg-key-without-original-suffix', { preview: 'image/jpeg', objectKey: 'legacy/storage/owned' }],
  ['infected-no-preview', { infected: true, preview: 'image/webp' }],
  ['missing-original', { objectMissing: true }],
  ['oversize-before-hash', { oversize: true }],
  ['checksum-mismatch-before-media', { source: { checksum_sha256: '0'.repeat(64) } }],
  ['media-mismatch-before-scanner', { mediaMismatch: true }],
  ['error-code-truncated-to-160-utf16-units', { scanFailure: 'x'.repeat(200) }],
  ['expired-upload-intent-is-not-loaded', { filter: 'attachment-owned' }],
  ['ordinary-worker-forces-v2-scanner-environment', { enabled: false }],
  ['offline-migration-requires-freeze', { offlineMigration: true }],
  ['offline-migration-with-freeze', { offlineMigration: true, uploadsFrozen: true }],
  ['scanner-unavailable-before-claim', { scannerUnavailable: true }],
  ['scanner-disabled-before-claim', { scannerDisabled: true }],
  ['preview-unavailable-before-claim', { previewUnavailable: true }],
  ...Array.from({ length: 12 }, (_unused, i) => [`error-retry-attempt-${i + 1}`, { attempt: i + 1, scanFailure: 'OWNED_SCANNER_ERROR' }]),
]) await capture(name, input);

// Exact policy aliases are an independent B0 function, not Rust expectations.
const legacyContext = createContext({});
const legacyBody = legacyOriginal.slice(legacyOriginal.indexOf('export function legacyAttachmentPolicy(')).replace(/^export /, '');
new Script(transpile(legacyBody) + '\nthis.policy = legacyAttachmentPolicy;').runInContext(legacyContext, { timeout: 1000 });
const owners = ['support_ticket','employee_request','leave_request','expense_claim','expense_payment','invoice_payment','staff_task','social_draft','quality_hold','commerce_order_delivery','holding_brand','branch_brand','company_storefront_brand','unknown'];
const purposes = ['support_evidence','internal_evidence','customer_shareable','request_evidence','it_request_evidence','fulfillment','medical_certificate','leave_supporting_photo','evidence','receipt','merchant_receipt','supporting_document','payment_slip','bank_payment_slip','task_evidence','post_image','post-image','quality_inspection_evidence','proof_of_delivery','primary','inverse','logo','unknown'];
const legacy = owners.flatMap(owner => purposes.map(purpose => ({ owner, purpose, result: normalize(legacyContext.policy(owner, purpose)) })));
const receipt = { baseline, source: { path: sourcePath, sha256: hash(original) }, legacySource: { path: legacyPath, sha256: hash(legacyOriginal) }, oracle: { runtime: process.version, typescript: ts.version, generatorSha256: hash(await readFile(import.meta.url.startsWith('file:') ? new URL(import.meta.url) : import.meta.url)), disclaimer: 'Effect-intent/source algorithm evidence only; no SQL/lease safety, decoder or authority acceptance is implied.' }, cases, legacy };
await writeFile(output, `${JSON.stringify(receipt, null, 2)}\n`, { mode: 0o600 });
process.stdout.write(JSON.stringify({ cases: cases.length, legacy: legacy.length, source: receipt.source.sha256, legacySource: receipt.legacySource.sha256 }) + '\n');
