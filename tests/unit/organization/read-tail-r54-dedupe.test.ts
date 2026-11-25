import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import { activeNodeByKind } from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r27';

vi.mock('@erpx/server-workflow/public/command-assertions', () => ({
  assertion: (db: unknown, commandId: string, step: string, query: string, args: (string | number | null)[]) => ({
    commandId, step, query, args,
  }),
}));

import { attachmentServiceOrganizationFacts } from '../../../packages/server-organization/src/adapters/attachments-service-facts';
import { ORG_BRANCH_COMPANY_SOURCE } from '../../../packages/server-organization/src/adapters/org-sources';

const db = {} as never;
type Captured = { commandId: string; step: string; query: string; args: (string | number | null)[] };
const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r27.ts');

// Byte oracles recorded verbatim from the pre-r54 (BEFORE=222) source. Only
// owner-supplied projections vary; the helper renders each head verbatim.
const ORACLES: [column: string, kind: string, sql: string][] = [
  ['company_id,id branch_id', 'branch', "SELECT company_id,id branch_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'"],
  ['company_id', 'branch', "SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'"],
  ['id,display_name AS name', 'company', "SELECT id,display_name AS name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'"],
  ['1', 'company', "SELECT 1 FROM organization_nodes WHERE id=? AND kind='company' AND status='active'"],
];
const HOLDING_GUARD = "?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND kind='company' AND status='active')";

describe('Organization read-tail r54 dedupe', () => {
  test('the four active company/branch point reads render byte-identical SQL', () => {
    for (const [column, kind, sql] of ORACLES) expect(activeNodeByKind(column, kind)).toBe(sql);
    expect(`?=1 AND EXISTS(${activeNodeByKind('1', 'company')})`).toBe(HOLDING_GUARD);
    // The exported canonical constant keeps its exact pre-r54 value.
    expect(ORG_BRANCH_COMPANY_SOURCE).toBe(ORACLES[1][2]);
  });

  test('the holding-brand guard keeps its exact query and binds at runtime', () => {
    const guard = attachmentServiceOrganizationFacts(db).versionGuard(
      'k', 'holding_brand', 'b', { companyId: 'c' } as never, 7, 3,
    ) as unknown as Captured;
    expect(guard.query).toBe(HOLDING_GUARD);
    expect(guard.query).toBe(`?=1 AND EXISTS(${activeNodeByKind('1', 'company')})`);
    expect(guard.commandId).toBe('k');
    expect(guard.step).toBe('attachment-owner-3');
    expect(guard.args).toEqual([7, 'c']);
  });

  test('r27 family is defined once and every one of the four sites delegates with unchanged binds', () => {
    expect(helper.match(/export const activeNodeByKind =/g) ?? []).toHaveLength(1);
    const migration = read('attachment-migration-owner.ts');
    const org = read('org-sources.ts');
    const product = read('product-company.ts');
    const attachments = read('attachments-service-facts.ts');
    for (const [, , sql] of ORACLES) {
      expect(migration).not.toContain(sql);
      expect(org).not.toContain(sql);
      expect(product).not.toContain(sql);
      expect(attachments).not.toContain(sql);
    }
    expect(migration.split('activeNodeByKind(').length - 1).toBe(2);
    expect(org.split('activeNodeByKind(').length - 1).toBe(1);
    expect(product.split('activeNodeByKind(').length - 1).toBe(1);
    expect(attachments.split('activeNodeByKind(').length - 1).toBe(1);
    expect(attachments.split('versionedNodeActiveGuard(').length - 1).toBe(2);

    expect(migration).toContain("activeNodeByKind('company_id,id branch_id','branch')");
    expect(org).toContain("ORG_BRANCH_COMPANY_SOURCE =\n  activeNodeByKind('company_id','branch');");
    expect(product).toContain("activeNodeByKind('id,display_name AS name','company')");
    expect(attachments).toContain("`?=1 AND EXISTS(${activeNodeByKind('1','company')})`, [version, companyId]");

    // Binds stay byte-identical at every replaced call site.
    expect(migration).toContain('.bind(id).first<{company_id:string;branch_id:string}>()');
    expect(product).toContain('.bind(companyId).first<{ id: string; name: string }>()');
    expect(attachments).toContain('[version, companyId]);');

    // FOR UPDATE / FOR SHARE lock literals and spaced JOIN projections stay untouched.
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements drops 222 -> 218', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(space).toBe(218);
    expect(space).toBeLessThan(222);
  });
});
