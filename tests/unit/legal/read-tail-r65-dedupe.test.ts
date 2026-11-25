import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalMembersAggSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r9-sources';
import { legalJsonbAggByItemSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r25-sources';
import { legalItemsByIdCompanyHead } from '../../../packages/server-legal/src/adapters/legal-read-tail-r45-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r65 byte + SHA oracles captured from the literal
 * legalMembersAggSql fragment and the executed attachment call site. The
 * fragment has no binds (it correlates on i.id); both call-site binds still
 * come from the unchanged legalItemsByIdCompanyHead id/company head. */
const AGG = '(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM legal_members m WHERE m.item_id=i.id)';
const AGG_SHA = 'e3ebc36f663fa17f1ad79fb8ce3e0851f0a845f49f00342ffe4457f007d4ec67';

const ATTACHMENT_SQL =
  `SELECT i.*,jsonb_build_object('item',to_jsonb(i)-'version'-'updated_at','members',${AGG})::text image FROM legal_items i WHERE id=? AND company_id=?`;
const ATTACHMENT_SHA = '0a99091a11162ed2cea7f5218b66e96b4abb1597755538b6995d650d7798a5e2';

describe('legal read-tail r65 members jsonb-agg head reuse', () => {
  it('renders the folded fragment byte-identically to the pre-r65 oracle', () => {
    expect(legalMembersAggSql).toBe(AGG);
    expect(sha256(legalMembersAggSql)).toBe(AGG_SHA);
    expect(legalMembersAggSql).toBe(legalJsonbAggByItemSql('m', 'legal_members', 'user_id'));
    expect((legalMembersAggSql.match(/\?/g) ?? []).length).toBe(0);
    expect(legalMembersAggSql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps the attachment call-site SQL and its two binds byte-identical', () => {
    const rendered = legalItemsByIdCompanyHead(
      `i.*,jsonb_build_object('item',to_jsonb(i)-'version'-'updated_at','members',${legalMembersAggSql})::text image`,
    );
    expect(rendered).toBe(ATTACHMENT_SQL);
    expect(sha256(rendered)).toBe(ATTACHMENT_SHA);
    expect((rendered.match(/\?/g) ?? []).length).toBe(2);
    expect(rendered.indexOf('id=?')).toBeLessThan(rendered.indexOf('company_id=?'));
  });

  it('leaves r9 delegating to the single r25 jsonb-agg renderer', async () => {
    const [r9, r25] = await Promise.all([
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r25-sources.ts'),
    ]);
    expect(r9).toContain("from './legal-read-tail-r25-sources'");
    expect(r9).toContain("legalMembersAggSql = legalJsonbAggByItemSql('m','legal_members','user_id')");
    const line = r9.split('\n').find(l => l.includes('legalMembersAggSql ='))!;
    expect(line).not.toContain('SELECT ');
    expect((r25.match(/SELECT /g) ?? []).length).toBe(2);
    expect((r25.match(/export const legalJsonbAggByItemSql/g) ?? []).length).toBe(1);
    expect((r25.match(/export const legalMemberEditExistsSql/g) ?? []).length).toBe(1);
  });

  it('keeps both callers and their locks/native text unchanged', async () => {
    const [att, purchases, r9, r25] = await Promise.all([
      src('attachment-purchases.ts'), src('purchases.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r25-sources.ts'),
    ]);
    expect(att).toContain("import {legalMembersAggSql} from './legal-read-tail-r9-sources'");
    expect(att).toContain("'members',${legalMembersAggSql}");
    expect(purchases).toContain('legalCompanyAdvisoryLockSql,legalMembersAggSql');
    expect(purchases).toContain("'members',${legalMembersAggSql}");
    for (const text of [att, purchases]) expect(text).not.toContain('(SELECT jsonb_agg(to_jsonb(m)');
    expect(att).toContain('LOCK TABLE legal_items,legal_members,legal_documents IN SHARE ROW EXCLUSIVE MODE');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r9).toContain('pg_advisory_xact_lock');
    expect(r25).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('retirement:ts-statements is BEFORE=134 -> AFTER=133', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(134);
    expect(total).toBeLessThanOrEqual(133);
    expect(total).toBe(109);
  });
});
