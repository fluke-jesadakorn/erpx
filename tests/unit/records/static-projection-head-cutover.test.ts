import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  employeeRequestsProjectionSql,
  platformOwnerProjectionSources,
} from '../../../packages/server-records/src/adapters/platform-owner-projection-sources';
import { commerceFinalOwnerProjectionSources } from '../../../packages/server-records/src/adapters/commerce-final-owner-projection-sources';
import { identityVerifiedContextProjections } from '../../../packages/server-records/src/adapters/identity-access-verified-context-projections';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const source = read('packages/server-records/src/adapters/platform-owner-projection-sources.ts');
const contract = read('packages/server-records/src/adapters/owner-projection-head-sources.json');
const rust = read('packages/server-records/src/adapters/owner_projection_head.rs');

// Frozen bytes of the retired employee_requests projection literal.
const projection =
  "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"category\",\"title\",\"description\",\"location\",\"impact\",\"needed_date\",\"items_json\",\"currency\",\"requester_user_id\",\"created_by\",\"parent_request_id\",\"state\",\"team_id\",\"assignee_user_id\",\"routing_json\",\"approval_json\",\"blocked_reason\",\"resolution\",\"purchase_order_id\",\"handover_json\",\"version\",\"evidence_version\",\"created_at\",\"updated_at\",\"engagement_id\" FROM employee_requests";

describe('Records static owner-projection head cutover', () => {
  test('the native contract renders the retired projection byte-identically', () => {
    expect(employeeRequestsProjectionSql).toBe(projection);
    expect(employeeRequestsProjectionSql).toHaveLength(431);
    expect(sha(employeeRequestsProjectionSql)).toBe(
      'f06d1bfcd4bd1cdf1c5a021775eb15f216bbba1fcff68968b43cef966efadf7a',
    );
    expect(employeeRequestsProjectionSql).not.toMatch(
      /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i,
    );
  });

  test('both retained exports and the JSON contract carry the same frozen bytes', () => {
    expect(platformOwnerProjectionSources.employee_requests).toBe(projection);
    expect(JSON.parse(contract)).toEqual({ employeeRequests: projection });
    expect(contract.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  test('the replaced TS literal is deleted and the adapter reads the native contract', () => {
    expect(source).not.toMatch(/SELECT /);
    expect(source).toContain('import contract from "./owner-projection-head-sources.json" with { type: "json" };');
    expect(source).toContain('export const employeeRequestsProjectionSql = heads.employeeRequests;');
    expect(source).not.toContain('.prepare(');
    expect(source).not.toContain('.bind(');
    expect(rust).toContain('include_str!("owner-projection-head-sources.json")');
    expect(rust).toContain('pub fn employee_requests_projection()');
  });

  test('live consumers still compose the exact prior SQL bytes', () => {
    const identity = identityVerifiedContextProjections();
    expect(identity.records_employee_requests_facts_employee_requests_r_from).toBe(
      `FROM (${projection}) r`,
    );
    expect(sha(identity.records_employee_requests_facts_employee_requests_r_from)).toBe(
      '18980ef56ba727d0cb173302fec18c5b9f886b85496430be02b75f07d1e29a4d',
    );
    expect(commerceFinalOwnerProjectionSources.employee_requests).toBe(projection);
    const department = read('packages/server-records/src/adapters/department-override-facts.ts');
    expect(department).toContain(
      "const pendingDepartmentRowsSql = `${employeeRequestsProjectionSql} WHERE company_id=? AND state='awaiting_approval'",
    );
    expect(department).toContain(
      "ORDER BY updated_at DESC LIMIT 150`",
    );
  });

  test('owner retirement:ts-statements drops 7 -> 6', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts'))
          total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, 'packages/server-records/src'));
    expect(total).toBe(6);
    expect(total).toBeLessThan(7);
  });
});
