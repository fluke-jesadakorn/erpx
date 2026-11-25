import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { absentSocialPostGuard, messagingLatestMessage, socialHandoverAckJoin } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');
const messaging = src('messaging.ts');
const obligations = src('work-package-obligations.ts');
const receipts = src('handover-receipts.ts');
const purchases = src('work-social-purchases.ts');

// Exact pre-r44 SQL fragments at the BEFORE=344 HEAD. Tails, closed column
// references, the COALESCE/EXISTS wrappers, bind order, locks and notification
// targets were never changed. The oracle SHAs pin the bytes so expectations
// cannot drift silently.
const PREVIEW = "(SELECT CASE WHEN msg.body='' THEN 'Attachment' ELSE left(msg.body,160) END FROM messaging_messages msg WHERE msg.room_id=r.id ORDER BY msg.seq DESC LIMIT 1)";
const UPDATED = '(SELECT msg.created_at FROM messaging_messages msg WHERE msg.room_id=r.id ORDER BY msg.seq DESC LIMIT 1)';
const ABSENT_NOTICE = 'NOT EXISTS(SELECT 1 FROM social_posts p WHERE p.id=n.source_id AND p.company_id=n.company_id)';
const ABSENT_REPLY = 'NOT EXISTS(SELECT 1 FROM social_posts p WHERE p.id=reply.post_id AND p.company_id=reply.company_id)';
const ACK_ACKNOWLEDGED = 'EXISTS(SELECT 1 FROM social_handover_acknowledgements ack JOIN social_post_participants recipient ON recipient.post_id=ack.post_id AND recipient.user_id=ack.user_id WHERE ack.post_id=p.id AND ack.post_version=p.version AND ack.user_id=? AND recipient.user_id<>p.author_id)';
const ACK_COUNT = '(SELECT count(*)::int FROM social_handover_acknowledgements ack JOIN social_post_participants recipient ON recipient.post_id=ack.post_id AND recipient.user_id=ack.user_id WHERE ack.post_id=p.id AND ack.post_version=p.version AND recipient.user_id<>p.author_id)';

// Full pre-r44 source lines, including their original indentation and wrappers.
const PREVIEW_LINE = `    ${PREVIEW} preview,`;
const UPDATED_LINE = `    COALESCE(${UPDATED},r.created_at) updated_at`;
const ABSENT_NOTICE_LINE = ` OR (n.source_type IN ('post','social_post') AND scope.package_id IN ('work','work-social','work-routines') AND (EXISTS(SELECT 1 FROM posts p WHERE p.id=n.source_id) OR scope.package_id IN ('work','work-social') AND ${ABSENT_NOTICE}))`;
const ABSENT_REPLY_LINE = ` UNION ALL SELECT 'social-agent-reply',reply.id,reply.attempt,reply.state,to_jsonb(reply),reply.state IN ('pending','running') FROM social_agent_replies reply CROSS JOIN scope WHERE reply.company_id=scope.company_id AND (EXISTS(SELECT 1 FROM posts p WHERE p.id=reply.post_id) OR scope.package_id IN ('work','work-social') AND ${ABSENT_REPLY})`;
const ACK_ACK_LINE = ` ${ACK_ACKNOWLEDGED} acknowledged,`;
const ACK_COUNT_LINE = ` ${ACK_COUNT} "acknowledgedCount"`;

