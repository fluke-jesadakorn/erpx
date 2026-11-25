import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/people-hire-handoff-facts.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r28 baseline, BEFORE=50). Only the row source,
// WHERE predicate and optional `NOT ` are parameterized; the caller keeps its
// binds, so every emitted assertion guard is byte-identical to the former
// inline copies. This is the NOT-capable assertion shape, distinct from r24's
// positive-only existsRow and r27's version-checked existsVersionRow.
const HIRE_SOURCE = `EXISTS(SELECT 1 FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id WHERE h.id=? AND h.company_id=? AND o.status='accepted' AND o.version=? AND h.source_offer_version=o.version AND c.version=? AND c.status='active' AND c.retention_until>=?)`;
const NOT_CONSUMED = `NOT EXISTS(SELECT 1 FROM talent_hire_consumption_receipts WHERE handoff_id=?)`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text: string): (negated: boolean, source: string, where: string) => string {
  const helper = text.match(/const existsAssertion=[\s\S]*?;\n/)![0]
    .replace(/:\s*boolean/g, '').replace(/:\s*string/g, '');
  return new Function(`${helper}\nreturn existsAssertion;`)();
}

describe('talent read-tail r29 assertion EXISTS guard family dedupe', () => {
  it('declares the shared family once and routes both guards through it', async () => {
    const text = await readFile(file, 'utf8');
    expect(text.split('const existsAssertion=').length - 1).toBe(1);
    expect(text.split('existsAssertion(').length - 1).toBe(2); // guard9 + guard10 call sites
    expect(text.split('EXISTS(SELECT 1 FROM').length - 1).toBe(1); // helper body only
    expect(text.split('NOT EXISTS(SELECT 1 FROM').length - 1).toBe(0);
    expect(text).not.toMatch(/assertion\(database,key,value,"(NOT )?EXISTS\(SELECT 1 FROM/);
  });

  it('emits byte-identical SQL for both former inline guards', async () => {
    const guard = evaluate(await readFile(file, 'utf8'));
    expect(guard(false, 'talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id', "h.id=? AND h.company_id=? AND o.status='accepted' AND o.version=? AND h.source_offer_version=o.version AND c.version=? AND c.status='active' AND c.retention_until>=?")).toBe(HIRE_SOURCE);
    expect(guard(true, 'talent_hire_consumption_receipts', 'handoff_id=?')).toBe(NOT_CONSUMED);
  });

  it('keeps source consumers, binds and locks unchanged', async () => {
    const text = await readFile(file, 'utf8');
    for (const ref of [
      'const candidateLockSql=(mode:\'SHARE\'|\'UPDATE\')=>`SELECT id FROM talent_candidates WHERE id=? FOR ${mode}`;',
      'stageConsumeHireHandoffEffect2(candidate_id:string){return database.prepare(candidateLockSql("SHARE")).bind(candidate_id);}',
      'stageChangeCandidatePrivacyEffect(candidateId:string){return database.prepare(candidateLockSql("UPDATE")).bind(candidateId);}',
      'existsVersionRow("talent_candidates"," AND status=? AND privacy_hold::int=?")',
      '[id,companyId,expectedOfferVersion,expectedCandidateVersion,today]',
      '[id]);}',
      'guard9(key:string,value:"hire-source"',
      'guard10(key:string,value:"hire-not-consumed"',
    ]) expect(text, ref).toContain(ref);
    expect(text).not.toMatch(/LOCK TABLE|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below the recorded 50 r28 baseline', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(12);
    expect(count).toBeLessThan(40);
  });
});
