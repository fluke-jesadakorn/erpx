import { readFile, readdir, access } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const adapter = new URL('../../../packages/server-talent/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, adapter), 'utf8');

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

// r25 owned the shared owner-source image projection head. All six lineages
// (candidate, requisition, application, proposal, offer, handoff) now plan
// their exact statement in the native registry; the head and the TS renderer
// are retired, so no TS caller can author an owner-source image again.
const kinds = [
  'candidate', 'candidate_file', 'requisition', 'requisition_file',
  'application', 'application_file', 'proposal', 'proposal_file',
  'offer', 'offer_file', 'handoff', 'handoff_file',
] as const;
const constant = (name: string) =>
  `RD_${name.replace('_file', '').toUpperCase()}_SOURCE_IMAGE${name.endsWith('_file') ? '_FILE' : ''}`;

describe('talent owner-source image head retirement (r25/r37)', () => {
  it('removes the shared TS renderer and every inline owner-image literal', async () => {
    const purchases = await src('purchases.ts');
    expect(purchases).not.toContain('talentOwnerSourceQuery');
    expect(purchases).not.toContain('talentSourceImageSelect');
    expect(purchases).not.toContain('owner_assignment.scope_kind');
    expect(purchases).not.toContain('talentHirePeopleSources');
    expect(purchases).toContain('const ownerImageKind=');
    expect(purchases).toContain('nativeTalentPlan(');
    expect(purchases).toContain('nativeTalentReadFirst<{image:string}>(');
    await expect(access(new URL('talent-read-tail-r25-sources.ts', adapter))).rejects.toThrow();
  });

  it('registers all twelve byte-exact native owner-image plans', async () => {
    const registry = await src('native_talent_read_sql.rs');
    for (const name of kinds) {
      const base = name.replace('_file', '');
      const kind = `talent_rd_${base}_source_image${name.endsWith('_file') ? '_file' : ''}`;
      expect(registry, kind).toContain(`pub const ${constant(name)}: &str = "${kind}"`);
      expect(registry, kind).toContain(`READ_${constant(name).slice(3)},`);
    }
    expect(registry).toContain('assert_eq!(STATEMENTS.len(), 30)');
    // The IAM owner-eligibility witness now lives in the owner plans only.
    expect(registry).toContain("'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment");
  });

  it('lowers the talent owner SELECT KPI from 11 to 10', async () => {
    let count = 0;
    for (const file of await tsFiles(new URL('../../../packages/server-talent/src/', import.meta.url))) {
      count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    }
    expect(count).toBe(10);
    expect(count).toBeLessThan(11);
  });
});
