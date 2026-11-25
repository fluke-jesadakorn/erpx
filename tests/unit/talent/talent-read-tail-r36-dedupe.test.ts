import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const util = new URL('../../../packages/server-talent/src/adapters/talent-read-tail-r36-sources.ts', import.meta.url);
const purchases = new URL('../../../packages/server-talent/src/adapters/purchases.ts', import.meta.url);
const records = new URL('../../../packages/server-talent/src/adapters/record-purchases.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r35 baseline, BEFORE=45). Only the row source
// and predicate are parameterized; the shared `SELECT 1 FROM ... WHERE ...`
// head keeps both emitted reads byte-identical to the former inline copies.
const COMMAND_RECEIPT = `SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`;
const SCOPE = '__SCOPE__';
const WORKSPACE_RECORD = `SELECT 1 FROM workspace_records WHERE module_id='talent' AND id=? AND ${SCOPE}`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text: string): (source: string, predicate: string) => string {
  const helper = text.match(/export const talentExistsSelect = [\s\S]*?;\n/)![0]
    .replace('export const', 'const').replace(/:\s*string/g, '');
  return new Function(`${helper}\nreturn talentExistsSelect;`)();
}

describe('talent read-tail r36 standalone existence-read head family dedupe', () => {
  it('declares the shared head once and routes both reads through it', async () => {
    const [u, p, r] = await Promise.all([util, purchases, records].map(f => readFile(f, 'utf8')));
    expect(u.split('export const talentExistsSelect =').length - 1).toBe(1);
    expect(u.split('SELECT 1 FROM').length - 1).toBe(1);
    expect(p.split('import {talentExistsSelect}').length - 1).toBe(1);
    expect(p.split('talentExistsSelect(').length - 1).toBe(1);
    expect(r.split('import {talentExistsSelect}').length - 1).toBe(1);
    expect(r.split('talentExistsSelect(').length - 1).toBe(1);
    expect(p).not.toContain('"SELECT 1 FROM command_receipts');
    expect(r).not.toContain('`SELECT 1 FROM workspace_records');
  });

  it('emits byte-identical SQL for both former inline reads', async () => {
    const emit = evaluate(await readFile(util, 'utf8'));
    expect(emit('command_receipts', "command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")).toBe(COMMAND_RECEIPT);
    expect(emit('workspace_records', `module_id='talent' AND id=? AND ${SCOPE}`)).toBe(WORKSPACE_RECORD);
  });

  it('keeps native reads, binds, locks and owner-local boundary unchanged', async () => {
    const [p, r] = await Promise.all([purchases, records].map(f => readFile(f, 'utf8')));
    expect(p).toContain('.bind(command.key,command.operation,actor.userId,actor.companyId)');
    expect(p).toContain('nativeClearCommandAssertions(database, actor.companyId, key)');
    expect(r).toContain("nativeTalentReadFirst<{capability:string;company_id:string}>('talent_rd_workspace_record_source',[id],database)");
    expect(r).toContain('.bind(id,...scope.args)');
    // The pre-existing owner source table lock is preserved verbatim; no new
    // lock or write is introduced by routing the two reads through the helper.
    expect(p).toContain('LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(r).not.toContain('LOCK TABLE');
    for (const text of [p, r]) expect(text).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below BEFORE=45', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(11);
    expect(count).toBeLessThan(40);
  });
});
