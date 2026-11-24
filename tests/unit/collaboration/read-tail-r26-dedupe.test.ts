import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialAgentReplyStateGuard, messagingOwnerGuard } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const agent = readFileSync(resolve(adapters, 'social-agent.ts'), 'utf8');
const messaging = readFileSync(resolve(adapters, 'messaging.ts'), 'utf8');

// Byte-identical Social agent-reply state guards that appeared at two read
// sites in social-agent.ts: the failed-reply and current-lease assertions.
// Only the trailing state predicate varies, so it is parameterized; the fixed
// `id=?` prefix, bind order and lock semantics stay in place.
const FAILED = "EXISTS(SELECT 1 FROM social_agent_replies WHERE id=? AND state='failed')";
const RUNNING = "EXISTS(SELECT 1 FROM social_agent_replies WHERE id=? AND state='running' AND lease_token=? AND lease_until>?)";

// Byte-identical messaging owner/version existence guards that appeared at two
// read sites in messaging.ts: the team-owner and owner-version assertions.
// Only the table and trailing version predicate vary; the fixed
// `id=? AND company_id=? AND owner_id=?` prefix, bind order and lock semantics stay.
const TEAM_OWNER = 'EXISTS(SELECT 1 FROM messaging_teams WHERE id=? AND company_id=? AND owner_id=?)';
const TEAM_VERSION = 'EXISTS(SELECT 1 FROM messaging_teams WHERE id=? AND company_id=? AND owner_id=? AND version=?)';
const ROOM_VERSION = 'EXISTS(SELECT 1 FROM messaging_rooms WHERE id=? AND company_id=? AND owner_id=? AND version=?)';

describe('Collaboration read-tail r26 parameterized families', () => {
  test('socialAgentReplyStateGuard stays byte-identical to both pre-dedupe literals', () => {
    expect(socialAgentReplyStateGuard("state='failed'")).toBe(FAILED);
    expect(socialAgentReplyStateGuard("state='running' AND lease_token=? AND lease_until>?")).toBe(RUNNING);
    for (const sql of [FAILED, RUNNING]) expect(sql.split('SELECT ').length - 1).toBe(1);
  });

  test('messagingOwnerGuard stays byte-identical to all pre-dedupe literals', () => {
    expect(messagingOwnerGuard('messaging_teams', '')).toBe(TEAM_OWNER);
    expect(messagingOwnerGuard('messaging_teams', ' AND version=?')).toBe(TEAM_VERSION);
    expect(messagingOwnerGuard('messaging_rooms', ' AND version=?')).toBe(ROOM_VERSION);
    for (const sql of [TEAM_OWNER, TEAM_VERSION, ROOM_VERSION]) expect(sql.split('SELECT ').length - 1).toBe(1);
  });

  test('shared fragments are declared once, inline literals are gone, call sites remain', () => {
    expect(shared.match(/export const socialAgentReplyStateGuard=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const messagingOwnerGuard=/g) ?? []).toHaveLength(1);
    expect(agent).not.toContain("EXISTS(SELECT 1 FROM social_agent_replies WHERE id=? AND state='");
    expect(messaging).not.toContain('EXISTS(SELECT 1 FROM messaging_teams WHERE id=? AND company_id=? AND owner_id=?');
    expect(agent).toContain('socialAgentReplyStateGuard("state=\'failed\'")');
    expect(agent).toContain('socialAgentReplyStateGuard("state=\'running\' AND lease_token=? AND lease_until>?")');
    expect(messaging).toContain('messagingOwnerGuard("messaging_teams", "")');
    expect(messaging).toContain('messagingOwnerGuard(isTeam ? "messaging_teams" : "messaging_rooms", " AND version=?")');
    // one import + one call per former inline reader
    expect(agent.split('socialAgentReplyStateGuard').length - 1).toBe(3);
    expect(messaging.split('messagingOwnerGuard').length - 1).toBe(3);
  });

  test('keeps the emitted SQL read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    for (const sql of [FAILED, RUNNING, TEAM_OWNER, TEAM_VERSION, ROOM_VERSION]) expect(sql).not.toMatch(lockRe);
    for (const [text, helper] of [[agent, 'socialAgentReplyStateGuard('], [messaging, 'messagingOwnerGuard(']] as const) {
      for (const line of text.split('\n').filter(l => l.includes(helper) && !l.includes('import'))) {
        expect(line).not.toMatch(lockRe);
      }
    }
  });

  test('bind order at all four former inline call sites is unchanged', () => {
    expect(agent).toContain('socialAgentReplyStateGuard("state=\'failed\'"), [id]');
    expect(agent).toContain('socialAgentReplyStateGuard("state=\'running\' AND lease_token=? AND lease_until>?"), [id, lease_token, finish]');
    expect(messaging).toContain('messagingOwnerGuard("messaging_teams", ""), [teamId, companyId, userId]');
    expect(messaging).toContain('messagingOwnerGuard(isTeam ? "messaging_teams" : "messaging_rooms", " AND version=?"), [id, companyId, userId, expectedVersion]');
  });
});

describe('Collaboration read-tail r26 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=405 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r26 deduped two byte-identical families: the Social agent-reply state
    // guards (social-agent.ts) and the messaging owner/version guards
    // (messaging.ts), each into one shared parameterized fragment in
    // read-tail-sql.ts.
    // r27 deduped a further byte-identical family below this r26 baseline.
    expect(total).toBeLessThanOrEqual(403);
    expect(total).toBeLessThan(405);
  });
});
