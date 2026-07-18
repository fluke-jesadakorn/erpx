import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import {
  createRoleAction,
  createDepartmentAction,
  deleteRoleAction,
  deleteDepartmentAction,
  assignUserAction,
} from '@/app/(app)/(protected)/policy/_actions';
import type { MatrixTarget } from '@/policy/matrixRepo';

interface UserLite {
  id: number;
  fullname: string;
  employee_code: string;
  department: string | null;
  perm_role_ids: string[];
  perm_role_names: string[];
}

interface Props {
  targets: MatrixTarget[];
  users: UserLite[];
  canEdit: boolean;
  canAssign: boolean;
  flash?: { kind: 'ok' | 'err'; code: string; meta?: Record<string, string> } | null;
  view?: 'assignment' | 'roles' | 'departments';
}

function Badge({ children, tone }: { children: React.ReactNode; tone: 'positive' | 'caution' | 'critical' | 'info' | 'neutral' }) {
  const cls: Record<string, string> = {
    positive: 'bg-positive-soft/30 text-positive border-positive/40',
    caution: 'bg-caution-soft/30 text-caution border-caution/40',
    critical: 'bg-critical-soft/30 text-critical border-critical/40',
    info: 'bg-info-soft/30 text-info border-info/40',
    neutral: 'bg-paper-3/40 text-mute border-rule/40',
  };
  return (
    <span className={['inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md border text-[10px] font-bold uppercase tracking-[0.1em] whitespace-nowrap', cls[tone]].join(' ')}>
      {children}
    </span>
  );
}

function flashCopy(code: string, meta?: Record<string, string>): { tone: 'positive' | 'critical' | 'caution'; en: string; th?: string; de?: string } | null {
  switch (code) {
    case 'role_created':           return { tone: 'positive', en: `Role created.` };
    case 'department_created':     return { tone: 'positive', en: `Department created.` };
    case 'role_deleted':           return { tone: 'positive', en: `Role deleted (${meta?.members ?? '0'} member${meta?.members === '1' ? '' : 's'} dropped).` };
    case 'department_deleted':     return { tone: 'positive', en: `Department deleted (${meta?.members ?? '0'} member${meta?.members === '1' ? '' : 's'} detached).` };
    case 'user_assigned':          return { tone: 'positive', en: `User #${meta?.user} assignment saved.` };
    case 'invalid_role':           return { tone: 'critical', en: 'Invalid role (name must be snake/kebab-case; level 1–10).' };
    case 'invalid_department':     return { tone: 'critical', en: 'Invalid department id (snake/kebab-case).' };
    case 'duplicate_role':         return { tone: 'caution',  en: 'A role with that id+level already exists.' };
    case 'duplicate_department':   return { tone: 'caution',  en: 'A department with that id already exists.' };
    case 'system_role_protected':  return { tone: 'critical', en: 'Seed persona roles are protected from deletion.' };
    case 'forbidden':              return { tone: 'critical', en: 'Insufficient permission for this action.' };
    case 'invalid_user':           return { tone: 'critical', en: 'Pick a user first.' };
    case 'user_not_found':         return { tone: 'critical', en: 'User does not exist.' };
    case 'missing_id':             return { tone: 'critical', en: 'Missing id.' };
    case 'not_found':              return { tone: 'caution',  en: 'Target not found (already deleted?).' };
    default:                       return null;
  }
}