// Re-resolve each call site's own interpolation with the production renderer,
// so the byte oracle covers the rendered SQL rather than only the helper output.
const render = (value: string) => {
  const substitutions: Array<[string, string]> = [
    [`\${messagingLatestMessage("CASE WHEN msg.body='' THEN 'Attachment' ELSE left(msg.body,160) END")}`, PREVIEW],
    ["\${messagingLatestMessage('msg.created_at')}", UPDATED],
    ["\${absentSocialPostGuard('n.source_id','n.company_id')}", ABSENT_NOTICE],
    ["\${absentSocialPostGuard('reply.post_id','reply.company_id')}", ABSENT_REPLY],
    ["\${socialHandoverAckJoin('1',' AND ack.user_id=? AND recipient.user_id<>p.author_id')}", socialHandoverAckJoin('1', ' AND ack.user_id=? AND recipient.user_id<>p.author_id')],
    ["\${socialHandoverAckJoin('count(*)::int',' AND recipient.user_id<>p.author_id')}", socialHandoverAckJoin('count(*)::int', ' AND recipient.user_id<>p.author_id')],
  ];
  for (const [from, to] of substitutions) value = value.split(from).join(to);
  return value;
};

describe('Collaboration read-tail r44 byte oracles', () => {
  test('the six retired SQL fragments are pinned byte for byte', () => {
    for (const [value, digest] of [
      [PREVIEW, 'f8ac0a05f233a06ab7950d1d04635b40973455fd278ecd5bb03db2ff1a488377'],
      [UPDATED, '719b0ef108912fb520937dbe002eb133fe865b64cf290173eecb5258d92c13ff'],
      [ABSENT_NOTICE, 'c5f032d28fff301692352fcc55452a92f6efde9b06ff90cceb6df6b518a0be97'],
      [ABSENT_REPLY, 'ed8546ff7f754d359f90000c780633e338989c6c594a1a7d88a389c282f8dc93'],
      [ACK_ACKNOWLEDGED, '6eedc1b069206fd113a595f6ecd60acdd880d259ce125425ae4c758dd6863662'],
      [ACK_COUNT, '41bafd4fea65b2ec3b1709694dfca1c2980f9bea5a8f3301c84bda77f7bfcfb2'],
    ] as Array<[string, string]>) {
      expect(sha(value)).toBe(digest);
      expect(value.match(/SELECT /g)).toHaveLength(1);
    }
  });

  test('the six full pre-r44 source lines are pinned byte for byte', () => {
    for (const [value, digest] of [
      [PREVIEW_LINE, '5636b2fbc20275982fd365010d5bf41b0c9fa7019a94ac0bef984708a8e2504f'],
      [UPDATED_LINE, '41bccaac1a6a42c9285a862909e78f6220130236806eff1f309c15c7964d891a'],
      [ABSENT_NOTICE_LINE, '33ae7d4f47094a1323faeeec5f5edcb30875dccd812724f9de7bc5bdf6522209'],
      [ABSENT_REPLY_LINE, '10129e2ef83ec5541e24315c2400295f3894a63c1c88ff8012d7ee8452e543fd'],
      [ACK_ACK_LINE, '3b5437f7a06be685eaa843bea3f87f3430dac0ebe4d17c6e6f93fd9aaf45326c'],
      [ACK_COUNT_LINE, '83dd6eab0bd67e36e11ba2bf311c86df53d95933f3e7b6bfb6c64127401d2078'],
    ] as Array<[string, string]>) {
      expect(sha(value)).toBe(digest);
    }
  });
});

