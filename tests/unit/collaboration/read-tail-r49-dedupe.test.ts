import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialAiMonthlyUsageCount, socialAiUsageProjection, socialAiUsageRead } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');

// Exact pre-r49 rendered AI-usage reads at BEFORE=334, pinned by SHA so the byte
// identity of both retired helper heads cannot drift silently. The r49 merge only
// factors the shared `SELECT ${cols} FROM social_ai_usage` head into one renderer;
// projected columns, the company/month bind order, callers and locks stay fixed.
const HEAD = 'SELECT ${cols} FROM social_ai_usage';
const MONTH = ' WHERE company_id=? AND month=?';
const HOSPITALITY_COLS = '"id","company_id","actor_id","month","state","input_tokens","output_tokens","lease_until","created_at"';

type Site = {
  name: string;
  cols: string;
  kind: 'projection' | 'count';
  sql: string;
  sqlSha: string;
  bytes: number;
  binds: number;
};
const SITES: Site[] = [
  {
    name: 'agent usage row projection',
    cols: 'company_id,month',
    kind: 'projection',
    sql: 'SELECT company_id,month FROM social_ai_usage',
    sqlSha: 'b883d3dfc75ae28e4847db2f54aa48b2605b1cea25dd374a4b58da4cd822abaa',
    bytes: 44,
    binds: 0,
  },
  {
    name: 'hospitality owner usage row projection',
    cols: HOSPITALITY_COLS,
    kind: 'projection',
    sql: `SELECT ${HOSPITALITY_COLS} FROM social_ai_usage`,
    sqlSha: '9934f8001b5644f83bd16f1b1221d90ed984372f0780db6ae8c5df54f51e6024',
    bytes: 130,
    binds: 0,
  },
  {
    name: 'agent monthly usage count',
    cols: 'count(*)',
    kind: 'count',
    sql: `SELECT count(*) FROM social_ai_usage${MONTH}`,
    sqlSha: '35955989b9758a07160b3584016404cf55a29310a0f41e07182ecec11165017a',
    bytes: 67,
    binds: 2,
  },
  {
    name: 'records monthly usage count',
    cols: 'COUNT(*)',
    kind: 'count',
    sql: `SELECT COUNT(*) FROM social_ai_usage${MONTH}`,
    sqlSha: '0bf6312e9c074ce0995954dbb001c22c1b47cacd42dd2674f8422e6f39be10bd',
    bytes: 67,
    binds: 2,
  },
  {
    name: 'beta monthly token usage count',
    cols: 'input_tokens,output_tokens',
    kind: 'count',
    sql: `SELECT input_tokens,output_tokens FROM social_ai_usage${MONTH}`,
    sqlSha: 'fc3b10920fd4eeda795250d40dd85dc198225ab517fc0e3683bbe837b4715937',
    bytes: 85,
    binds: 2,
  },
];

const viaWrapper = (site: Site) => site.kind === 'count' ? socialAiMonthlyUsageCount(site.cols) : socialAiUsageProjection(site.cols);
const viaShared = (site: Site) => site.kind === 'count' ? socialAiUsageRead(site.cols, MONTH) : socialAiUsageRead(site.cols);

describe('Collaboration read-tail r49 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired head renders byte for byte`, () => {
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(sha(viaWrapper(site))).toBe(site.sqlSha);
      expect(sha(viaShared(site))).toBe(site.sqlSha);
      expect(viaWrapper(site)).toBe(site.sql);
      expect(viaShared(site)).toBe(site.sql);
      expect(site.sql.startsWith(`SELECT ${site.cols} FROM social_ai_usage`)).toBe(true);
      expect(site.sql.endsWith(site.kind === 'count' ? MONTH : ' FROM social_ai_usage')).toBe(true);
      expect(site.sql.match(/SELECT /g) ?? []).toHaveLength(1);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect(site.sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    });
  }

  test('the two retired helper bodies shared an exact 35-byte head', () => {
    const beforeProjection = 'SELECT ${cols} FROM social_ai_usage';
    const beforeCount = 'SELECT ${cols} FROM social_ai_usage' + MONTH;
    expect(Buffer.byteLength(HEAD)).toBe(35);
    expect(sha(HEAD)).toBe('2aa21f85eb0e0237f214ce34c93bc758daec3be85963ea7ec35ae60fc54731ee');
    expect(beforeProjection.slice(0, 35)).toBe(HEAD);
    expect(beforeCount.slice(0, 35)).toBe(HEAD);
    expect(sha(beforeProjection.slice(0, 35))).toBe(sha(beforeCount.slice(0, 35)));
    expect(beforeCount.slice(35)).toBe(MONTH);
  });
});

