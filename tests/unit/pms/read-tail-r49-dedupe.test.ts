import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { pmsAddonPackageSettlementSources } from '../../../packages/server-pms/src/adapters/addon-package-obligations';
import { pmsSettlementSourceKeySql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r49-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles captured at HEAD (KPI 258) before the r49 dedupe. Every closed
// union recomposition and every rendered settlement source must match verbatim.
const HEAD = {
  charge: "SELECT e.package_id,e.company_id,e.property_id,r.id||':charge' source_key",
  reverse: "SELECT e.package_id,e.company_id,e.property_id,r.id||':reverse'",
  settlementReceipt: "SELECT e.package_id,e.company_id,e.property_id,r.id||CASE WHEN r.data->>'description'='Receive OTA / company remittance' THEN ':settlement' ELSE ':receipt' END",
  refund: "SELECT e.package_id,e.company_id,e.property_id,r.id||':refund'",
} as const;
const HEAD_SHA = {
  charge: '3c76b9337e09254d0ffe2915b20c1c54cad79ef3271196dd0e8d111ed820c58e',
  reverse: '014d46886543d1f761f1a4080a6faa03e6e98dcbc5016e80008f6cacdf940e6d',
  settlementReceipt: '3046fb710277c1c4c5ef2570c0c6e536457ea212068bd2d0225d0ab357e67f53',
  refund: '89022770966a886f100a497a6c06b60306c497aac857267d972d185807c03515',
} as const;
const KINDS = Object.keys(HEAD) as (keyof typeof HEAD)[];
const GENERATED = {
  charge: pmsSettlementSourceKeySql('charge'),
  reverse: pmsSettlementSourceKeySql('reverse'),
  settlementReceipt: pmsSettlementSourceKeySql('settlementReceipt'),
  refund: pmsSettlementSourceKeySql('refund'),
} as const;

// Rendered source SHA/length/SELECT-count per owner site at HEAD (KPI 258).
const RENDERED = {
  'pos-hotel-charge': {
    accounts: ['00e9b8413ac71016a3e54e8cb8d6a6040b2405191c2a73c36151916de5e43e41', 3425, 17],
    postings: ['b8dadaefff7e18d4e0bdc168489a51f329f10dae85a2c85ffdfbd873427ff8d6', 3780, 19],
    requiredPostings: ['430ef11ce916b125598df8c9d69b58564a997a650a5fc026a916efe78460eeb3', 4598, 21],
    payments: ['da8e4c9f1bf89ef50fc28306052ab7a9e097ce003c9156c05a3458d76ea3f590', 5486, 19],
    stockMoves: ['f59aa289ae401b69ce57ca32b152c8a9ee95e0897945fadda48371af5bab8675', 3657, 17],
  },
  'pms-minibar-linen': {
    accounts: ['ae42e0c60e170f5630d36b4b9a3c445ad8d1c30cbfc6e39fbec1852265549636', 2711, 11],
    postings: ['3912b698c64bc16e75cc4c04b9fe4bdd041f7b82cbc9171d60922b02235cf36d', 3066, 13],
    requiredPostings: ['4052d8104001a8821c60fb47b81938ef1bc675db30b321a511679fb95a2f9f15', 3884, 15],
    payments: ['1319e098759b5fde0c9d4212bd308efe2d6ab2453eaadba15a797a9a0891c10b', 4772, 13],
    stockMoves: ['29e99d3bafcd278457b361ff5366a0c69842b45e58206c7d3e77b0b78f9f53a2', 2943, 11],
  },
} as const;
const PACKAGES = Object.keys(RENDERED) as (keyof typeof RENDERED)[];

const SHARED = 'adapters/pms-read-tail-r49-sql.ts';
const SITE = 'adapters/addon-package-obligations.ts';
async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
const calls = (text: string, name: string) => text.split(`${name}(`).length - 1;
const names = (text: string, name: string) => text.split(name).length - 1;

describe('pms read-tail r49 settlement source head dedupe', () => {
  it('pins the exact HEAD bytes of all four retired heads', () => {
    for (const k of KINDS) expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
  });

  it('declares one closed union in the owner-local module with no caller SQL', async () => {
    const shared = await src(SHARED);
    expect(names(shared, 'pmsSettlementSourceKeySql')).toBe(1);
    expect(shared).toContain('type SourceKey =');
    expect(shared).toContain('const SOURCE_KEY:');
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
    expect(shared.split('SELECT ').length - 1, 'renderer adds one SELECT token').toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes all four heads byte-identically with binds untouched', () => {
    for (const k of KINDS) {
      expect(GENERATED[k], k).toBe(HEAD[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length, k).toBe((HEAD[k].match(/\?/g) ?? []).length);
    }
  });

  it('renders byte-identical settlement sources for both package owners', () => {
    for (const pid of PACKAGES) {
      const all = pmsAddonPackageSettlementSources(pid);
      for (const k of ['accounts', 'postings', 'requiredPostings', 'payments', 'stockMoves'] as const) {
        const [headSha, headLen, headSelects] = RENDERED[pid][k];
        const sql = all[k];
        expect(sha256(sql), `${pid}.${k}`).toBe(headSha);
        expect(sql.length, `${pid}.${k}`).toBe(headLen);
        expect(sql.split('SELECT ').length - 1, `${pid}.${k}`).toBe(headSelects);
      }
    }
  });

  it('routes all four heads through the helper and retires the inline heads', async () => {
    const site = await src(SITE);
    expect(calls(site, 'pmsSettlementSourceKeySql')).toBe(4);
    for (const k of KINDS) {
      expect(site).toContain(`pmsSettlementSourceKeySql('${k}')`);
      expect(site.split(HEAD[k]).length - 1, k).toBe(0);
    }
    // The sibling projections and the lock-free refund witness keep their SQL.
    expect(site).toContain("SELECT a.package_id,a.company_id,a.property_id,a.id,a.track_due,r.version,r.data");
    expect(site).toContain('SELECT m.package_id,m.company_id,m.property_id,m.id,m.version,m.warehouse_id');
  });

  it('leaves native registry, r47/r48 pins, locks and prior helpers untouched', async () => {
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(49);
    expect(rust).toContain('pub const PMS_RD_SECTION_RELATED_RECORDS: &str = "pms_rd_section_related_records";');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(49);
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 258 to 255', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(222);
    expect(bare).toBe(223);
    expect(spaced).toBeLessThan(258);
    expect(bare).toBeLessThan(260);
  });
});
