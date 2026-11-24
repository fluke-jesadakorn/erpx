import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r21-sql.ts';
const CALL_SITES = [
  'adapters/collaboration-home-sources.ts',
  'adapters/operations-handover-facts.ts',
  'adapters/pms/home.ts',
];
const RECIPIENT = 'CROSS JOIN (SELECT ?::text user_id) recipient';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r21 parameterized recipient cross-join dedupe', () => {
  it('declares the shared SQL fragment exactly once', async () => {
    const shared = await src(SHARED);
    expect(shared.split(RECIPIENT).length - 1).toBe(1);
  });

  it('routes every source reader through the shared fragment with no inline literal', async () => {
    const uses = [1, 1, 2];
    for (let i = 0; i < CALL_SITES.length; i++) {
      const text = await src(CALL_SITES[i]);
      // one import plus one use per occurrence in the file
      expect(text.split('PMS_RECIPIENT_CROSS_JOIN_SQL').length - 1).toBe(uses[i] + 1);
      expect(text).not.toContain(RECIPIENT);
    }
  });

  it('keeps the fragment read-only and leaves distinct locking reads inline', async () => {
    expect(RECIPIENT).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
  });

  it('lowers the pms owner SELECT KPI strictly below the r20 after=477 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(358);
    expect(count).toBeLessThan(477);
  });
});
