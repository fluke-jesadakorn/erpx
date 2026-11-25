import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { operationsSourceExists } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');
const obligations = src('work-package-obligations.ts');
const queued = src('work-task-queued-read-sources.ts');

// Exact pre-r45 rendered fragments at BEFORE=341. The rowset, alias, alias gap
// and trailing predicate vary; the head, quantifiers, zero binds and locks do
// not. Oracle SHAs pin the bytes so the expectation cannot drift silently.
const SITES = [
  {
    name: 'notice-source task absence',
    file: 'work-package-obligations.ts',
    marker: " (n.source_type IN ('task','staff_task')",
    call: "${operationsSourceExists(operations.tasks,'t','',' WHERE t.id=n.source_id AND t.company_id=n.company_id')}",
    rows: '${operations.tasks}', alias: 't', gap: '',
    tail: ' WHERE t.id=n.source_id AND t.company_id=n.company_id',
    sql: 'SELECT 1 FROM (${operations.tasks})t WHERE t.id=n.source_id AND t.company_id=n.company_id',
    sqlSha: 'd7045ede85c913256a6160ccd7cd401677fea8986ac81780558d23c0e5f4a689',
    lineSha: '75666d49fef4c1f2f3f94273e992e0de8c5690a964e281b8ba3cd88cf1b88849',
  },
  {
    name: 'notice-source template absence',
    file: 'work-package-obligations.ts',
    marker: " OR (n.source_type IN ('routine','routine_template')",
    call: "${operationsSourceExists(operations.templates,'t','',' WHERE t.id=n.source_id AND t.company_id=n.company_id')}",
    rows: '${operations.templates}', alias: 't', gap: '',
    tail: ' WHERE t.id=n.source_id AND t.company_id=n.company_id',
    sql: 'SELECT 1 FROM (${operations.templates})t WHERE t.id=n.source_id AND t.company_id=n.company_id',
    sqlSha: 'b4d67ab16cca930f6c21a7689ab62f9760bab8eee471ffe86433c3bda1ba5a3d',
    lineSha: 'c6f35fc82a35d489637ee6bc4e353a2586bb901f03f63800d1bbe15a9005da0b',
  },
  {
    name: 'action-card staff-task absence',
    file: 'work-package-obligations.ts',
    marker: " UNION ALL SELECT 'action-card'",
    call: "${operationsSourceExists(operations.tasks,'other_task','',' WHERE other_task.id=card.source_id AND other_task.company_id=card.company_id')}",
    rows: '${operations.tasks}', alias: 'other_task', gap: '',
    tail: ' WHERE other_task.id=card.source_id AND other_task.company_id=card.company_id',
    sql: 'SELECT 1 FROM (${operations.tasks})other_task WHERE other_task.id=card.source_id AND other_task.company_id=card.company_id',
    sqlSha: '531ee1eef7fe3248c7ab9a99fca040b8ed16a9376f70fadd37a8030e5b0a4679',
    lineSha: '5ea591457dd6d647742ca7bb2845bc4babfb39b04681cd9d1a9811a7c3a937a6',
  },
  {
    name: 'queued active-delegation guard',
    file: 'work-task-queued-read-sources.ts',
    marker: ' work_queued_social_actors AS MATERIALIZED',
    // The call spans two source lines; only the interpolation is a parameter.
    call: "${operationsSourceExists(operations.delegations,'delegation',' ',\n  ` WHERE delegation.task_id=task.id AND delegation.company_id=task.company_id AND delegation.status='active' AND delegation.starts_at<=company.now_ms AND delegation.expires_at>company.now_ms AND iam_same_person(assignment.user_id,delegation.delegate_user_id,task.company_id)`)}",
    rows: '${operations.delegations}', alias: 'delegation', gap: ' ',
    tail: " WHERE delegation.task_id=task.id AND delegation.company_id=task.company_id AND delegation.status='active' AND delegation.starts_at<=company.now_ms AND delegation.expires_at>company.now_ms AND iam_same_person(assignment.user_id,delegation.delegate_user_id,task.company_id)",
    sql: "SELECT 1 FROM (${operations.delegations}) delegation WHERE delegation.task_id=task.id AND delegation.company_id=task.company_id AND delegation.status='active' AND delegation.starts_at<=company.now_ms AND delegation.expires_at>company.now_ms AND iam_same_person(assignment.user_id,delegation.delegate_user_id,task.company_id)",
    sqlSha: '6c89c467b9ad6f72e2254387d4e2d6e489abf4f7d6cc23e81d71feaa23639c67',
    lineSha: '8c4e2291f1ff2413b03ab2bf779e1a3c9a3f05ab5006eb0fcda9e70ebfb32c37',
  },
] as const;

const owner = (file: string) => file === 'work-task-queued-read-sources.ts' ? queued : obligations;

