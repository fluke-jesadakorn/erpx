import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalMemberEditExistsSql, legalJsonbAggByItemSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r25-sources';
import workContract from '../../../packages/server-legal/src/adapters/work-purchases-sources.json' with {
  type: 'json',
};
import contract from '../../../packages/server-legal/src/adapters/legal-read-tail-r25-sources.json' with {
  type: 'json',
};

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const memberEdit = (alias: string) => `EXISTS(SELECT 1 FROM legal_members ${alias} WHERE ${alias}.item_id=i.id AND ${alias}.user_id=? AND ${alias}.access='edit')`;
const aggBy = (alias: string, table: string, order = 'id') => `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${order}) FROM ${table} ${alias} WHERE ${alias}.item_id=i.id)`;

const aggs: [string, string, string][] = [
  ['d', 'legal_documents', 'id'],
  ['s', 'legal_shares', 'id'],
  ['e', 'legal_executed_copies', 'id'],
  ['ap', 'legal_approvals', 'cycle'],
  ['au', 'legal_authorities', 'id'],
  ['dx', 'legal_destructions', 'id'],
  ['h', 'legal_holds', 'id'],
  ['d', 'legal_destructions', 'id'],
];

describe('legal read-tail r25 parameterized family dedupe', () => {
  it('renders byte-identical edit-member existence SQL for both aliases', () => {
    expect(legalMemberEditExistsSql('em')).toBe(memberEdit('em'));
    expect(legalMemberEditExistsSql('editor')).toBe(memberEdit('editor'));
    expect(legalMemberEditExistsSql('$&')).toBe(memberEdit('$&'));
  });

  it('renders byte-identical jsonb aggregate SQL for every alias/table/order', () => {
    for (const [alias, table, order] of aggs) {
      expect(legalJsonbAggByItemSql(alias, table, order)).toBe(aggBy(alias, table, order));
    }
    expect(legalJsonbAggByItemSql('d', 'legal_documents')).toBe(aggBy('d', 'legal_documents'));
  });

  it('keeps both fragments read-only (no writes or row locks)', () => {
    const all = [legalMemberEditExistsSql('em'), ...aggs.map(([a, t, o]) => legalJsonbAggByItemSql(a, t, o))].join(' ');
    expect(all).toMatch(/SELECT 1 FROM legal_members|jsonb_agg\(to_jsonb/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition each and no inline family literals in callers', async () => {
    const [shared, access, work, purchases, canonical] = await Promise.all([
      src('legal-read-tail-r25-sources.ts'),
      src('legal/access.ts'),
      src('work-purchases.ts'),
      src('purchases.ts'),
      src('canonical-attachment-read.ts'),
    ]);
    expect(shared.match(/export const legalMemberEditExistsSql/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const legalJsonbAggByItemSql/g) ?? []).toHaveLength(1);
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(shared).toContain('legal-read-tail-r25-sources.json');
    expect(shared).toContain('LEGAL_READ_TAIL_R25_CONTRACT_INVALID');
    expect(contract).toEqual({
      memberEditExists:
        "EXISTS(SELECT 1 FROM legal_members {alias} WHERE {alias}.item_id=i.id AND {alias}.user_id=? AND {alias}.access='edit')",
      jsonbAggByItem:
        '(SELECT jsonb_agg(to_jsonb({alias}) ORDER BY {alias}.{order}) FROM {table} {alias} WHERE {alias}.item_id=i.id)',
    });
    const rust = await src('legal_read_tail_r25.rs');
    expect(rust).toContain('include_str!("legal-read-tail-r25-sources.json")');
    expect(rust).toContain('render_legal_member_edit_exists');
    expect(rust).toContain('render_legal_jsonb_agg_by_item');
    expect(rust).toContain('435206d2b50d517fc159f385c28867f259b718f6d5a52005431e4d8c4480ee50');
    expect(access).toContain('legalMemberEditExistsSql("em")');
    expect(access).not.toContain(memberEdit('em'));
    expect(work).toContain('legalMemberEditExistsSql("editor")');
    expect(work).not.toContain(memberEdit('editor'));
    for (const file of [purchases, canonical]) {
      expect(file).not.toMatch(/\(SELECT jsonb_agg\(to_jsonb\(\w+\) ORDER BY \w+\.\w+\) FROM legal_\w+ \w+ WHERE \w+\.item_id=i\.id\)/);
    }
    expect(purchases).toContain('legalJsonbAggByItemSql("ap","legal_approvals","cycle")');
    expect(canonical).toContain('legalJsonbAggByItemSql("h","legal_holds")');
  });

  it('keeps the work-purchases filter sources and row read in the native contract', async () => {
    const [work, rust] = await Promise.all([
      src('work-purchases.ts'),
      src('work_purchases.rs'),
    ]);
    expect(work.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(work).toContain('work-purchases-sources.json');
    expect(work).toContain('fill(contract.rows');
    expect(workContract.recordSource).toBe(
      "SELECT id work_id,id item_id,'record' source FROM legal_items",
    );
    expect(workContract.approvalSource).toBe(
      "SELECT 'legal-approval:'||id,id,'approval' FROM legal_items",
    );
    expect(workContract.featureSource).toContain('{table}');
    expect(workContract.rows).toContain('{memberEdit}');
    expect(workContract.rows).toContain('{inList}');
    expect(workContract.rows).toContain('{scope}');
    expect(workContract.rows).toContain('{purchase}');
    expect(rust).toContain('include_str!("work-purchases-sources.json")');
    expect(rust).toContain('render_work_rows');
    expect(rust).toContain('36313aadc7878271d3b810421a6d31d7ca59e5e260c7d9a5568f064cb81c9ace');
  });

  it('retirement:ts-statements stays strictly below the recorded 259 r24 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(259);
    expect(total).toBeLessThanOrEqual(251);
  });

  it('retirement:ts-statements is BEFORE=103 -> AFTER=101 for the r25 native contract', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(103);
    expect(total).toBe(35);
  });
});