describe('Collaboration read-tail r49 AI-usage read family', () => {
  test('one renderer plus two retained thin wrappers replace the two heads', () => {
    expect(shared.match(/export const socialAiUsageRead=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialAiUsageProjection=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialAiMonthlyUsageCount=/g) ?? []).toHaveLength(1);
    expect(shared).toContain("export const socialAiUsageRead=(cols:string,tail='')=>`SELECT ${cols} FROM social_ai_usage${tail}`;");
    expect(sha(line(shared, 'export const socialAiUsageRead='))).toBe('cc14834e954b46ffa6f07a81d97ccf47964eba4e7aea19a00ce829f540c0ec8b');
    expect(sha(line(shared, 'export const socialAiUsageProjection='))).toBe('871b54166f9ae07c4d255f7b746aa2509aaaaf151da98d07d6de1a9ab716726d');
    expect(sha(line(shared, 'export const socialAiMonthlyUsageCount='))).toBe('26b29fdb29e6f7f6ec0e208c817c0ec7ba9fdcb8d01863d6592311344f96f932');
    for (const marker of ['export const socialAiUsageProjection=', 'export const socialAiMonthlyUsageCount=']) {
      const wrapper = line(shared, marker);
      expect(wrapper).not.toContain('SELECT ');
      expect(wrapper).toContain('socialAiUsageRead(');
    }
    expect(line(shared, 'export const socialAiUsageProjection=')).toContain('socialAiUsageRead(cols)');
    expect(line(shared, 'export const socialAiMonthlyUsageCount=')).toContain("socialAiUsageRead(cols,' WHERE company_id=? AND month=?')");
    expect((shared.match(/SELECT \$\{cols\} FROM social_ai_usage/g) ?? []).length).toBe(1);
  });

  test('the new renderer adds no lock, write or notification statement', () => {
    const added = line(shared, 'export const socialAiUsageRead=');
    expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    expect(added.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  test('consumers keep their original imports, wrappers and bind arrays', () => {
    const agentUsage = src('agent-ai-usage-projection.ts');
    const records = src('records-ai-usage-projection.ts');
    const finance = src('finance-ai-quota-sources.ts');
    const hospitality = src('hospitality-owner-projection-sources.ts');
    const beta = src('beta-management-sources.ts');
    const budget = src('social-ai-budget-projections.ts');
    expect(agentUsage).toContain("import {socialAiMonthlyUsageCount,socialAiUsageProjection} from './read-tail-sql';");
    expect(agentUsage).toContain("export const socialAiUsageSql=socialAiUsageProjection('company_id,month');");
    expect(agentUsage).toContain("agentSocialMonthlyCount=()=>socialAiMonthlyUsageCount('count(*)')");
    expect(records).toContain("export const recordsSocialUsageCountSql = '('+socialAiMonthlyUsageCount('COUNT(*)')+')';");
    expect(finance).toContain('export const financeAiSocialUsageSql=socialAiUsageSql;');
    expect(hospitality).toContain(`social_ai_usage: socialAiUsageProjection('${HOSPITALITY_COLS}'),`);
    expect(beta).toContain("aiUsageSource:socialAiMonthlyUsageCount('input_tokens,output_tokens')");
    expect(budget).toContain("socialMonthlyUsageCountSql='('+socialAiMonthlyUsageCount('count(*)')+')'");
  });
});

describe('Collaboration read-tail r49 protected SQL isolation', () => {
  test('post-r50 revision wrappers, inbox pins, locks and agent-reply guards stay pinned', () => {
    // r50 folded the r28/r37 revision aggregates into one alias-parameterized
    // renderer; the two lines below are now thin wrappers whose rendered SQL is
    // still byte-identical (oracled in read-tail-r50-dedupe.test.ts).
    expect(sha(line(shared, 'export const socialPostRevisionsAgg='))).toBe('69bcdd6271524e9466d33d6d2b0dd21a3dfb9caed2ce81658c8bbb2cd457575d');
    expect(sha(line(shared, 'export const socialPostRevisionsFileAgg='))).toBe('d649d3932556829deac57895db6435f4f6a1fcf6ba717d8d4c954b68200b4c8c');
    expect(sha(line(shared, 'export const socialAiDraftById='))).toBe('fad052f2521c61756c64debcc43e7a3c78b34b24a5c9e3be300ac71b2a52e9bf');
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    const purchases = src('work-social-purchases.ts');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-agent.ts')).toContain('SELECT pg_advisory_xact_lock(hashtext(?))');
  });
});

describe('Collaboration read-tail r49 KPI pins', () => {
  test('retirement:ts-statements is pinned at 272, below the r49 BEFORE=334 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two byte-identical 35-byte AI-usage heads -> one shared renderer with two
    // thin wrappers: 334 -> 333. r50 folds the two revisions aggregates into one
    // alias-parameterized renderer: 333 -> 332.
    expect(total).toBe(272);
    expect(total).toBeLessThan(334);
  });
});
