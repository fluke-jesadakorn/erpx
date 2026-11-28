import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  socialPurchaseActorFeature,
  socialPostByIdPredicate,
  messagingMemberIds,
} from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');

describe('Collaboration read-tail r16 parameterized families', () => {
  test('socialPurchaseActorFeature stays byte-identical to the four pre-dedupe literals', () => {
    const collab = "social_purchase_actor.company_id','collaboration";
    const ops = "social_purchase_actor.company_id','operations";
    expect(socialPurchaseActorFeature(`scheduledFeaturePredicate('${collab}')`)).toBe(
      "SELECT 1 FROM social_purchase_actor WHERE scheduledFeaturePredicate('social_purchase_actor.company_id','collaboration')",
    );
    expect(socialPurchaseActorFeature(`scheduledFeaturePredicate('${ops}')`)).toBe(
      "SELECT 1 FROM social_purchase_actor WHERE scheduledFeaturePredicate('social_purchase_actor.company_id','operations')",
    );
    expect(socialPurchaseActorFeature("scheduledFeaturePredicate('company_id','collaboration')")).toBe(
      "SELECT 1 FROM social_purchase_actor WHERE scheduledFeaturePredicate('company_id','collaboration')",
    );
    // the EXISTS wrapper caller keeps its original form
    expect(`EXISTS(${socialPurchaseActorFeature(`scheduledFeaturePredicate('${collab}')`)})`).toBe(
      "EXISTS(SELECT 1 FROM social_purchase_actor WHERE scheduledFeaturePredicate('social_purchase_actor.company_id','collaboration'))",
    );
  });

  test('socialPostByIdPredicate stays byte-identical to both pre-dedupe literals', () => {
    expect(socialPostByIdPredicate("operationalHandoverReceiptPredicate('p')")).toBe(
      "SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND operationalHandoverReceiptPredicate('p')",
    );
    expect(socialPostByIdPredicate("sourceAccessSql('p','?::text')")).toBe(
      "SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND sourceAccessSql('p','?::text')",
    );
  });

  test('messagingMemberIds stays byte-identical to both pre-dedupe literals', () => {
    expect(messagingMemberIds('messaging_team_members', 'team_id')).toBe(
      'SELECT user_id FROM messaging_team_members WHERE team_id=?',
    );
    expect(messagingMemberIds('messaging_room_members', 'room_id')).toBe(
      'SELECT user_id FROM messaging_room_members WHERE room_id=?',
    );
  });

  test('each family is defined once in the shared file', () => {
    expect(shared.match(/export const socialPurchaseActorFeature=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialPostByIdPredicate=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const messagingMemberIds=/g) ?? []).toHaveLength(1);
  });

  test('inline family literals are gone and callers use the helpers', () => {
    for (const [file, helper, calls] of [
      ['work-social-read-policy.ts', 'socialPurchaseActorFeature', 2],
      ['work-social-draft-policy.ts', 'socialPurchaseActorFeature', 1],
      ['work-social-purchases.ts', 'socialPurchaseActorFeature', 1],
      ['work-social-draft-policy.ts', 'socialPostByIdPredicate', 1],
      ['work-social-purchases.ts', 'socialPostByIdPredicate', 1],
      ['messaging.ts', 'messagingMemberIds', 2],
    ] as const) {
      const text = readFileSync(resolve(adapters, file), 'utf8');
      expect(text.split(`${helper}(`).length - 1).toBe(calls);
    }
    const draft = readFileSync(resolve(adapters, 'work-social-draft-policy.ts'), 'utf8');
    const purchases = readFileSync(resolve(adapters, 'work-social-purchases.ts'), 'utf8');
    const readPolicy = readFileSync(resolve(adapters, 'work-social-read-policy.ts'), 'utf8');
    const messaging = readFileSync(resolve(adapters, 'messaging.ts'), 'utf8');
    expect(draft).not.toContain('${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE');
    expect(purchases).not.toContain('${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE');
    expect(readPolicy).not.toContain("EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('social_purchase_actor.company_id','collaboration')})");
    expect(readPolicy).not.toContain("${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('social_purchase_actor.company_id','operations')}");
    expect(draft).not.toContain('SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND');
    expect(purchases).not.toContain('SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND');
    expect(messaging).not.toContain('SELECT user_id FROM ${');
  });
});

describe('Collaboration read-tail r16 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r16 BEFORE=424 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(424);
  });
});
