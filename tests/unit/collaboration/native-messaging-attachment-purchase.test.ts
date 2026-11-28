import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'modules/collaboration/server/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const adapter = src('messaging-attachment-purchases.ts');
const planner = src('notification_statement.rs');

const KINDS = [
  'messaging_attachment_witness_head',
  'messaging_attachment_witness_tail',
  'messaging_attachment_guard_head',
  'messaging_attachment_guard_room',
] as const;

describe('Messaging attachment purchase witness and guard', () => {
  it('all four retired SQL spans are owner-registered and read-only', () => {
    for (const kind of KINDS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line).not.toMatch(/INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  it('adapter composes owner bytes around the registered aggregates and scopes', () => {
    expect(adapter.match(/SELECT /g) ?? []).toHaveLength(0);
    for (const kind of KINDS) expect(adapter).toContain(`'${kind}'`);
    expect(adapter).toContain("const source=async(companyId:string)=>`${await nativeInboxSqlFragment(companyId,'messaging_attachment_witness_head')}${messagingRoomMembersAgg}");
    expect(adapter).toContain("${messagingTeamMembersAgg('r.team_id')}${await nativeInboxSqlFragment(companyId,'messaging_attachment_witness_tail')}`;");
    expect(adapter).toContain('database.prepare(await source(actor.companyId))');
    expect(adapter).toContain("'messaging_attachment_guard_head')}${await source(scope.companyId)}${await nativeInboxSqlFragment(scope.companyId,'messaging_attachment_guard_room')}${messagingRoomAccessSql()})");
    expect(adapter).toContain('[scope.targetId,scope.companyId,scope.witness,...messagingRoomAccessArgs(actor)');
  });

  it('keeps purchase commands, assertion fences and failure mapping untouched', () => {
    expect(adapter).toContain('purchaseCommandBoundary(database,mode,key');
    expect(adapter).toContain('nativeClearCommandAssertions(database, actor.companyId, key)');
    expect(adapter).toContain('MESSAGING_ATTACHMENT_CHANGED');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 237 -> 234 with the purchase witness retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(225);
    expect(total).toBeLessThan(237);
  });
});