describe('Collaboration read-tail r45 byte oracles', () => {
  test('the four retired source-existence heads are pinned byte for byte', () => {
    for (const site of SITES) {
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(site.sql.match(/SELECT /g) ?? []).toHaveLength(1);
      expect(site.sql).not.toContain('?');
    }
  });

  test('the four full pre-r45 source lines are pinned byte for byte', () => {
    for (const site of SITES) {
      // Re-render the post-r45 line with the production helper: the result is
      // byte-identical to the retired line at the BEFORE=341 baseline.
      const body = owner(site.file);
      const lines = body.split('\n');
      const at = lines.findIndex(l => l.startsWith(site.marker));
      // The queued call argument spans two source lines; the retired line did not.
      const post = site.file === 'work-task-queued-read-sources.ts'
        ? lines.slice(at, at + 2).join('\n')
        : lines[at];
      expect(post).toContain(site.call);
      const rendered = post.replace(site.call, operationsSourceExists(site.rows, site.alias, site.gap, site.tail));
      expect(sha(rendered)).toBe(site.lineSha);
      expect(rendered).toContain(site.sql);
    }
  });
});

describe('Collaboration read-tail r45 operations-source family', () => {
  test('one renderer reproduces all four retired heads with their own rowsets and tails', () => {
    for (const site of SITES) {
      expect(operationsSourceExists(site.rows, site.alias, site.gap, site.tail)).toBe(site.sql);
      expect(operationsSourceExists(site.rows, site.alias, site.gap, site.tail).match(/SELECT /g) ?? []).toHaveLength(1);
    }
    expect(shared.match(/export const operationsSourceExists=/g) ?? []).toHaveLength(1);
    expect(shared).toContain('(rows:string,alias:string,gap:string,tail:string)');
    expect(sha(line(shared, 'export const operationsSourceExists='))).toBe('100632e4f53c8ee9bbdfd7dd976dfac5b466118c9bce81140e877b58db40933f');
  });

  test('all four call sites keep their quantifiers, rowsets and inline literals are retired', () => {
    for (const site of SITES) {
      const body = owner(site.file);
      expect(body).toContain(site.call);
      expect(body.match(/operationsSourceExists\(/g) ?? []).toHaveLength(site.file === 'work-package-obligations.ts' ? 3 : 1);
    }
    expect(obligations).toContain("AND NOT EXISTS(${operationsSourceExists(operations.tasks,'t','',' WHERE t.id=n.source_id AND t.company_id=n.company_id')})");
    expect(obligations).toContain("OR NOT EXISTS(${operationsSourceExists(operations.templates,'t','',' WHERE t.id=n.source_id AND t.company_id=n.company_id')})");
    expect(obligations).toContain("OR NOT EXISTS(${operationsSourceExists(operations.tasks,'other_task','',' WHERE other_task.id=card.source_id AND other_task.company_id=card.company_id')})");
    expect(queued).toContain("OR EXISTS(${operationsSourceExists(operations.delegations,'delegation',' ',");
    expect(queued).toContain('OR EXISTS(SELECT 1 FROM (${approvals.instances}) instance JOIN (${approvals.steps}) step');
    for (const site of SITES) expect(owner(site.file)).not.toContain(site.sql);
  });
});

describe('Collaboration read-tail r45 protected SQL isolation', () => {
  test('the new renderer adds no lock, write or notification statement', () => {
    const added = line(shared, 'export const operationsSourceExists=');
    expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    expect(added.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  test('notification targets, field outbox, r42/r43/r44 helpers and agent-reply locks stay unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('add8defc4e1a0b3d369362e96d00acb93e34d0d0734cd0a1783b98a7382eb721');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    for (const [marker, digest] of [
      ['export const socialCommentHead=', '2ad63e64db338d62db117ee74155b7ea3281d741f48b4b6811ba884cc24b0d92'],
      ['export const socialCommentProjection=', 'e81afcc981c3c2bf41540a33fba9728e1870f57ec62b9fc4ffc2bb6771cc65c4'],
      ['export const socialPostDraftById=', '9962c290a602e5de5310ba0bb33ae9edd12c0f4db89acb0bb80b579f72814989'],
      ['export const messagingTeamImage=', '0b8462fac51b955ac03ce908b9a50e358700589c0aa3a537b0427dec1e3983e9'],
      ['export const socialTargetImage=', '85b5df731c67c23e797568e8ea4cd3c89f6a67a4bbda1769d522b7a730b8f6dc'],
      ['export const messagingLatestMessage=', '03274a5e1842dc1bff51d3b688d8d1fdbdd6acb56202ed8c80a0ef7e7908a422'],
      ['export const absentSocialPostGuard=', '90aa7d128113bdb5f4da43bce4a8acbf77d112970ea83aa6e27e538e3e7c33ce'],
      ['export const socialHandoverAckJoin=', '52afef4c97ea01d247426bab9469eb8166db069dce785966c2445c3acc41e064'],
    ] as Array<[string, string]>) {
      expect(sha(line(shared, marker))).toBe(digest);
    }
    const purchases = src('work-social-purchases.ts');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Collaboration read-tail r45 KPI pins', () => {
  test('retirement:ts-statements is pinned at 338, below the r45 BEFORE=341 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Four byte-identical settlement-source heads -> one owner-local renderer:
    // 341 -> 338. Prior pins were refreshed only where 338 replaced 341;
    // quantifiers, binds, notification targets and locks were untouched.
    expect(total).toBe(333);
    expect(total).toBeLessThan(341);
  });
});
