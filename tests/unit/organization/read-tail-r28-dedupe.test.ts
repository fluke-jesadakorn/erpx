import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  companySnapshotAggregate,
  correlatedNodeJson,
} from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r28';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const helper = read('organization-column-alias-families-r28.ts');
const graphGuard = read('organization-graph-guard.ts');
const projectWorker = read('project-worker-startup-sources.ts');
const attachmentRead = read('attachment-read-sources.ts');

describe('Organization read-tail r28 relation/alias-parameterized families', () => {
  test('companySnapshotAggregate stays byte-identical to the six snapshot image literals', () => {
    expect(companySnapshotAggregate('organization_positions', 'p')).toBe(
      '(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM organization_positions p WHERE p.company_id=?)',
    );
    expect(companySnapshotAggregate('staff_levels', 'l')).toBe(
      '(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM staff_levels l WHERE l.company_id=?)',
    );
    expect(companySnapshotAggregate('organization_nodes', 'n')).toBe(
      '(SELECT jsonb_agg(to_jsonb(n) ORDER BY n.id) FROM organization_nodes n WHERE n.company_id=?)',
    );
  });

  test('correlatedNodeJson stays byte-identical to the three attachment image literals', () => {
    expect(correlatedNodeJson('branch', 'node.branch_id', 'node.company_id')).toBe(
      '(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=node.branch_id AND branch.company_id=node.company_id)',
    );
    expect(correlatedNodeJson('node', 'change.target_node_id', 'change.company_id')).toBe(
      '(SELECT to_jsonb(node) FROM organization_nodes node WHERE node.id=change.target_node_id AND node.company_id=change.company_id)',
    );
    expect(correlatedNodeJson('branch', 'change.branch_id', 'change.company_id')).toBe(
      '(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=change.branch_id AND branch.company_id=change.company_id)',
    );
  });

  test('each r28 family is defined exactly once and r17-r27 helpers are untouched', () => {
    for (const name of ['companySnapshotAggregate', 'correlatedNodeJson']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    const prior = read('organization-column-alias-families.ts');
    const priorR27 = read('organization-column-alias-families-r27.ts');
    const tail = read('organization-read-tail-sources.ts');
    for (const name of ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']) {
      expect(prior.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']) {
      expect(priorR27.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['departmentSnapshotMissing', 'ownerVersionGuardSource']) {
      expect(tail.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone and every fact reader calls its r28 helper', () => {
    for (const table of ['organization_positions', 'staff_levels', 'organization_nodes']) {
      expect(graphGuard).not.toContain(`jsonb_agg(to_jsonb(${table === 'organization_positions' ? 'p' : table === 'staff_levels' ? 'l' : 'n'})`);
      expect(projectWorker).not.toContain(`jsonb_agg(to_jsonb(${table === 'organization_positions' ? 'p' : table === 'staff_levels' ? 'l' : 'n'})`);
    }
    expect(graphGuard.split('companySnapshotAggregate(').length - 1).toBe(3);
    expect(projectWorker.split('companySnapshotAggregate(').length - 1).toBe(3);
    expect(attachmentRead).not.toContain('(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=node.branch_id');
    expect(attachmentRead).not.toContain('(SELECT to_jsonb(node) FROM organization_nodes node WHERE node.id=change.target_node_id');
    expect(attachmentRead).not.toContain('(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=change.branch_id');
    expect(attachmentRead.split('correlatedNodeJson(').length - 1).toBe(3);
  });
});

describe('Organization read-tail r28 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r28 BEFORE=299 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThanOrEqual(277);
    expect(total).toBeLessThan(299);
  });
});
