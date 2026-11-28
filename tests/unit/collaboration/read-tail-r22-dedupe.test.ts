import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostPublishReceipt } from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const obligations = readFileSync(resolve(adapters, 'work-package-obligations.ts'), 'utf8');
const purchases = readFileSync(resolve(adapters, 'work-social-purchases.ts'), 'utf8');
// Byte-identical publish-receipt witness that appeared at two read sites:
// work-package-obligations.ts (p.id / p.author_id) and work-social-purchases.ts
// (post.id / draft.author_id). Only the post-id and actor-id column expressions
// vary, so both are parameterized and the binds and lock semantics stay in order.
const EXPECTED = (postId: string, actorId: string) =>
  `SELECT 1 FROM command_receipts receipt WHERE receipt.operation='social.post.publish' AND receipt.record_id=${postId} AND receipt.actor_id=${actorId} AND receipt.result_json::jsonb->>'id'=${postId}`;
const INLINE = `SELECT 1 FROM command_receipts receipt WHERE receipt.operation='social.post.publish' AND receipt.record_id=`;

describe('Collaboration read-tail r22 parameterized publish-receipt witness family', () => {
  test('socialPostPublishReceipt stays byte-identical to both pre-dedupe literals', () => {
    expect(socialPostPublishReceipt('p.id', 'p.author_id')).toBe(EXPECTED('p.id', 'p.author_id'));
    expect(socialPostPublishReceipt('post.id', 'draft.author_id')).toBe(EXPECTED('post.id', 'draft.author_id'));
    // The witness contributes exactly one SELECT; the operation literal is fixed.
    expect(socialPostPublishReceipt('p.id', 'p.author_id').split('SELECT ').length - 1).toBe(1);
    expect(socialPostPublishReceipt('p.id', 'p.author_id').split("'social.post.publish'").length - 1).toBe(1);
  });

  test('the shared fragment is declared once, both inline literals are gone, both call sites remain', () => {
    expect(shared.match(/export const socialPostPublishReceipt=/g) ?? []).toHaveLength(1);
    // one import + one call site in each consumer file
    expect(obligations.split('socialPostPublishReceipt').length - 1).toBe(2);
    expect(purchases.split('socialPostPublishReceipt').length - 1).toBe(2);
    expect(obligations).not.toContain(INLINE);
    expect(purchases).not.toContain(INLINE);
  });

  test('keeps the fragment read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    expect(socialPostPublishReceipt('p.id', 'p.author_id')).not.toMatch(lockRe);
    for (const file of [obligations, purchases]) {
      for (const line of file.split('\n').filter(l => l.includes('socialPostPublishReceipt(') && !l.includes('import'))) {
        expect(line).not.toMatch(lockRe);
      }
    }
  });
});

describe('Collaboration read-tail r22 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r22 BEFORE=410 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r22 deduped the 2x publish-receipt witness family in work-package-obligations.ts
    // and work-social-purchases.ts into one shared parameterized fragment.
    expect(total).toBeLessThan(410);
  });
});
