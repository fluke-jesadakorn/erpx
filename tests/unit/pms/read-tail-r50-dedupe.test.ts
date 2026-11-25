import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { pmsAddonPackageSettlementSources } from '../../../packages/server-pms/src/adapters/addon-package-obligations';
import { pmsSettledProjectionHeadSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r50-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// The byte-identical `payments` CTE head retired at HEAD (KPI 255). Both the
// `expected` materialized CTE and the outer settled-receipts projection opened
// with these exact bytes; the renderer must reproduce them verbatim.
const HEAD = 'SELECT e.package_id,e.company_id,e.property_id,';
const HEAD_SHA = {
  expected: '5db767503182f32bec5ea6f4cd01b9243d4f9f252caf66b1bd6518d6e8359cca',
  receipts: '5db767503182f32bec5ea6f4cd01b9243d4f9f252caf66b1bd6518d6e8359cca',
} as const;
const KINDS = Object.keys(HEAD_SHA) as (keyof typeof HEAD_SHA)[];
const GENERATED = {
  expected: pmsSettledProjectionHeadSql('expected'),
  receipts: pmsSettledProjectionHeadSql('receipts'),
} as const;

// Rendered source SHA/length/SELECT-count per owner site at HEAD (KPI 255).
// The dedupe must not change a single rendered byte, bind or tail.
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

const SHARED = 'adapters/pms-read-tail-r50-sql.ts';
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

describe('pms read-tail r50 settled projection head dedupe', () => {
  it('pins the exact HEAD bytes of the shared payments CTE head', () => {
    expect(sha256(HEAD)).toBe(HEAD_SHA.expected);
    expect(HEAD_SHA.receipts).toBe(HEAD_SHA.expected);
    for (const k of KINDS) expect(sha256(GENERATED[k]), k).toBe(HEAD_SHA[k]);
  });

  it('declares one closed union in the owner-local module with no caller SQL', async () => {
    const shared = await src(SHARED);
    expect(names(shared, 'pmsSettledProjectionHeadSql')).toBe(1);
    expect(shared).toContain("type Head = 'expected' | 'receipts'");
    expect(shared).toContain('const KINDS:');
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
    expect(shared.split('SELECT ').length - 1, 'renderer adds one SELECT token').toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes both heads byte-identically with zero head binds', () => {
    for (const k of KINDS) {
      expect(GENERATED[k], k).toBe(HEAD);
      expect((GENERATED[k].match(/\?/g) ?? []).length, k).toBe(0);
      expect(GENERATED[k].split('SELECT ').length - 1, k).toBe(1);
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

  it('routes both heads through the helper and retires the inline heads', async () => {
    const site = await src(SITE);
    expect(calls(site, 'pmsSettledProjectionHeadSql')).toBe(2);
    for (const k of KINDS) expect(site).toContain(`pmsSettledProjectionHeadSql('${k}')`);
    expect(site.split(HEAD).length - 1, 'no raw inline head remains').toBe(0);
    // Both full projections and their tails stay in place.
    expect(site).toContain("expected AS MATERIALIZED (${pmsSettledProjectionHeadSql('expected')}r.id entry_id,r.version,r.data,");
    expect(site).toContain("${pmsSettledProjectionHeadSql('receipts')}e.entry_id,e.version,e.expected_kind,");
    expect(site).toContain('FROM financial_entries e JOIN records r ON r.id=e.id AND r.property_id=e.property_id JOIN pms_properties p');
    expect(site).toContain('FROM expected e LEFT JOIN pms_documents d');
    // The outer bind order is unchanged.
    expect(site).toContain('.bind(input.companyId,input.packageId).all()');
    // Sibling projections keep their SQL.
    expect(site).toContain('SELECT a.package_id,a.company_id,a.property_id,a.id,a.track_due,r.version,r.data');
    expect(site).toContain('SELECT m.package_id,m.company_id,m.property_id,m.id,m.version,m.warehouse_id');
  });

  it('leaves native registry, r47/r48 pins, locks and prior helpers untouched', async () => {
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(33);
    expect(rust).toContain('pub const PMS_RD_SECTION_RELATED_RECORDS: &str = "pms_rd_section_related_records";');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(33);
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 49]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 255 to 254', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(243);
    expect(bare).toBe(245);
    expect(spaced).toBeLessThan(255);
    expect(bare).toBeLessThan(257);
  });
});
