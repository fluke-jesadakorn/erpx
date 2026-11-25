import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { projectionColumnHeadSql } from '../../../packages/server-records/src/adapters/read-tail-r32-sql';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const source = read('packages/server-records/src/adapters/employee-requests-read.ts');

// Frozen value of the retained cross-owner actor-name projection fragment.
const actorNameSql = "COALESCE((SELECT legal_name FROM hr_people p WHERE p.company_id=e.company_id AND p.user_id=e.actor_id AND p.merged_into_id IS NULL LIMIT 1),'Company member')";

describe('Records employee_request_events detail head cutover', () => {
  test('the single-row event read renders byte-identically through the shared r32 head', () => {
    const sql = projectionColumnHeadSql(
      `e.id,e.action,e.actor_id,${actorNameSql} actor_name,e.note,e.evidence_json,e.request_version,e.created_at`,
      'employee_request_events e',
      ' WHERE request_id=? AND company_id=? ORDER BY created_at,id LIMIT 250',
    );
    expect(sql).toBe(
      "SELECT e.id,e.action,e.actor_id,COALESCE((SELECT legal_name FROM hr_people p WHERE p.company_id=e.company_id AND p.user_id=e.actor_id AND p.merged_into_id IS NULL LIMIT 1),'Company member') actor_name,e.note,e.evidence_json,e.request_version,e.created_at FROM employee_request_events e WHERE request_id=? AND company_id=? ORDER BY created_at,id LIMIT 250",
    );
    expect(sha(sql)).toBe('3f17e32c7047e42c0dbee48d14a2581d4ba029dfd5a37f08f9618c845b983f0b');
    expect(sql).toContain('ORDER BY created_at,id LIMIT 250');
  });

  test('the replaced TS literal is deleted and the call site delegates', () => {
    expect(source).not.toMatch(/SELECT e\.id,e\.action|SELECT \* FROM/);
    expect(source).toContain("import { projectionColumnHeadSql } from './read-tail-r32-sql';");
    expect(source).toContain('projectionColumnHeadSql(`e.id,e.action,e.actor_id,${requestActorNameSql} actor_name');
    expect(source).toContain("'employee_request_events e',' WHERE request_id=? AND company_id=? ORDER BY created_at,id LIMIT 250')");
    expect(source.match(/projectionColumnHeadSql\(/g) ?? []).toHaveLength(1);
    // Bindings stay in request_id, company_id order.
    expect(source).toContain('.bind(id, companyId)');
  });

  test('owner retirement:ts-statements drops 8 -> 7', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, 'packages/server-records/src'));
    expect(total).toBe(7);
    expect(total).toBeLessThan(8);
  });
});
