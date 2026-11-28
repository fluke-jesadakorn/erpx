import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import ts from 'typescript';

const adapters = resolve(__dirname, '../../../modules/talent/server/src/adapters');
const src = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const compile = (decl: string) => ts.transpileModule(decl, { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Real owner-local renderer, extracted from source so the oracles exercise the
// shipped implementation rather than a copy of its body.
const headDecl = src('talent-read-tail-r42-sources.ts').match(/export const selectSourceRow = [\s\S]*?;\n/)![0].replace('export const', 'const ');
const selectSourceRow = new Function(compile(headDecl) + '\nreturn selectSourceRow;')() as (projection: string) => string;

// Exact pre-r42 rendered fragments (BEFORE=13). Captured from the former inline
// `<select> <projection> FROM` literals; stubs keep every value local, the
// shared head is the only thing that moved.
const GOLDEN = {
  hireEntry: "__CLOCK__(hire_entry_clock)\n SELECT h.id FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id\n JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id\n JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id AND r.branch_id=h.branch_id AND r.department_id IS NOT DISTINCT FROM h.department_id\n JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id AND c.company_id=h.company_id\n CROSS JOIN __SCOPE__ hire_entry CROSS JOIN hire_entry_clock\n WHERE h.id=? AND h.company_id=hire_entry.company_id AND o.status='accepted' AND h.source_offer_version<=o.version\n AND __READ__(hire_entry,people.manage) AND __VER__\n AND __FEAT__(h.company_id,talent) AND __FEAT__(h.company_id,people_payroll)\n AND __PURCH__(h.company_id,recruit.offers,history,hire_entry_clock.now_ms)",
  candidatePrivacyEntry: "__CLOCK__(candidate_privacy_clock)\n SELECT c.id FROM talent_candidates c CROSS JOIN __SCOPE__ candidate_privacy_entry CROSS JOIN candidate_privacy_clock\n WHERE c.id=? AND c.company_id=candidate_privacy_entry.company_id AND (__READ__(candidate_privacy_entry,legal.hold) OR __READ__(candidate_privacy_entry,legal.hold.release) OR __READ__(candidate_privacy_entry,legal.destroy) OR __READ__(candidate_privacy_entry,people.manage)) AND __VER__\n AND __FEAT__(c.company_id,talent)\n AND __PURCH__(c.company_id,recruit.candidates,history,candidate_privacy_clock.now_ms)",
} as const;
const DIGEST = {
  hireEntry: [873, '0fa69eca0dc02b9c4860a85275723c99fdfaa717a50800f10d61e3e8f940f4b1'],
  candidatePrivacyEntry: [561, '731b08f6b1938f66ed3d9b619a2b3d6bbee5dc0aaa1f34da1690332653bc33df'],
} as const;

// The two database.prepare template literals in product-entry.ts, in source order.
function templates(): string[] {
  const text = src('product-entry.ts');
  const out: string[] = [];
  let pos = 0;
  while (true) {
    const start = text.indexOf('database.prepare(`', pos);
    if (start < 0) break;
    const tick = start + 'database.prepare('.length;
    let i = tick + 1;
    let depth = 0;
    while (i < text.length) {
      const c = text[i];
      if (c === '\\') { i += 2; continue; }
      if (c === '$' && text[i + 1] === '{') { depth++; i += 2; continue; }
      if (c === '}' && depth > 0) { depth--; i++; continue; }
      if (c === '`' && depth === 0) break;
      i++;
    }
    out.push(text.slice(tick, i + 1));
    pos = i + 1;
  }
  return out;
}

function render(tpl: string): string {
  const stubs: Record<string, unknown> = {
    talentClockCte: (name: string) => `__CLOCK__(${name})`,
    talentCompanyScopeCte: '__SCOPE__',
    assignmentReadSql: (a: string, p: string) => `__READ__(${a},${p})`,
    assignmentVersionGuard: '__VER__',
    scheduledFeaturePredicate: (a: string, b: string) => `__FEAT__(${a},${b})`,
    purchasedCapabilityPredicate: (...a: string[]) => `__PURCH__(${a.join(',')})`,
    permissions: ['legal.hold', 'legal.hold.release', 'legal.destroy', 'people.manage'],
    selectSourceRow,
  };
  const names = Object.keys(stubs);
  return new Function(...names, `return ${tpl};`)(...names.map((n) => stubs[n])) as string;
}

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r42 source-row select head family', () => {
  test('both gates render byte-identically to the pre-r42 inline literals', () => {
    const [hire, privacy] = templates();
    const actual = { hireEntry: render(hire), candidatePrivacyEntry: render(privacy) };
    for (const key of ['hireEntry', 'candidatePrivacyEntry'] as const) {
      expect(actual[key], key).toBe(GOLDEN[key]);
      expect(actual[key].length, key).toBe(DIGEST[key][0]);
      expect(sha(actual[key]), key).toBe(DIGEST[key][1]);
      readOnly(actual[key]);
    }
  });

  test('one owner-local head, both call sites routed, no inline source-row literal remains', () => {
    const head = src('talent-read-tail-r42-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(head.match(/export const selectSourceRow/g) ?? []).toHaveLength(1);
    expect(selectSourceRow('h.id')).toBe('SELECT h.id FROM');
    expect(selectSourceRow('c.id')).toBe('SELECT c.id FROM');
    readOnly(selectSourceRow('h.id'));

    const callsite = src('product-entry.ts');
    expect(callsite.match(/selectSourceRow\(/g) ?? []).toHaveLength(2);
    expect(callsite.match(/\bSELECT (h|c)\.id FROM\b/g) ?? []).toHaveLength(0);
    expect(callsite).toMatch(/from ['"]\.\/talent-read-tail-r42-sources['"]/);
  });

  test('locks, native reads, prior families and cross-owner literals stay untouched', () => {
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE talent_requisitions');
    expect(src('people-hire-handoff-facts.ts')).toContain('FOR ${mode}');
    expect(src('native-talent-read.ts')).toContain('nativeTalentReadFirst');
    expect(src('talent-read-tail-r27-sources.ts')).toContain('EXISTS(SELECT 1 FROM ${source} WHERE ');
    expect(src('talent-read-tail-r36-sources.ts')).toContain('SELECT 1 FROM ${source} WHERE ${predicate}');
    expect(src('talent-read-tail-r37-sources.ts')).toContain('export const selectProjection');
    expect(src('talent-read-tail-r41-sources.ts')).toContain('export const talentSourceSubqueryExists');
    expect(src('native_talent_read_sql.rs')).toContain("'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment");
  });

  test('lowers the talent owner SELECT KPI from BEFORE=13 to 12', () => {
    const files: string[] = [];
    const walk = (d: string) => {
      for (const e of readdirSync(d, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(d, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(resolve(__dirname, '../../../modules/talent/server/src'));
    for (const re of [/SELECT/g, /\bSELECT\b/g]) {
      let count = 0;
      for (const f of files) count += (readFileSync(f, 'utf8').match(re) ?? []).length;
      expect(count).toBe(10);
      expect(count).toBeLessThan(13);
    }
  });
});
