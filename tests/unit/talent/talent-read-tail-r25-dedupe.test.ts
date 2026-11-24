import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { talentSourceImageSelect } from '../../../packages/server-talent/src/adapters/talent-read-tail-r25-sources';

const adapter = new URL('../../../packages/server-talent/src/adapters/', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, adapter), 'utf8');

// Exact pre-dedupe emitted SQL (r24 baseline, BEFORE=55). Only the projected
// row shape is parameterized; the caller keeps its own FROM source, predicates
// and binds, so every emitted statement stays byte-identical.
const image = (projection: string) => `SELECT ${projection}::text image`;
const ownerEligible = (ref: string) =>
  `EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=${ref}.company_id AND owner_assignment.user_id=${ref}.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id))`;
const jsonbImage = (application: string, handoff: boolean) =>
  `jsonb_build_object('record',to_jsonb(t),'application',to_jsonb(${application}),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',${ownerEligible(application)}${handoff ? ",'offer',to_jsonb(o),'receipt',to_jsonb(receipt),'people',hired.image" : ''})`;

const scopeTail = 'r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)';
const EXPECTED = {
  candidate: `${image('to_jsonb(c)')} FROM talent_candidates c WHERE c.id=? AND c.company_id=?`,
  requisition: `${image('to_jsonb(r)')} FROM talent_requisitions r WHERE r.id=? AND ${scopeTail}`,
  application: `${image(jsonbImage('t', false))} FROM talent_applications t JOIN talent_requisitions r ON r.id=t.requisition_id AND r.company_id=t.company_id JOIN talent_candidates c ON c.id=t.candidate_id AND c.company_id=t.company_id  WHERE t.id=? AND ${scopeTail} AND t.requisition_version<=r.version`,
  offer: `${image(jsonbImage('a', false))} FROM talent_offers t JOIN talent_applications a ON a.id=t.application_id AND a.company_id=t.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id  WHERE t.id=? AND ${scopeTail} AND t.application_version<=a.version`,
  handoff: `${image(jsonbImage('a', true))} FROM talent_hire_handoffs t JOIN talent_offers o ON o.id=t.offer_id AND o.company_id=t.company_id JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=t.id AND receipt.company_id=t.company_id LEFT JOIN (__PEOPLE_SRC__) hired ON hired.handoff_id=t.id AND hired.company_id=t.company_id WHERE t.id=? AND ${scopeTail} AND t.candidate_id=c.id AND t.branch_id=r.branch_id AND t.department_id IS NOT DISTINCT FROM r.department_id AND t.source_offer_version<=o.version`,
};

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

function evaluateOwnerSources(text: string, helperText: string): (kind: string, file?: boolean) => string {
  const helper = helperText.match(/export const talentSourceImageSelect = [\s\S]*?;\n/)![0]
    .replace('export const', 'const').replace(/:\s*string/g, '');
  // r37 universal head injected so the extracted helper can render standalone.
  const head = 'const selectProjection = (projection) => `SELECT ${projection}`;';
  const scopeDecl = text.match(/const scope="[^"]*";/)![0];
  const start = text.indexOf('export function talentOwnerSourceQuery');
  const end = text.indexOf('\nexport function talentOwnerSourceArgs', start);
  const fn = text.slice(start, end).replace('export function', 'function')
    .replace(/:\s*TalentTarget\["kind"\]/g, '').replace(/:\s*string/g, '');
  return new Function('talentHirePeopleSources', `${head}\n${helper}\n${scopeDecl}\n${fn}\nreturn talentOwnerSourceQuery;`)('__PEOPLE_SRC__');
}

describe('talent read-tail r25 source-image projection family dedupe', () => {
  it('renders byte-identical image SQL for every projected row shape', () => {
    expect(talentSourceImageSelect('to_jsonb(c)')).toBe(image('to_jsonb(c)'));
    expect(talentSourceImageSelect('to_jsonb(r)')).toBe(image('to_jsonb(r)'));
    expect(talentSourceImageSelect(jsonbImage('a', true))).toBe(image(jsonbImage('a', true)));
  });

  it('emits byte-identical owner-source SQL for all five lineages', async () => {
    const q = evaluateOwnerSources(await src('purchases.ts'), await src('talent-read-tail-r25-sources.ts'));
    expect(q('candidate')).toBe(EXPECTED.candidate);
    expect(q('requisition')).toBe(EXPECTED.requisition);
    expect(q('application')).toBe(EXPECTED.application);
    expect(q('offer')).toBe(EXPECTED.offer);
    expect(q('handoff')).toBe(EXPECTED.handoff);
  });

  it('keeps one definition, three call sites and no inline image projection', async () => {
    const [shared, purchases] = await Promise.all([
      src('talent-read-tail-r25-sources.ts'), src('purchases.ts'),
    ]);
    expect(shared.match(/export const talentSourceImageSelect/g) ?? []).toHaveLength(1);
    expect(shared).toContain('selectProjection(');
    expect(shared).toContain("from './talent-read-tail-r37-sources'");
    expect(purchases.match(/\$\{talentSourceImageSelect\(/g) ?? []).toHaveLength(3);
    expect(purchases).toContain("import {talentSourceImageSelect} from './talent-read-tail-r25-sources'");
    expect(purchases).not.toMatch(/SELECT \$\{record\("(c|r)"\)\}::text image/);
    expect(purchases).not.toMatch(/SELECT jsonb_build_object\('record'[\s\S]*?::text image/);
    // No writes or row locks were added to the shared text helper.
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps owner locks, authority guards and consumers unchanged', async () => {
    const text = await src('purchases.ts');
    for (const ref of [
      'LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts IN SHARE ROW EXCLUSIVE MODE',
      'export function talentOwnerSourceQuery',
      'export function talentOwnerSourceArgs',
      'export async function talentOwnerSourceImage',
      'export function talentOwnerSourceReadLocks',
      'export async function requireTalentCurrentAuthority',
      'export async function withTalentPurchasedCommand',
    ]) expect(text, ref).toContain(ref);
  });

  it('lowers the talent owner SELECT KPI strictly below the recorded 55 r24 baseline', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(14);
    expect(count).toBeLessThan(40);
  });
});
