import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import sharp from 'sharp';

const execute = promisify(execFile), directory = resolve('fixtures/files/attachment-formats');
const sha256 = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');
const sourceHash = sha256(await readFile(import.meta.filename));
if (process.argv.length !== 3 || !['--write', '--check'].includes(process.argv[2]))
  throw new Error('Choose --write to regenerate with the recorded encoder, or --check for offline checksums.');
if (process.argv[2] === '--write') {
  const encoder = process.env.ERPX_HEIF_ENC ?? '/opt/homebrew/opt/libheif/bin/heif-enc';
  const version = (await execute(encoder, ['--version'], { timeout: 5_000, maxBuffer: 4_096 })).stdout.trim();
  if (version.split('\n')[0] !== '1.23.6') throw new Error('RECORDED_HEIF_ENCODER_1_23_6_REQUIRED');
  const temporary = await mkdtemp(join(tmpdir(), 'erpx-owned-heic-'));
  try {
    const png = join(temporary, 'source.png'), heic = join(temporary, 'rotated-non-square.heic');
    await sharp({ create: { width: 80, height: 40, channels: 3, background: { r: 27, g: 115, b: 212 } } })
      .withMetadata({ orientation: 1, exif: { IFD0: { ImageDescription: 'private metadata' } } }).png().toFile(png);
    await execute(encoder, ['--rotate-cw', '90', '-q', '70', '-o', heic, png], { timeout: 20_000, maxBuffer: 1_048_576 });
    await mkdir(directory, { recursive: true });
    const files: Record<string, { bytes: number; sha256: string }> = {};
    for (const [name, path] of [['source.png', png], ['rotated-non-square.heic', heic]]) {
      const bytes = await readFile(path);
      files[name] = { bytes: bytes.length, sha256: sha256(bytes) };
      await writeFile(join(directory, name), bytes);
    }
    await writeFile(join(directory, 'manifest.json'), JSON.stringify({ version: 1,
      provenance: 'ERPX-authored solid-color raster; no external image or personal data',
      generatorSha256: sourceHash, encoderVersion: version, sourceWidth: 80, sourceHeight: 40,
      clockwiseRotation: 90, decodedWidth: 40, decodedHeight: 80, description: 'private metadata', files }, null, 2) + '\n');
  } finally { await rm(temporary, { recursive: true, force: true }); }
} else {
  const manifest = JSON.parse(await readFile(join(directory, 'manifest.json'), 'utf8'));
  if (manifest.version !== 1 || manifest.generatorSha256 !== sourceHash
    || Object.keys(manifest.files).sort().join(',') !== 'rotated-non-square.heic,source.png') throw new Error('HEIC_FIXTURE_SOURCE_CHANGED');
  for (const name of ['source.png', 'rotated-non-square.heic']) {
    const bytes = await readFile(join(directory, name));
    if (bytes.length !== manifest.files[name].bytes || sha256(bytes) !== manifest.files[name].sha256)
      throw new Error('HEIC_FIXTURE_BYTES_CHANGED');
  }
}
console.log(`OWNED_HEIC_FIXTURE_${process.argv[2] === '--check' ? 'VERIFIED' : 'GENERATED'}`);
