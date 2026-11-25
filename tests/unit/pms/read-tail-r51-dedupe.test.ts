import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('../../../packages/server-pms/src/adapters/metric-purchases', () => ({
  requirePmsMetricPurchase: vi.fn(async () => {}),
  pmsMetricHistoryAccess: vi.fn(),
}));
import { readExecutiveMetricProjection } from '../../../packages/server-pms/src/adapters/executive-metric-projections';
import { pmsExecutiveMetricLabelHeadSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r51-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// The byte-identical label head retired at HEAD (KPI 254). All three folded
// sites opened with these exact bytes; the renderer must reproduce them verbatim.
const HEAD = 'SELECT ${label} label,';
const HEAD_SHA = '407b48e24b8744ce0bed0fce0d3daba08334ce5ffe7300600503074cf3a9ded0';

// Three byte oracles: full rendered SQL for each folded site captured at HEAD
// (KPI 254). SHA, length, SELECT count and bind order all survive the fold.
const ORACLES = {
  occupancy: {
    sha: '240c4de5d1ba09cdf34fe63f358aa9f08ff1a6a125146d3576dc3fb9b4acf01a',
    len: 1806, selects: 1,
    binds: ['company-1', 'branch-1', 'branch-1', '2026-01-01', '2026-01-31'],
  },
  adr: {
    sha: '704f4b92e3300c309ac73710b0f0c8080f46e7aeb412ffad8d4264a1daf681c7',
    len: 3880, selects: 10,
    binds: ['company-1', 'branch-1', 'branch-1', 'THB', '2026-01-01', '2026-01-31', '2026-01-01', '2026-01-31'],
  },
  guestAccountBalance: {
    sha: 'de882884a4b8819614309507cafa715b7fd3f3f9120bca82282a70c59cbc0171',
    len: 1470, selects: 5,
    binds: ['company-1', 'branch-1', 'branch-1', 'THB'],
  },
} as const;
const CASES = {
  occupancy: ['hotel-occupancy', 'month'],
  adr: ['hotel-adr', 'branch'],
  guestAccountBalance: ['hotel-guest-account-balance', 'branch'],
} as const;
const KINDS = Object.keys(ORACLES) as (keyof typeof ORACLES)[];

const SHARED = 'adapters/pms-read-tail-r51-sql.ts';
const SITE = 'adapters/executive-metric-projections.ts';
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

async function rendered(id: string, breakdown: string) {
  const sqls: string[] = [], binds: unknown[][] = [];
  const db = {
    prepare(sql: string) {
      sqls.push(sql);
      return {
        bind(...values: unknown[]) { binds.push(values); return this; },
        async all<T>() { return { results: [] as T[] }; },
      };
    },
  };
  const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'THB' };
  await readExecutiveMetricProjection(db as never,
    { companyId: 'company-1', branchId: 'branch-1' } as never,
    id as never, filters as never, breakdown as never);
  return { sql: sqls[0]!, binds: binds[0]! };
}

describe('pms read-tail r51 executive metric label head dedupe', () => {
  it('pins the exact HEAD bytes of the shared label head', () => {
    expect(sha256(HEAD)).toBe(HEAD_SHA);
    expect(pmsExecutiveMetricLabelHeadSql('x')).toBe(HEAD.replace('${label}', 'x'));
  });

  it('declares one owner-local renderer with no caller SQL or write seam', async () => {
    const shared = await src(SHARED);
    expect(names(shared, 'pmsExecutiveMetricLabelHeadSql')).toBe(1);
    expect(shared.split(HEAD).length - 1, 'renderer adds one head').toBe(1);
    expect(shared.split('SELECT ').length - 1, 'renderer adds one SELECT token').toBe(1);
    expect(shared).toContain('(label: string): string =>');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes the head byte-identically with zero head binds', () => {
    for (const label of ["'Hotel occupancy'", 'p.name', 'substring(n.business_date,1,7)']) {
      expect(pmsExecutiveMetricLabelHeadSql(label), label).toBe(`SELECT ${label} label,`);
      expect((pmsExecutiveMetricLabelHeadSql(label).match(/\?/g) ?? []).length, label).toBe(0);
      expect(pmsExecutiveMetricLabelHeadSql(label).split('SELECT ').length - 1, label).toBe(1);
    }
  });

  it('renders byte-identical projections for all three folded sites', async () => {
    for (const k of KINDS) {
      const [id, breakdown] = CASES[k];
      const { sql, binds } = await rendered(id, breakdown);
      expect(sha256(sql), `${k}.sha`).toBe(ORACLES[k].sha);
      expect(sql.length, `${k}.len`).toBe(ORACLES[k].len);
      expect(sql.split('SELECT ').length - 1, `${k}.selects`).toBe(ORACLES[k].selects);
      expect(binds, `${k}.binds`).toEqual(ORACLES[k].binds);
    }
  });

  it('routes all three heads through the helper and retires the inline heads', async () => {
    const site = await src(SITE);
    expect(calls(site, 'pmsExecutiveMetricLabelHeadSql')).toBe(3);
    for (const n of [32, 80, 93]) expect(site).toContain('${pmsExecutiveMetricLabelHeadSql(label)}');
    expect(site.split(HEAD).length - 1, 'only the distinct blockers head stays inline').toBe(1);
    expect(site).toContain('sql = `SELECT ${label} label,count(*)::text value');
    // Tails, label expressions and bind order stay byte-identical at every site.
    expect(site).toContain('COALESCE(sum(CASE WHEN ${valid} THEN ${sold}::numeric ELSE 0::numeric END),0)::text numerator,');
    expect(site).toContain('COALESCE(sum(l.debit_minor::numeric-l.credit_minor::numeric),0)::text value');
    expect(site).toContain('COALESCE(sum(CASE WHEN n.valid_metrics THEN COALESCE(rr.revenue_minor,0::numeric) ELSE 0::numeric END),0)::text numerator,');
    expect(site).toContain('const label = breakdown === "month" ? "substring(r.data_json::jsonb->>\'date\',1,7)"');
    expect(site).toContain("breakdown === \"branch\" ? pmsBranchLabelSql('n2')");
    expect(site).toContain("? EXEC_BRANCH_LABEL_SQL\n            : breakdown === \"property\" ? \"p.name\" : \"'Open guest account balances'\"");
    expect(site).toContain('values = [a.companyId, a.branchId, a.branchId, f.from, f.to]');
    expect(site).toContain('values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to, f.from, f.to]');
    expect(site).toContain('values = [a.companyId, a.branchId, a.branchId, f.currency]');
    expect(site).toContain('.bind(...values).all<RawMetricRow>()');
  });

  it('leaves native registry, r47-r50 helpers, locks and prior helpers untouched', async () => {
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(58);
    expect(rust).toContain('pub const PMS_RD_SECTION_RELATED_RECORDS: &str = "pms_rd_section_related_records";');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(58);
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 49, 50]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
    for (const n of [49, 50]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).split(HEAD).length - 1, `r${n}`).toBe(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 254 to 252', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(215);
    expect(bare).toBe(216);
    expect(spaced).toBeLessThan(252);
    expect(bare).toBeLessThan(254);
  });
});
