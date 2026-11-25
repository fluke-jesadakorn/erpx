import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  versionAppointments: 'SELECT version FROM calendar_appointments WHERE id=? AND company_id=?',
  versionAnnouncements: 'SELECT version FROM calendar_announcements WHERE id=? AND company_id=?',
  actorScope: (where: string) =>
    `SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${where}`,
  roleAllows: (permission: string) =>
    `SELECT iam_role_allows(?,?,'${permission}') allowed`,
} as const;

describe('work_projection read-tail r10 dedupe', () => {
  it('keeps one canonical versionById builder and no inline version points', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.match(/SELECT version FROM \$\{table\} WHERE id=\? AND company_id=\?/g)).toBeNull();
    expect(shared).toContain("appointmentProbe('version', '', table)");
    expect(await readFile(src('calendar/reminder-authority.ts'), 'utf8')).not.toContain(original.versionAppointments);
    expect(await readFile(src('calendar/purchases.ts'), 'utf8')).not.toContain('SELECT version FROM ${kind(input)');
  });

  it('keeps one canonical actor-scope and role-allows builder in purchases', async () => {
    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain("derivedProbeSql(scopeRowSql('company_id,?::text branch_id,?::text department_id'),'workforce_actor_scope',where)");
    expect(purchases.match(/SELECT iam_role_allows\(\?,\?,'\$\{permission\}'\) allowed/g)).toHaveLength(1);
    expect(purchases).toContain('actorScopeSql(scope.where)');
    expect(purchases).toContain('actorScopeSql(placement.where)');
    expect(purchases).toContain("roleAllowsSql('calendar.announcements.publish')");
    expect(purchases).toContain("roleAllowsSql('calendar.appointments.manage')");
  });

  it('emits the original byte-identical SQL for every parameterized caller', async () => {
    const { versionById, derivedProbeSql, scopeRowSql } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    expect(versionById('calendar_appointments')).toBe(original.versionAppointments);
    expect(versionById('calendar_announcements')).toBe(original.versionAnnouncements);
    expect(derivedProbeSql(scopeRowSql('company_id,?::text branch_id,?::text department_id'), 'workforce_actor_scope', 'scope.where')).toBe(
      original.actorScope('scope.where'),
    );
    expect(original.roleAllows('calendar.announcements.publish')).toBe(
      "SELECT iam_role_allows(?,?,'calendar.announcements.publish') allowed",
    );
  });
});
