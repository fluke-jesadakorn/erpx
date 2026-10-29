// Independent Node26 core-crypto witness for the proposed byte protocol.
// Synthetic material only; never read a runtime profile, key or environment.
import { createHmac } from 'node:crypto';
import { writeFileSync } from 'node:fs';
const key = Buffer.alloc(32, 0x31);
const fields: Buffer[] = [];
function encodedLength(length: number) {
  const value = Buffer.alloc(8);
  value.writeBigUInt64BE(BigInt(length));
  return value;
}
function field(name: string, value: string | Buffer) {
  const nameBytes = Buffer.from(name);
  const valueBytes = Buffer.isBuffer(value) ? value : Buffer.from(value);
  fields.push(encodedLength(nameBytes.length), nameBytes, encodedLength(valueBytes.length), valueBytes);
}
function optional(name: string, value: string | null) {
  field(name, value === null ? 'none' : 'some');
  if (value !== null) field('value', value);
}
field('purpose', 'erpx/directory/selected-profile/v1');
field('key-id', 'synthetic_key_a');
field('environment', 'local');
field('company', 'company');
field('connection', 'connection');
field('profile', 'fixture_a');
optional('holding', 'h001');
field('hosts-count', encodedLength(1));
field('host', 'directory.fixture.invalid');
optional('bind-dn', 'cn=synthetic-reader');
optional('bind-password', 'synthetic-password');
optional('ca-pem', null);
optional('tls-server-name', 'directory.fixture.invalid');
field('aliases-count', encodedLength(1));
field('alias-from', 'new-id');
field('alias-to', 'old-id');
const bytes = Buffer.concat(fields);
const result = {
  runtime: process.version,
  purpose: 'erpx/directory/selected-profile/v1',
  material: 'explicit synthetic literals only',
  messageByteLength: bytes.length,
  hmacSha256: createHmac('sha256', key).update(bytes).digest('hex'),
};
writeFileSync(new URL('../../../fixtures/identity_access/directory/provenance/source-tag-v1-oracle.json', import.meta.url), `${JSON.stringify(result, null, 2)}\n`);
console.log(`Captured one synthetic source-tag vector on ${process.version}.`);
