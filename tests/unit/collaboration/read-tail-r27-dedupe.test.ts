import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialAiPhaseImageSql } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const purchases = readFileSync(resolve(adapters, 'work-social-purchases.ts'), 'utf8');

// Byte-identical AI-phase image projections that appeared at two read sites in
// work-social-purchases.ts: the raw `aiRows` and redacted `identity` readers.
// Only the three per-field `-ARRAY[...]` redaction suffixes vary; the shared
// `jsonb_build_object(...)` SELECT heads, bind order and lock semantics stay.
const RAW = "SELECT jsonb_build_object('draft',(SELECT to_jsonb(d) FROM social_ai_drafts d WHERE d.id=?),'usage',(SELECT to_jsonb(u) FROM social_ai_usage u WHERE u.id=?),'reply',(SELECT to_jsonb(r) FROM social_agent_replies r WHERE r.id=?))::text image";
const REDACTED = "SELECT jsonb_build_object('draft',(SELECT to_jsonb(d)-ARRAY['state','result_json'] FROM social_ai_drafts d WHERE d.id=?),'usage',(SELECT to_jsonb(u)-ARRAY['state','input_tokens','output_tokens','lease_until'] FROM social_ai_usage u WHERE u.id=?),'reply',(SELECT to_jsonb(r)-ARRAY['state','error','error_code','lease_until','updated_at'] FROM social_agent_replies r WHERE r.id=?))::text image";
const DRAFT = "-ARRAY['state','result_json']";
const USAGE = "-ARRAY['state','input_tokens','output_tokens','lease_until']";
const REPLY = "-ARRAY['state','error','error_code','lease_until','updated_at']";

describe('Collaboration read-tail r27 parameterized AI-phase image family', () => {
  test('socialAiPhaseImageSql stays byte-identical to both pre-dedupe literals', () => {
    expect(socialAiPhaseImageSql('', '', '')).toBe(RAW);
    expect(socialAiPhaseImageSql(DRAFT, USAGE, REPLY)).toBe(REDACTED);
    for (const sql of [RAW, REDACTED]) expect(sql.split('SELECT ').length - 1).toBe(4);
  });

  test('shared fragment is declared once, inline literals are gone, call sites remain', () => {
    expect(shared.match(/export const socialAiPhaseImageSql=/g) ?? []).toHaveLength(1);
    expect(purchases).not.toContain("jsonb_build_object('draft'");
    expect(purchases).toContain("socialAiPhaseImageSql('', '', '')");
    expect(purchases).toContain(`socialAiPhaseImageSql("${DRAFT}", "${USAGE}", "${REPLY}")`);
    // one import + two call sites
    expect(purchases.split('socialAiPhaseImageSql').length - 1).toBe(3);
  });

  test('bind order at both former inline call sites is unchanged', () => {
    expect(purchases).toContain("socialAiPhaseImageSql('', '', '')).bind(phase.reservationId,phase.reservationId,phase.reply?.id??null)");
    expect(purchases).toContain(`)).bind(phase.reservationId,phase.reservationId,phase.reply?.id??null)`);
  });

  test('stays read-only and leaves LOCK TABLE / FOR UPDATE semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    expect(RAW).not.toMatch(lockRe);
    expect(REDACTED).not.toMatch(lockRe);
    expect(socialAiPhaseImageSql(DRAFT, USAGE, REPLY)).not.toMatch(lockRe);
    // the pre-existing lock statements in this file are byte-for-byte intact
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
  });
});

describe('Collaboration read-tail r27 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=403 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r27 deduped one new byte-identical family: the AI-phase image projections
    // (raw `aiRows` and redacted `identity`) in work-social-purchases.ts, each
    // into one shared parameterized fragment in read-tail-sql.ts. Net -4 SELECT
    // tokens; no lock statement or notification helper was modified.
    // r28 deduped a further byte-identical family below this r27 baseline.
    expect(total).toBeLessThanOrEqual(399);
    expect(total).toBeLessThan(403);
  });
});
