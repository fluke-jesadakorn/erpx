import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const util = new URL('../../../packages/server-talent/src/adapters/talent-read-tail-r27-sources.ts', import.meta.url);
const privacy = new URL('../../../packages/server-talent/src/adapters/people-hire-handoff-facts.ts', import.meta.url);
const sensitive = new URL('../../../packages/server-talent/src/adapters/attachments-sensitive-facts.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r26 baseline, BEFORE=52). Only the row source
// and the trailing predicate are parameterized; the fixed
// `id=? AND company_id=? AND version=?` prefix and bind order stay in the
// caller, so every emitted guard is byte-identical to the former inline copies.
const CANDIDATE_PRIVACY = `EXISTS(SELECT 1 FROM talent_candidates WHERE id=? AND company_id=? AND version=? AND status=? AND privacy_hold::int=?)`;
const SENSITIVE_PLAIN = `EXISTS(SELECT 1 FROM talent_candidates WHERE id=? AND company_id=? AND version=?)`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text: string): (source: string, extra?: string) => string {
  const prefix = text.match(/export const existsGuardPrefix = [\s\S]*?;\n/)![0].replace('export const', 'const').replace(/:\s*string/g, '');
  const helper = text.match(/export const existsVersionRow = [\s\S]*?version=\?\$\{extra\}\)`;\n/)![0]
    .replace('export const', 'const').replace(/:\s*string/g, '');
  return new Function(`${prefix}\n${helper}\nreturn existsVersionRow;`)();
}

describe('talent read-tail r27 version-guard family dedupe', () => {
  it('declares the shared family once and routes both guards through it', async () => {
    const [u, p, s] = await Promise.all([util, privacy, sensitive].map(f => readFile(f, 'utf8')));
    expect(u.split('export const existsGuardPrefix =').length - 1).toBe(1);
    expect(u.split('const existsVersionRow =').length - 1).toBe(1);
    expect(u.split('EXISTS(SELECT 1 FROM').length - 1).toBe(1);
    expect(u).toContain('${existsGuardPrefix(source)}id=? AND company_id=? AND version=?${extra})');
    expect(p.split('import {existsVersionRow}').length - 1).toBe(1);
    expect(p.split('existsVersionRow(').length - 1).toBe(1); // guard21 call
    expect(s.split('import {existsVersionRow}').length - 1).toBe(1);
    expect(s.split('existsVersionRow(').length - 1).toBe(1); // version guard call
    expect(p).not.toMatch(/EXISTS\(SELECT 1 FROM talent_candidates WHERE id=\? AND company_id=\? AND version=\?/);
    expect(s).not.toMatch(/EXISTS\(SELECT 1 FROM \$\{table\} WHERE id=\? AND company_id=\? AND version=\?/);
  });

  it('emits byte-identical SQL for both former inline guards', async () => {
    const guard = evaluate(await readFile(util, 'utf8'));
    expect(guard('talent_candidates', ' AND status=? AND privacy_hold::int=?')).toBe(CANDIDATE_PRIVACY);
    expect(guard('talent_candidates')).toBe(SENSITIVE_PLAIN);
  });

  it('keeps source consumers, binds and locks unchanged', async () => {
    const [p, s] = await Promise.all([privacy, sensitive].map(f => readFile(f, 'utf8')));
    expect(p, 'candidate privacy guard binds').toContain('[candidateId,companyId,version,status,value2]');
    expect(p, 'candidate privacy native read').toContain("nativeTalentReadFirst<");
    expect(s, 'sensitive version guard binds').toContain('[targetId, companyId, version]');
    expect(s, 'offer-mutable witness retained').toContain('offerFilesMutable(table)');
    for (const text of [p, s]) expect(text).not.toMatch(/LOCK TABLE|FOR\s+UPDATE|FOR\s+SHARE|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below BEFORE=47', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(13);
    expect(count).toBeLessThan(40);
  });
});
