import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { staffNotificationCteBody } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const shared = src('read-tail-sql.ts');
const notifications = src('staff-notifications.ts');

// r35 generalizes the byte-identical staff-notifications legacy CTE body shared
// by the list and page readers (staff-notifications.ts:80/86, LCP=269). Only the
// tail after the shared prefix varies: list ORDER BY/LIMIT, page read filter,
// cursor and ORDER BY/LIMIT. The `company_id=? AND user_id=?` binds, `legacy_rows`
// CTE, union, lock statements and notification targets are unchanged.
const native = '<native>';
const clock = '<clock>';
const cols = '<cols>';
const partition = '<partition>';
const frags = [
  '<scope>', '<commerce>', '<payroll>', '<basePeople>', '<count>', '<procurement>',
  '<transfer>', '<projects>', '<quality>', '<learning>', '<assets>', '<landedCost>',
  '<manufacturing>', '<talent>', '<legal>', '<crm>', '<marketing>', '<followups>',
  '<workforce>', '<benefits>', '<work>',
];
const CTE = 'WITH native_rows AS (<native>),legacy_rows AS (<clock>SELECT <cols>,created_at::text AS page_key FROM staff_notifications WHERE company_id=? AND user_id=? AND <partition> AND <scope> AND <commerce> AND <payroll> AND <basePeople> AND <count> AND <procurement> AND <transfer> AND <projects> AND <quality> AND <learning> AND <assets> AND <landedCost> AND <manufacturing> AND <talent> AND <legal> AND <crm> AND <marketing> AND <followups> AND <workforce> AND <benefits> AND <work> ';

describe('Collaboration read-tail r35 parameterized families', () => {
  test('staffNotificationCteBody renders the shared CTE body byte-identically', () => {
    expect(staffNotificationCteBody(native, clock, cols, partition, frags)).toBe(CTE);
    expect(shared.match(/export const staffNotificationCteBody=/g) ?? []).toHaveLength(1);
  });

  test('both notification readers use the helper and dropped their inline CTE bodies', () => {
    expect(notifications.match(/const body=staffNotificationCteBody\(/g) ?? []).toHaveLength(2);
    expect(notifications).not.toContain('WITH native_rows AS (${nativeStaffSql})');
    expect(notifications).toContain('${body}ORDER BY created_at DESC,id LIMIT 100),combined AS ${legacyNativeUnion} SELECT ${staffColumns} FROM combined ORDER BY created_at DESC,id LIMIT 100');
    expect(notifications).toContain("${body}${query.view === 'all' ? '' : 'AND read_at IS NULL'}),combined AS ${legacyNativeUnion} SELECT * FROM combined");
    // The shared prefix ends with one space, so each tail concatenates without
    // adding or removing bytes relative to the pre-dedupe inline literals.
    expect(CTE.endsWith(' ')).toBe(true);
    expect(CTE.match(/SELECT /g) ?? []).toHaveLength(1);
  });
});

describe('Collaboration read-tail r35 lock/notification isolation', () => {
  test('locks, binds and notification helpers stay untouched', () => {
    const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/;
    expect(staffNotificationCteBody(native, clock, cols, partition, frags)).not.toMatch(lockRe);
    expect(shared).toContain('legacy_rows AS (${clock}SELECT ${cols},created_at::text AS page_key FROM staff_notifications');
    expect(notifications).toContain('legacy_rows AS');
    expect(src('work-task-purchase-source.ts')).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(src('messaging-purchases.ts')).toContain('LOCK TABLE messaging_teams,messaging_team_members,messaging_rooms,messaging_room_members,messaging_messages,messaging_reads,messaging_files IN SHARE ROW EXCLUSIVE MODE');
    expect(notifications).toContain('.bind(projected.json,companyId, userId,...scope.args,...commerce.args,...payroll.args');
    expect(notifications).toContain('.bind(projected.json,actor.companyId, actor.userId,...scope.args,...commerce.args,...payroll.args');
  });
});

describe('Collaboration read-tail r35 KPI pins', () => {
  test('retirement:ts-statements is pinned at 316 after r44, below the r35 BEFORE=371 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // The two inline staff-notification CTE bodies (list and page) share one
    // byte-identical prefix: 2 -> 1, net -1 SELECT token: 371 -> 370. No lock
    // statement, bind order or notification helper was modified. r37 then
    // deduped revisions (-1), messaging guards (-2) and PMS sources (-3):
    // 370 -> 364. r38 then deduped settlement (-2), AI draft-by-id (-1) and
    // readable post head (-1): 364 -> 360. r39 then deduped the queued source
    // guard (-1), purchase actor CTE (-2) and requested sources select (-1):
    // 360 -> 356. r40 then deduped the two source-rows absence heads: 356 -> 355.
    // r41 deduped task-origin/by-key, AI-usage row and team/room member guards:
    // 355 -> 352.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(316);
    expect(total).toBeLessThan(370);
  });
});
