'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { headers } from 'next/headers';
import { query, withTransaction } from '@/db';
import { loadActivePermSession } from '@/perm/server';
import { authorize } from '@folio-lib/perm/authorize';
import { AccessError, setUserAccess } from '@folio-lib/perm/access';

async function session() {
  const h = await headers();
  const req = new Request('http://internal/policy', { headers: h as unknown as HeadersInit });
  return loadActivePermSession(req);
}

function actorOf(out: NonNullable<Awaited<ReturnType<typeof session>>>) {
  return {
    id: out.session.user.id,
    permissions: out.session.permissions,
    deptId: out.session.user.department,
    departmentId: out.session.user.department,
    level: out.session.user.rank ?? undefined,
    roleName: out.session.user.role,
  };
}

async function allowed(permission: string) {
  const out = await session();
  if (!out) return null;
  const decision = await authorize(actorOf(out), { kind: 'perm', perm: permission });
  return decision.allow ? out : null;
}

function validId(id: string): boolean {
  return /^[a-z][a-z0-9_-]{1,40}$/.test(id);
}

export async function createRoleAction(formData: FormData) {
  const out = await allowed('rbac:role:edit::allow');
  if (!out) redirect('/policy?error=forbidden');
  const id = String(formData.get('name') ?? '').trim().toLowerCase();
  const displayName = String(formData.get('display_name') ?? '').trim();
  const kind = formData.get('kind') === 'system' ? 'system' : 'hierarchy';
  const rank = kind === 'hierarchy' ? Number(formData.get('rank') ?? 7) : null;
  if (!validId(id) || !displayName || (kind === 'hierarchy' && (!Number.isInteger(rank) || Number(rank) < 1 || Number(rank) > 7))) {
    redirect('/policy?error=invalid_role');
  }
  try {
    await withTransaction(async (q) => {
      await q(
        `INSERT INTO perm.roles (id, display_name, description, kind, rank, is_system, sort_order)
         VALUES ($1, $2, 'Custom role', $3, $4, false, 1000)`,
        [id, displayName, kind, rank],
      );
      await q(
        `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.create', $1, $2)`,
        [`user:${out.session.user.id}`, { after: { id, displayName, kind, rank } }],
      );
    });
  } catch {
    redirect('/policy?error=duplicate_role');
  }
  revalidatePath('/policy');
  redirect('/policy?ok=role_created');
}

export async function createDepartmentAction(formData: FormData) {
  const out = await allowed('rbac:department:edit::allow');
  if (!out) redirect('/policy?error=forbidden');
  const id = String(formData.get('id') ?? '').trim().toLowerCase();
  const displayName = String(formData.get('label') ?? '').trim();
  if (!validId(id) || !displayName) redirect('/policy?error=invalid_department');
  try {
    await withTransaction(async (q) => {
      await q(
        `INSERT INTO perm.departments (id, display_name, is_system) VALUES ($1, $2, false)`,
        [id, displayName],
      );
      await q(
        `INSERT INTO perm.permissions (id, description) VALUES ($1, $2) ON CONFLICT DO NOTHING`,
        [`user:dept:${id}::allow`, `${displayName} department membership marker`],
      );
      await q(
        `INSERT INTO perm.audit (kind, actor, target) VALUES ('department.create', $1, $2)`,
        [`user:${out.session.user.id}`, { after: { id, displayName } }],
      );
    });
  } catch {
    redirect('/policy?error=duplicate_department');
  }
  revalidatePath('/policy');
  redirect('/policy?ok=department_created');
}

export async function deleteRoleAction(formData: FormData) {
  const out = await allowed('rbac:role:edit::allow');
  if (!out) redirect('/policy?error=forbidden');
  const id = String(formData.get('id') ?? '');
  const role = await query<{ is_system: boolean; display_name: string }>(
    `SELECT is_system, display_name FROM perm.roles WHERE id = $1`,
    [id],
  );
  if (!role.rows[0]) redirect('/policy?error=not_found');
  if (role.rows[0].is_system) redirect('/policy?error=system_role_protected');
  const members = await query<{ count: number }>(
    `SELECT count(*)::int AS count FROM perm.user_roles WHERE role_id = $1`,
    [id],
  );
  if ((members.rows[0]?.count ?? 0) > 0) redirect('/policy?error=role_has_members');
  await withTransaction(async (q) => {
    await q(`DELETE FROM perm.roles WHERE id = $1`, [id]);
    await q(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.delete', $1, $2)`,
      [`user:${out.session.user.id}`, { before: { id, ...role.rows[0] } }],
    );
  });
  revalidatePath('/policy');
  redirect('/policy?ok=role_deleted&members=0');
}

export async function deleteDepartmentAction(formData: FormData) {
  const out = await allowed('rbac:department:edit::allow');
  if (!out) redirect('/policy?error=forbidden');
  const id = String(formData.get('id') ?? '').trim().toLowerCase();
  const dept = await query<{ display_name: string; is_system: boolean }>(
    `SELECT display_name, is_system FROM perm.departments WHERE id = $1`,
    [id],
  );
  if (!dept.rows[0]) redirect('/policy?error=not_found');
  if (dept.rows[0].is_system) redirect('/policy?error=system_role_protected');
  const members = await query<{ count: number }>(
    `SELECT count(*)::int AS count FROM perm.user_departments WHERE department_id = $1`,
    [id],
  );
  if ((members.rows[0]?.count ?? 0) > 0) redirect('/policy?error=department_has_members');
  await withTransaction(async (q) => {
    await q(`DELETE FROM perm.departments WHERE id = $1`, [id]);
    await q(`DELETE FROM perm.permissions WHERE id = $1`, [`user:dept:${id}::allow`]);
    await q(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('department.delete', $1, $2)`,
      [`user:${out.session.user.id}`, { before: { id, ...dept.rows[0] } }],
    );
  });
  revalidatePath('/policy');
  redirect('/policy?ok=department_deleted&members=0');
}

export async function assignUserAction(formData: FormData) {
  const out = await session();
  if (!out) redirect('/login');
  const userId = Number(formData.get('user_id'));
  const departmentId = String(formData.get('department_id') ?? '');
  const hierarchyRoleId = String(formData.get('hierarchy_role_id') ?? '');
  const systemRoleId = String(formData.get('system_role_id') ?? '');
  if (!Number.isInteger(userId) || !departmentId || !hierarchyRoleId) {
    redirect('/policy?error=invalid_user');
  }
  try {
    await setUserAccess(actorOf(out), userId, {
      departmentId,
      hierarchyRoleId,
      systemRoleIds: systemRoleId ? [systemRoleId] : [],
    });
  } catch (error) {
    if (error instanceof AccessError && error.message.includes('own access')) {
      redirect('/policy?error=own_access_blocked');
    }
    redirect('/policy?error=forbidden');
  }
  revalidatePath('/policy');
  redirect(`/policy?ok=user_assigned&user=${userId}`);
}
