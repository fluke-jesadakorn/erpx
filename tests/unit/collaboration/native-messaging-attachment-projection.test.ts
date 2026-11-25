import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const adapter = src('messaging-attachment-read.ts');
const planner = src('notification_statement.rs');

describe('Private messaging attachment image projection', () => {
  it('owner owns the head and files SQL spans byte for byte', () => {
    expect(planner).toContain('"messaging_attachment_image_head" => "SELECT jsonb_build_object(\'room\',to_jsonb(r),\'members\',"');
    expect(planner).toContain('"messaging_attachment_files" =>');
    expect(planner).toContain(",'file',(SELECT jsonb_agg(jsonb_build_object('file',to_jsonb(f)");
    expect(planner).toContain('FROM messaging_files f WHERE f.room_id=r.id AND f.attachment_link_id=?))::text image FROM messaging_rooms r WHERE r.id=? AND r.company_id=?"');
  });

  it('adapter composes owner bytes around the registered member/team aggregates', () => {
    expect(adapter.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(adapter).toContain('${await nativeInboxSqlFragment(actor.companyId,\'messaging_attachment_image_head\')}${messagingRoomMembersAgg}');
    expect(adapter).toContain(",'team',${messagingTeamImage('r.team_id','r.company_id')},'teamMembers',${messagingTeamMembersAgg('r.team_id')}");
    expect(adapter).toContain('${await nativeInboxSqlFragment(actor.companyId,\'messaging_attachment_files\')}');
    expect(adapter).toContain('.bind(linkId,id,actor.companyId)');
  });

  it('keeps the transaction and purchase gates untouched', () => {
    expect(adapter).toContain('isTransactionalSqlDatabase(owner)');
    expect(adapter).toContain('requireMessagingPurchase(database,actor,\'history\')');
    expect(adapter).toContain('attachmentDeniedReadAudits(database,actor,linkId)');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 240 -> 237 with the attachment image projection retired', () => {
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
    expect(total).toBeLessThan(240);
  });
});
