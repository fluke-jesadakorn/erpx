import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import { nodeExistsGuard } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r50';

vi.mock('@erpx/server-workflow/public/command-assertions', () => ({
  assertion: (db: unknown, commandId: string, step: string, query: string, args: (string | number | null)[]) => ({
    commandId, step, query, args,
  }),
}));

import { peopleHirePlacementFacts } from '../../../packages/server-organization/src/adapters/people-hire-placement-facts';
import { peopleDirectoryOrganizationFacts } from '../../../packages/server-organization/src/adapters/people-directory-facts';

const db = {} as never;
type Captured = { commandId: string; step: string; query: string; args: (string | number | null)[] };

describe('Organization read-tail r53 retirement', () => {
  test('hire-placement guard13 keeps its exact SQL and binds through nodeExistsGuard', () => {
    const guard = peopleHirePlacementFacts(db).guard13('k', 'hire-placement', 'b', 'c', 'd1', 'd2', 'c2', 'b2') as unknown as Captured;
    expect(guard.query).toBe(
      "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND status='active') AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND status='active'))",
    );
    expect(guard.query).toBe(`${nodeExistsGuard("status='active'")} AND (?::text IS NULL OR ${nodeExistsGuard("branch_id=? AND status='active'")})`);
    expect(guard.commandId).toBe('k');
    expect(guard.step).toBe('hire-placement');
    expect(guard.args).toEqual(['b', 'c', 'd1', 'd2', 'c2', 'b2']);
  });

  test('directory guard4 keeps its exact SQL and binds through nodeExistsGuard', () => {
    const guard = peopleDirectoryOrganizationFacts(db).guard4('id', 'department', 'd1', 'd2', 'c', 'b') as unknown as Captured;
    expect(guard.query).toBe(
      "?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND kind='department' AND status='active')",
    );
    expect(guard.query).toBe(`?::text IS NULL OR ${nodeExistsGuard("branch_id=? AND kind='department' AND status='active'")}`);
    expect(guard.commandId).toBe('id');
    expect(guard.step).toBe('department');
    expect(guard.args).toEqual(['d1', 'd2', 'c', 'b']);
  });

  test('retirement:ts-statements drops 224 -> 222', () => {
    const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(space).toBe(214);
    expect(space).toBeLessThan(224);
  });
});
