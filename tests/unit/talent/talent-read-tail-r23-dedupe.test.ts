import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/package-obligations.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r22 baseline). Only the CTE name, row alias and
// lineage source are parameterized; the pinned strings are owner-local.
const EXPECTED_CTES = 'WITH scope AS(SELECT ?::text company_id),requisitions AS MATERIALIZED(SELECT r.* FROM (REQ) r,scope WHERE r.company_id=scope.company_id),applications AS MATERIALIZED(SELECT a.* FROM (APP) a,scope WHERE a.company_id=scope.company_id),offers AS MATERIALIZED(SELECT o.* FROM (OFF) o,scope WHERE o.company_id=scope.company_id)';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text: string): string {
  const helper = text.match(/const scopedMaterialized=[\s\S]*?;\n/)![0].replace(/:\s*string/g, '');
  // r37 universal head injected so the extracted helper can render standalone.
  const head = 'const selectProjection = (projection) => `SELECT ${projection}`;';
  const scopedMaterialized = new Function(head + helper + ' return scopedMaterialized;')();
  const start = text.indexOf('`WITH scope AS') + 1;
  const end = text.indexOf('\n', start);
  const tpl = text.slice(start, end);
  // r38 scope CTE routes its projection head through the same r37 universal
  // helper so the emitted WITH-scope prefix stays byte-identical.
  const selectProjection = (projection: string) => `SELECT ${projection}`;
  return new Function('scopedMaterialized', 'requisitions', 'applications', 'offers', 'selectProjection', 'return `' + tpl + '`;')(
    scopedMaterialized, 'REQ', 'APP', 'OFF', selectProjection,
  );
}

describe('talent read-tail r23 scoped-materialized CTE family dedupe', () => {
  it('declares the shared family once and routes all three CTEs through it', async () => {
    const text = await readFile(file, 'utf8');
    expect(text.split('const scopedMaterialized=').length - 1).toBe(1);
    expect(text.split('${scopedMaterialized(').length - 1).toBe(3);
    // the materialized head now renders through the r37 selectProjection head
    expect(text.split('AS MATERIALIZED(${selectProjection(').length - 1).toBe(1);
    expect(text).toContain("from \"./talent-read-tail-r37-sources\"");
  });

  it('emits byte-identical SQL for the company-scoped materialized CTEs', async () => {
    expect(evaluate(await readFile(file, 'utf8'))).toBe(EXPECTED_CTES);
  });

  it('keeps source locks and consumers unchanged', async () => {
    const text = await readFile(file, 'utf8');
    for (const ref of [
      'LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts',
      'talentSettlementPlacementReadLocks(database)',
      'export const talentPackageSettlementSources',
      'export async function talentPackageObligations',
      'kind,r.id,r.version,r.status state,NOT r.source_valid',
    ]) expect(text, ref).toContain(ref);
    expect(text).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
  });

  it('re-pins the talent owner SELECT KPI strictly below BEFORE=48 to 47', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(10);
    expect(count).toBeLessThan(40);
  });
});