describe('Collaboration read-tail r44 latest-message family', () => {
  test('one parameterized tail reproduces both previews with the COALESCE wrapper left local', () => {
    expect(messagingLatestMessage("CASE WHEN msg.body='' THEN 'Attachment' ELSE left(msg.body,160) END")).toBe(PREVIEW);
    expect(messagingLatestMessage('msg.created_at')).toBe(UPDATED);
    expect(`COALESCE(${messagingLatestMessage('msg.created_at')},r.created_at) updated_at`).toBe(UPDATED_LINE.slice(4));
    expect(UPDATED.match(/SELECT /g)).toHaveLength(1);
    expect(shared.match(/export const messagingLatestMessage=/g) ?? []).toHaveLength(1);
    expect(shared).toContain('projection:string');
    expect(sha(line(shared, 'export const messagingLatestMessage='))).toBe('03274a5e1842dc1bff51d3b688d8d1fdbdd6acb56202ed8c80a0ef7e7908a422');
  });

  test('the overview reader keeps both projections, bind order and ORDER BY tail', () => {
    expect(render(line(messaging, '    ${messagingLatestMessage('))).toBe(PREVIEW_LINE);
    expect(render(line(messaging, '    COALESCE(${messagingLatestMessage('))).toBe(UPDATED_LINE);
    expect(messaging.match(/messagingLatestMessage\(/g) ?? []).toHaveLength(2);
    expect(sha(line(messaging, '    ${messagingLatestMessage('))).toBe('5c805a21f1d03105d39cac7bf6e4479a9e9551ef9c5d2b0c15022ff00a1cd4fa');
    expect(sha(line(messaging, '    COALESCE(${messagingLatestMessage('))).toBe('ac3172ed69db7134e92865fd7280f0de0981e75f5b5e4caebd076f52d165f524');
    expect(messaging).toContain('.bind(userId, userId2, companyId, ...value)');
    expect(messaging).toContain('ORDER BY updated_at DESC,r.id');
    expect(messaging).not.toContain("(SELECT CASE WHEN msg.body='' THEN 'Attachment'");
    expect(messaging).not.toContain('(SELECT msg.created_at FROM messaging_messages msg');
  });
});

describe('Collaboration read-tail r44 absent-social-post guard family', () => {
  test('one closed-reference renderer reproduces both guards with zero binds', () => {
    expect(absentSocialPostGuard('n.source_id', 'n.company_id')).toBe(ABSENT_NOTICE);
    expect(absentSocialPostGuard('reply.post_id', 'reply.company_id')).toBe(ABSENT_REPLY);
    for (const guard of [ABSENT_NOTICE, ABSENT_REPLY]) {
      expect(guard.match(/SELECT /g)).toHaveLength(1);
      expect(guard).not.toContain('?');
    }
    expect(shared.match(/export const absentSocialPostGuard=/g) ?? []).toHaveLength(1);
    expect(sha(line(shared, 'export const absentSocialPostGuard='))).toBe('90aa7d128113bdb5f4da43bce4a8acbf77d112970ea83aa6e27e538e3e7c33ce');
  });

  test('both obligation arms keep their closed refs and existing post predicate', () => {
    expect(render(line(obligations, ` OR (n.source_type IN ('post','social_post')`))).toBe(ABSENT_NOTICE_LINE);
    expect(render(line(obligations, ` UNION ALL SELECT 'social-agent-reply'`))).toBe(ABSENT_REPLY_LINE);
    expect(obligations.match(/absentSocialPostGuard\(/g) ?? []).toHaveLength(2);
    expect(sha(line(obligations, ` OR (n.source_type IN ('post','social_post')`))).toBe('abe9c6e31a7ba254909ff898a66d23930bfd5c83f46627fa689df566b2064def');
    expect(sha(line(obligations, ` UNION ALL SELECT 'social-agent-reply'`))).toBe('eba6d83ef2fd29c73c6cc9bf77fa35273666c955cb879679328cc03037dcda33');
    expect(obligations).toContain("EXISTS(SELECT 1 FROM posts p WHERE p.id=n.source_id)");
    expect(obligations).toContain("EXISTS(SELECT 1 FROM posts p WHERE p.id=reply.post_id)");
    expect(obligations).not.toContain('NOT EXISTS(SELECT 1 FROM social_posts p WHERE p.id=n.source_id');
    expect(obligations).not.toContain('NOT EXISTS(SELECT 1 FROM social_posts p WHERE p.id=reply.post_id');
  });
});

describe('Collaboration read-tail r44 handover acknowledgement join family', () => {
  test('one join-head renderer reproduces the EXISTS and count variants', () => {
    expect(`EXISTS(${socialHandoverAckJoin('1', ' AND ack.user_id=? AND recipient.user_id<>p.author_id')})`).toBe(ACK_ACKNOWLEDGED);
    expect(`(${socialHandoverAckJoin('count(*)::int', ' AND recipient.user_id<>p.author_id')})`).toBe(ACK_COUNT);
    expect(ACK_ACKNOWLEDGED.split('?').length - 1).toBe(1);
    expect(ACK_COUNT).not.toContain('?');
    expect(shared.match(/export const socialHandoverAckJoin=/g) ?? []).toHaveLength(1);
    expect(sha(line(shared, 'export const socialHandoverAckJoin='))).toBe('52afef4c97ea01d247426bab9469eb8166db069dce785966c2445c3acc41e064');
  });

  test('both receipt call sites keep their wrappers, bind and bind order', () => {
    expect(render(line(receipts, ' EXISTS('))).toBe(ACK_ACK_LINE);
    expect(render(line(receipts, ' (${socialHandoverAckJoin('))).toBe(ACK_COUNT_LINE);
    expect(receipts.match(/socialHandoverAckJoin\(/g) ?? []).toHaveLength(2);
    expect(sha(line(receipts, ' EXISTS('))).toBe('fdf07b5e78e178598c98a8693a6407bc60b1851efb9b9e0df8a75ff2756c6642');
    expect(sha(line(receipts, ' (${socialHandoverAckJoin('))).toBe('f64c83287ceb1333a1ea9fffd7168418691f46ec727c39c5b0a7b72bc4a4e791');
    expect(receipts).toContain('.bind(...scope.args,actor.userId,actor.userId,JSON.stringify(ids),actor.companyId,actor.userId,now,now,actor.assignmentId)');
    expect(receipts).toContain('${socialPostRecipientExists(\'recipient.user_id=?\')} "canAcknowledge"');
    expect(receipts).not.toContain('EXISTS(SELECT 1 FROM social_handover_acknowledgements ack JOIN');
    expect(receipts).not.toContain('(SELECT count(*)::int FROM social_handover_acknowledgements ack JOIN');
  });
});

describe('Collaboration read-tail r44 protected SQL isolation', () => {
  test('new renderers add no lock, write or notification statement', () => {
    for (const marker of [
      'export const messagingLatestMessage=',
      'export const absentSocialPostGuard=',
      'export const socialHandoverAckJoin=',
    ]) {
      const added = line(shared, marker);
      expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
    }
  });

  test('agent-reply locks, notification targets and r42/r43 helpers stay unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    expect(sha(line(shared, 'export const socialCommentHead='))).toBe('2ad63e64db338d62db117ee74155b7ea3281d741f48b4b6811ba884cc24b0d92');
    expect(sha(line(shared, 'export const socialCommentProjection='))).toBe('e81afcc981c3c2bf41540a33fba9728e1870f57ec62b9fc4ffc2bb6771cc65c4');
    expect(sha(line(shared, 'export const socialPostDraftById='))).toBe('9962c290a602e5de5310ba0bb33ae9edd12c0f4db89acb0bb80b579f72814989');
    expect(sha(line(shared, 'export const messagingTeamImage='))).toBe('0b8462fac51b955ac03ce908b9a50e358700589c0aa3a537b0427dec1e3983e9');
    expect(sha(line(shared, 'export const socialTargetImage='))).toBe('85b5df731c67c23e797568e8ea4cd3c89f6a67a4bbda1769d522b7a730b8f6dc');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Collaboration read-tail r44 KPI pins', () => {
  test('retirement:ts-statements is pinned at 297, below the r44 BEFORE=344 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Latest-message tail 2->1 (-1), absent-social-post guard 2->1 (-1) and
    // handover ack join 2->1 (-1): 344 -> 341. Prior exact pins were refreshed
    // only where 341 replaced 344; locks and notification targets were untouched.
    expect(total).toBe(297);
    expect(total).toBeLessThan(344);
  });
});