export async function PolicyAdmin({ targets, users, canEdit, canAssign, flash, view = 'assignment' }: Props) {
  const locale = await getSecondaryLocale();
  const flashMsg = flash ? flashCopy(flash.code, flash.meta) : null;
  const roles = targets.filter((t) => t.kind === 'role');
  const departments = targets.filter((t) => t.kind === 'department');
  const sortedRoles = [...roles].sort((a, b) => Number(a.is_seed_persona) - Number(b.is_seed_persona) || a.id.localeCompare(b.id));
  const sortedDepts = [...departments].sort((a, b) => a.id.localeCompare(b.id));
  const sortedUsers = [...users].sort((a, b) => a.fullname.localeCompare(b.fullname));

  return (
    <section className="rounded-md border border-rule/40 bg-paper-2/30 p-4 md:p-5 space-y-5">
      <header className="flex items-center justify-between gap-3 flex-wrap">
        <div>
          <h2 className="text-[15px] font-bold text-ink">
            <T id="policy.admin.title" locale={locale} hideSecondary />
          </h2>
          <p className="text-[12px] text-mute mt-0.5">
            <T id="policy.admin.subtitle" locale={locale} />
          </p>
        </div>
        {flashMsg ? (
          <div className={[
            'flex items-center gap-2 px-3 py-2 rounded-lg border text-[12px] font-mono',
            flashMsg.tone === 'positive' ? 'border-positive/40 bg-positive-soft/30 text-positive'
            : flashMsg.tone === 'caution'  ? 'border-caution/40 bg-caution-soft/30 text-caution'
            :                                'border-critical/40 bg-critical-soft/30 text-critical',
          ].join(' ')}>
            {flashMsg.en}
          </div>
        ) : null}
      </header>

      {view === 'assignment' ? (
        <div className="max-w-2xl">
          <AssignUserForm users={sortedUsers} departments={sortedDepts} roles={sortedRoles} canAssign={canAssign} />
        </div>
      ) : view === 'roles' ? (
        <div className="grid grid-cols-1 xl:grid-cols-[minmax(280px,420px)_1fr] gap-4">
          <CreateRoleForm canEdit={canEdit} />
          <TargetList
            title="Roles"
            empty="No roles yet."
            canDelete={canEdit}
            rows={sortedRoles.map((r) => ({
              id: r.id,
              label: r.label,
              meta: `${r.role_kind ?? 'role'}${r.rank ? ` · rank ${r.rank}` : ''} · ${r.member_count} members`,
              kind: 'role',
              protected: r.is_seed_persona || r.is_system,
              action: deleteRoleAction,
            }))}
          />
        </div>
      ) : (
        <div className="grid grid-cols-1 xl:grid-cols-[minmax(280px,420px)_1fr] gap-4">
          <CreateDepartmentForm canEdit={canEdit} />
          <TargetList
            title="Departments"
            empty="No departments yet."
            canDelete={canEdit}
            rows={sortedDepts.map((d) => ({
              id: d.id,
              label: d.label,
              meta: `${d.member_count} members`,
              kind: 'department',
              protected: d.is_system,
              action: deleteDepartmentAction,
            }))}
          />
        </div>
      )}

    </section>
  );
}

function FieldShell({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[10px] font-mono uppercase tracking-[0.12em] text-mute">{label}</span>
      {children}
      {hint ? <span className="text-[11px] text-mute/80">{hint}</span> : null}
    </label>
  );
}

function inputCls(extra = '') {
  return [
    'w-full px-2.5 py-1.5 rounded-md text-[13px] font-mono bg-paper-1 border border-rule/60 text-ink',
    'focus:outline-none focus:border-accent/60',
    extra,
  ].join(' ');
}

function btnPrimary(disabled = false) {
  return [
    'w-full px-3 py-2 rounded-md text-[12px] font-bold uppercase tracking-[0.1em] border transition-colors',
    disabled
      ? 'border-rule/40 bg-paper-3/40 text-mute/70 cursor-not-allowed'
      : 'border-accent/60 bg-accent-soft/40 text-accent-strong hover:bg-accent-soft/70',
  ].join(' ');
}

function btnDanger(disabled = false) {
  return [
    'px-2 py-1 rounded-md text-[11px] font-bold uppercase tracking-[0.08em] border transition-colors',
    disabled
      ? 'border-rule/40 bg-paper-3/40 text-mute/60 cursor-not-allowed'
      : 'border-critical/50 bg-critical-soft/20 text-critical hover:bg-critical-soft/40',
  ].join(' ');
}

async function CreateRoleForm({ canEdit }: { canEdit: boolean }) {
  const locale = await getSecondaryLocale();
  return (
    <form action={createRoleAction} className="rounded-md border border-rule/40 bg-paper-1/40 p-3 space-y-2.5">
      <h3 className="text-[12px] font-bold uppercase tracking-[0.1em] text-mute">
        <T id="policy.admin.createRole" locale={locale} hideSecondary />
      </h3>
      <FieldShell label="name">
        <input name="name" required pattern="[a-z][a-z0-9_-]{1,40}" placeholder="regional_supervisor" className={inputCls()} />
      </FieldShell>
      <FieldShell label="display name">
        <input name="display_name" required placeholder="Regional supervisor" className={inputCls()} />
      </FieldShell>
      <FieldShell label="kind">
        <select name="kind" defaultValue="hierarchy" className={inputCls()}>
          <option value="hierarchy">Hierarchy</option>
          <option value="system">System</option>
        </select>
      </FieldShell>
      <FieldShell label="rank (1–7)" hint="1 = highest authority; ignored for system roles">
        <input name="rank" type="number" min={1} max={7} defaultValue={7} className={inputCls()} />
      </FieldShell>
      <button type="submit" disabled={!canEdit} className={btnPrimary(!canEdit)}>
        <T id="policy.admin.create" locale={locale} hideSecondary />
      </button>
    </form>
  );
}

