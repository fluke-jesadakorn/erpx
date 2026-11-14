import {existsSync} from 'node:fs';
import {isolatedVerificationRuntime} from './ci-postgres-contract';
import { createHash } from 'node:crypto';
import {execFileSync} from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import sharp from 'sharp';
import { attachmentScannerReadiness, scanAttachmentBytes } from '@erpx/server-files/public/attachments/scanner';
import { sanitizedAttachmentPreview } from '@erpx/server-files/public/attachments/preview';
import {ciHeifDecoderInventory} from './ci-attachment-contract';

if (!isolatedVerificationRuntime(process.env.CI, process.env.GITHUB_ACTIONS, process.env.ERPX_LOCAL_VERIFICATION, process.platform, existsSync('/.dockerenv')) || process.platform!=='linux') throw new Error('ISOLATED_GITHUB_ATTACHMENT_PREREQUISITES_REQUIRED');
const environment = { ...process.env, ERPX_ATTACHMENTS_V2: 'true' };
const readiness = await attachmentScannerReadiness(environment);
if (!readiness.ready || !readiness.engine?.startsWith(`ClamAV ${process.env.ERPX_CLAMAV_VERSION}/`))
  throw new Error(`CI_ACTUAL_SCANNER_NOT_READY:${readiness.error ?? 'CLAMAV_ENGINE_VERSION_MISMATCH'}`);
const clean = await scanAttachmentBytes(new TextEncoder().encode('clean ERPX CI prerequisite'), environment);
const eicar = ['X5O!P%@AP[4\\PZX54(P^)', '7CC)7}$EICAR-STANDARD-', 'ANTIVIRUS-TEST-FILE!$H+H*'].join('');
const detected = await scanAttachmentBytes(new TextEncoder().encode(eicar), environment);
if (!clean.clean || detected.clean || !/Eicar/i.test(detected.signature ?? '')) throw new Error('CI_ACTUAL_SCANNER_PROBE_FAILED');
// Ubuntu packages HEVC support separately from the converter CLI. Merely
// installing libheif-examples does not establish an available HEIC decoder.
const decoderPackages=ciHeifDecoderInventory(execFileSync('/usr/bin/dpkg-query',[
  '-W','-f','${Package}\t${Version}\n','libheif1','libheif-examples','libheif-plugin-libde265','libde265-0',
],{encoding:'utf8',timeout:5_000,maxBuffer:16*1024}));
const source = await readFile(resolve('fixtures/files/attachment-formats/rotated-non-square.heic'));
const manifest = JSON.parse(await readFile(resolve('fixtures/files/attachment-formats/manifest.json'), 'utf8'));
const sha256 = createHash('sha256').update(source).digest('hex');
if (source.length !== manifest.files['rotated-non-square.heic'].bytes || sha256 !== manifest.files['rotated-non-square.heic'].sha256)
  throw new Error('CI_OWNED_HEIC_FIXTURE_CHANGED');
const preview = await sanitizedAttachmentPreview(source, 'image/heic');
if (!preview) throw new Error('CI_REAL_HEIC_DECODER_REQUIRED');
const metadata = await sharp(preview.bytes).metadata();
if (metadata.width !== 40 || metadata.height !== 80 || metadata.exif || metadata.xmp || metadata.iptc)
  throw new Error('CI_REAL_HEIC_PREVIEW_PROBE_FAILED');
const directory = resolve('.scratch/dev/test-suites'); await mkdir(directory, { recursive: true, mode: 0o700 });
await writeFile(resolve(directory, 'ci-prerequisites.json'), JSON.stringify({ version: 1,
  scanner: { engine: readiness.engine, definitionUpdatedAt: readiness.definitionUpdatedAt, clean: true, eicar: true },
  heic: { sourceSha256: sha256, width: metadata.width, height: metadata.height, metadataStripped: true, decoderPackages },
  scope: 'Operational prerequisites only; not application or production acceptance' }, null, 2) + '\n', { mode: 0o600 });
console.log('CI_REAL_ATTACHMENT_PREREQUISITES_VERIFIED');
