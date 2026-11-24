import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { recordContextDetailSql } from '../../../packages/server-records/src/adapters/records';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered search-references SQL copied from records.ts:130-132 at
// claim, with every dynamic interpolation replaced by a fixed sentinel. The
// embedded newline + four-space indent is preserved byte-for-byte. Whole-string
// byte identity is the oracle: no table, predicate, quote or token may drift.
const S = {
  clock: '<CLOCK>',
  scope: '<SCOPE>',
  purchase: '<PURCHASE_WHERE>',
  commerce: '<COMMERCE_WHERE>',
  payroll: '<PAYROLL_WHERE>',
  people: '<PEOPLE_WHERE>',
  count: '<COUNT_WHERE>',
  procurement: '<PROCUREMENT_WHERE>',
  transfer: '<TRANSFER_WHERE>',
  projects: '<PROJECTS_WHERE>',
  quality: '<QUALITY_WHERE>',
  learning: '<LEARNING_WHERE>',
  assets: '<ASSETS_WHERE>',
  landedCost: '<LANDED_COST_WHERE>',
  manufacturing: '<MANUFACTURING_WHERE>',
  talent: '<TALENT_WHERE>',
  legal: '<LEGAL_WHERE>',
  crm: '<CRM_WHERE>',
  marketing: '<MARKETING_WHERE>',
  followups: '<FOLLOWUPS_WHERE>',
  workforce: '<WORKFORCE_WHERE>',
  benefits: '<BENEFITS_WHERE>',
  service: '<SERVICE_WHERE>',
  work: '<WORK_WHERE>',
  placeholders: '<PLACEHOLDERS>',
} as const;

const columns = 'module_id,id,title,status,version';
const table = 'workspace_records';
const order = ' ORDER BY updated_at DESC LIMIT 12';
const where = `archived_at IS NULL AND ${S.scope}
    AND ${S.purchase} AND ${S.commerce} AND ${S.payroll} AND ${S.people} AND ${S.count} AND ${S.procurement} AND ${S.transfer} AND ${S.projects} AND ${S.quality} AND ${S.learning} AND ${S.assets} AND ${S.landedCost} AND ${S.manufacturing} AND ${S.talent} AND ${S.legal} AND ${S.crm} AND ${S.marketing} AND ${S.followups} AND ${S.workforce} AND ${S.benefits} AND ${S.service} AND ${S.work} AND module_id IN (${S.placeholders}) AND (module_id||'/'||id ILIKE ? OR title ILIKE ?)`;
const pre = {
  sql: `${S.clock}SELECT ${columns} FROM ${table} WHERE ${where}${order}`,
  sha: 'a68806b1c47fa790c71c97c4bdef62501967f7476c2ecc3245514b471288bb3a',
} as const;

describe('Records read-tail r27 recordContextDetailSql family', () => {
  test('search references renders byte-identically and stays read-only', () => {
    const actual = `${S.clock}${recordContextDetailSql(table, columns, where, order)}`;
    expect(actual).toBe(pre.sql);
    expect(sha(actual)).toBe(pre.sha);
    expect(sha(pre.sql)).toBe(pre.sha);
    expect((actual.match(/SELECT /g) ?? []).length).toBe(1);
    readOnly(actual);
  });

  test('the family is one generator literal with five call sites and the clock stays outside', () => {
    expect(records.match(/const recordContextDetailSql\s*=/g) ?? []).toHaveLength(1);
    expect(records.match(/recordContextDetailSql\(/g) ?? []).toHaveLength(5); // mentions/suggestions/transitions + search references + reference-edge family
    expect(records).toContain('const recordContextDetailSql = (table: string, columns: string, where: string, order: string) =>');
    expect(records).toContain('`SELECT ${columns} FROM ${table} WHERE ${where}${order}`');
    // Search references now routes through the shared generator: the clock is
    // prepended outside it, the embedded newline/indent and binds are unchanged.
    expect(records).toContain("${purchase.clock}${recordContextDetailSql('workspace_records','module_id,id,title,status,version',`archived_at IS NULL AND ${scope.sql}");
    expect(records).toContain("' ORDER BY updated_at DESC LIMIT 12')");
    expect(records).toContain("module_id IN (${placeholders}) AND (module_id||'/'||id ILIKE ? OR title ILIKE ?)");
    expect(records).toContain('.bind(...args,...purchase.args,...commerce.args,');
    expect(records).toContain('...work.args, ...modules, term, term2).all();');
    expect(records).not.toContain('${purchase.clock}SELECT module_id,id,title,status,version FROM workspace_records');
    expect(records).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r11-r25 families, cross-owner ports and all Records source locks stay independent', () => {
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).toContain('const referenceEdgesSql ='); // r14
    expect(records).toContain('const recordContextDetailSql ='); // r15
    expect(records).toContain("import {withExpenseWorkspaceRecordRead} from '@erpx/server-people_payroll/public/record-purchases';");
    expect(records).toContain('financeWorkspaceSearchScope} from "@erpx/server-finance/public/document-purchases"');
    expect(records).toContain("import { RecordsIdentityAccessOwnedFacts } from '@erpx/server-identity_access/public/records-records-owned-facts';");
    const legal = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(legal).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(legal).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Records read-tail r27 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r27 BEFORE=25 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r25 folded the two projection-source guards (27 -> 26), r26 folded the
    // workspace version fence (26 -> 25), r27 folded the search references
    // SELECT into the record context detail generator (25 -> 24) and r28 folds
    // the reference-edge family into that generator (24 -> 23). No lock,
    // cross-owner port or native seam changed.
    expect(total).toBe(23);
    expect(total).toBeLessThan(25);
  });
});