async function CreateDepartmentForm({ canEdit }: { canEdit: boolean }) {
  const locale = await getSecondaryLocale();
  return (
    <form action={createDepartmentAction} className="rounded-md border border-rule/40 bg-paper-1/40 p-3 space-y-2.5">
      <h3 className="text-[12px] font-bold uppercase tracking-[0.1em] text-mute">
        <T id="policy.admin.createDepartment" locale={locale} hideSecondary />
      </h3>
      <FieldShell label="id" hint="snake or kebab-case">
        <input name="id" required pattern="[a-z][a-z0-9_-]{1,40}" placeholder="sales" className={inputCls()} />
      </FieldShell>
      <FieldShell label="label">
        <input name="label" required placeholder="Sales" className={inputCls()} />
      </FieldShell>
      <button type="submit" disabled={!canEdit} className={btnPrimary(!canEdit)}>
        <T id="policy.admin.create" locale={locale} hideSecondary />
      </button>
    </form>
  );
}

async function AssignUserForm({
  users,
  departments,
  roles,
  canAssign,
}: {
  users: UserLite[];
  departments: MatrixTarget[];
  roles: MatrixTarget[];
  canAssign: boolean;
}) {
  const locale = await getSecondaryLocale();
  return (
    <form action={assignUserAction} className="rounded-md border border-rule/40 bg-paper-1/40 p-3 space-y-2.5">
      <h3 className="text-[12px] font-bold uppercase tracking-[0.1em] text-mute">
        <T id="policy.admin.assignUser" locale={locale} hideSecondary />
      </h3>
      <FieldShell label="user">
        <select name="user_id" required className={inputCls()}>
          <option value="">— pick user —</option>
          {users.map((u) => (
            <option key={u.id} value={u.id}>
              {u.fullname} · {u.employee_code}{u.department ? ` · ${u.department}` : ''}
            </option>
          ))}
        </select>
      </FieldShell>
      <FieldShell label="department" hint="Exactly one department per configured user">
        <select name="department_id" required className={inputCls()} defaultValue="">
          <option value="">— pick department —</option>
          {departments.map((d) => (
            <option key={d.id} value={d.id}>{d.label} · {d.id}</option>
          ))}
        </select>
      </FieldShell>
      <FieldShell label="hierarchy role">
        <select name="hierarchy_role_id" required className={inputCls()} defaultValue="">
          <option value="">— pick hierarchy role —</option>
          {roles.filter((r) => r.role_kind === 'hierarchy').map((r) => (
            <option key={r.id} value={r.id}>{r.label} · {r.id}</option>
          ))}
        </select>
      </FieldShell>
      <FieldShell label="system role" hint="Optional; one maximum">
        <select name="system_role_id" className={inputCls()} defaultValue="">
          <option value="">— none —</option>
          {roles.filter((r) => r.role_kind === 'system').map((r) => (
            <option key={r.id} value={r.id}>{r.label} · {r.id}</option>
          ))}
        </select>
      </FieldShell>
      <button type="submit" disabled={!canAssign} className={btnPrimary(!canAssign)}>
        <T id="policy.admin.saveAssignment" locale={locale} hideSecondary />
      </button>
    </form>
  );
}

interface TargetRow {
  id: string;
  label: string;
  meta: string;
  kind: 'role' | 'department';
  protected: boolean;
  action: (formData: FormData) => Promise<void>;
}

function TargetList({
  title,
  rows,
  empty,
  canDelete,
}: {
  title: string;
  rows: TargetRow[];
  empty: string;
  canDelete: boolean;
}) {
  return (
    <div className="rounded-md border border-rule/40 bg-paper-1/40 p-3">
      <header className="flex items-center justify-between mb-2">
        <h3 className="text-[12px] font-bold uppercase tracking-[0.1em] text-mute">{title}</h3>
        <Badge tone="neutral">{rows.length}</Badge>
      </header>
      <div className="divide-y divide-rule/30 max-h-[40vh] overflow-auto">
        {rows.length === 0 ? (
          <div className="px-2 py-4 text-center text-[12px] text-mute">{empty}</div>
        ) : rows.map((r) => (
          <div key={r.id} className="flex items-center justify-between gap-2 px-2 py-1.5">
            <div className="flex min-w-0 items-center gap-2">
              <div className="min-w-0 flex flex-col">
                <span className="text-[13px] font-semibold text-ink truncate">{r.label}</span>
                <span className="text-[11px] font-mono text-mute truncate">{r.id} · {r.meta}</span>
              </div>
              {r.protected ? <Badge tone="caution">seed</Badge> : null}
            </div>
            <form action={r.action}>
              <input type="hidden" name="id" value={r.id} />
              <button
                type="submit"
                disabled={!canDelete || r.protected}
                title={r.protected ? 'Protected (seed persona)' : 'Delete'}
                className={btnDanger(!canDelete || r.protected)}
              >
                <T id="policy.admin.delete" hideSecondary />
              </button>
            </form>
          </div>
        ))}
      </div>
    </div>
  );
}
