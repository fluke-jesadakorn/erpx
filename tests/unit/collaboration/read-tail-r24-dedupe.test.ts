import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostByIdGuard } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const purchases = readFileSync(resolve(adapters, 'work-social-purchases.ts'), 'utf8');
const draftPolicy = readFileSync(resolve(adapters, 'work-social-draft-policy.ts'), 'utf8');
// Byte-identical owner-scoped post existence guards that appeared at two read
// sites: the retained handover-publish source witness and the system-post
// capability check. Only the trailing predicate varies, so it is parameterized;
// the fixed prefix, bind order and lock semantics stay in place.
const AUTHOR = 'SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND author_id=?';
const SYSTEM = "SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND kind='system'";
const INLINE = 'SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND ';

describe('Collaboration read-tail r24 parameterized post-by-id guard family', () => {
  test('socialPostByIdGuard stays byte-identical to both pre-dedupe literals', () => {
    expect(socialPostByIdGuard('author_id=?')).toBe(AUTHOR);
    expect(socialPostByIdGuard("kind='system'")).toBe(SYSTEM);
    // The helper contributes exactly one SELECT; the varying tail adds none.
    expect(AUTHOR.split('SELECT ').length - 1).toBe(1);
    expect(socialPostByIdGuard('author_id=?').split('SELECT ').length - 1).toBe(1);
  });

  test('the shared fragment is declared once, inline literals are gone, call sites remain', () => {
    expect(shared.match(/export const socialPostByIdGuard=/g) ?? []).toHaveLength(1);
    expect(purchases).toContain("socialPostByIdGuard('author_id=?')");
    expect(draftPolicy).toContain("socialPostByIdGuard(\"kind='system'\")");
    expect(purchases).not.toContain(INLINE);
    expect(draftPolicy).not.toContain(INLINE);
    // one import + one call per consumer file
    expect(purchases.split('socialPostByIdGuard').length - 1).toBe(2);
    expect(draftPolicy.split('socialPostByIdGuard').length - 1).toBe(2);
  });

  test('keeps the emitted SQL read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    for (const sql of [AUTHOR, SYSTEM]) expect(sql).not.toMatch(lockRe);
    for (const line of [...purchases.split('\n'), ...draftPolicy.split('\n')].filter(l => l.includes('socialPostByIdGuard(') && !l.includes('import'))) {
      expect(line).not.toMatch(lockRe);
    }
  });

  test('bind order at the two former inline call sites is unchanged', () => {
    expect(purchases).toContain('.bind(command.id,actor.companyId,actor.userId)');
    expect(draftPolicy).toContain('.bind(command.id,actor.companyId)');
  });
});

describe('Collaboration read-tail r24 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=407 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r24 deduped the 2x byte-identical post-by-id guard prefix into one shared
    // parameterized fragment in read-tail-sql.ts.
    expect(total).toBe(406);
    expect(total).toBeLessThan(407);
  });
});
