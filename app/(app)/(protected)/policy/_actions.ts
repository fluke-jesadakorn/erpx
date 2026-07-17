'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { headers } from 'next/headers';
import { query, withTransaction } from '@/db';
import {
  loadActivePermSession, hasPermission, PERM, buildPerm, buildRoleId,
} from '@/perm/server';

type AuthResult = { ok: true; actorId: number } | { ok: false; redirect: string };

async function authz(required: 'edit' | 'assign'): Promise<AuthResult> {
  const h = await headers();
  const req = new Request('http://internal/policy', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  if (!out) return { ok: false, redirect: '/login' };
  const ok = required === 'edit'
    ? hasPermission(out.session, PERM.rbac.matrix.edit)
    : (
      hasPermission(out.session, PERM.rbac.role.assign) ||
      hasPermission(out.session, PERM.user.role.assign) ||
      hasPermission(out.session, PERM.rbac.matrix.edit)
    );
  if (!ok) return { ok: false, redirect: '/policy?error=forbidden' };
  return { ok: true, actorId: (out.session.user as { id: number }).id };
}

function isValidId(id: string): boolean {
  return /^[a-z][a-z0-9_-]{1,40}$/.test(id);
}

export async function createRoleAction(formData: FormData) {
  const auth = await authz('edit');
  if (!auth.ok) return redirect(auth.redirect);

  const name = String(formData.get('name') ?? '').trim().toLowerCase();
  const display_name = String(formData.get('display_name') ?? '').trim();
  const level = Number(formData.get('level') ?? 5);
  if (!isValidId(name) || !display_name || level < 1 || level > 10)
    return redirect('/policy?error=invalid_role');
  const id = buildRoleId(name, Math.floor(level));
  const grantedBy = `user:${auth.actorId}`;
  try {
    await withTransaction(async (q) => {
      const dup = await q<{ id: string }>(`SELECT id FROM perm.roles WHERE id = $1`, [id]);
      if (dup.rows.length > 0) throw new Error('duplicate');
      await q(
        `INSERT INTO perm.roles (id, display_name, description, is_system, sort_order)
         VALUES ($1, $2, $3, false, 1000)`,
        [id, display_name, 'Custom role'],
      );
    });
  } catch {
    return redirect('/policy?error=duplicate_role');
  }
  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.create', $1, $2)`,
    [grantedBy, { role_id: id, display_name, level, source: 'policy_admin' }],
  );
  revalidatePath('/policy');
  return redirect('/policy?ok=role_created');
}

export async function createDepartmentAction(formData: FormData) {
  const auth = await authz('edit');
  if (!auth.ok) return redirect(auth.redirect);

  const id = String(formData.get('id') ?? '').trim().toLowerCase();
  const label = String(formData.get('label') ?? '').trim();
  if (!isValidId(id) || !label)
    return redirect('/policy?error=invalid_department');

  const permId = buildPerm({ domain: 'user', subject: 'dept', verb: id });
  const grantedBy = `user:${auth.actorId}`;
  try {
    await withTransaction(async (q) => {
      const dup = await q<{ id: string }>(`SELECT id FROM perm.permissions WHERE id = $1`, [permId]);
      if (dup.rows.length > 0) throw new Error('duplicate');
      await q(
        `INSERT INTO perm.permissions (id, description) VALUES ($1, $2)`,
        [permId, `Department membership: ${label}`],
      );
      await q(
        `INSERT INTO perm.roles (id, display_name, description, is_system, sort_order)
         VALUES ($1, $2, $3, true, 50)
         ON CONFLICT (id) DO NOTHING`,
        [id, label, 'Department target'],
      );
    });
  } catch {
    return redirect('/policy?error=duplicate_department');
  }
  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('policy.target.create', $1, $2)`,
    [grantedBy, { kind: 'department', id, label }],
  );
  revalidatePath('/policy');
  return redirect('/policy?ok=department_created');
}

export async function deleteRoleAction(formData: FormData) {
  const auth = await authz('edit');
  if (!auth.ok) return redirect(auth.redirect);

  const id = String(formData.get('id') ?? '');
  if (!id) return redirect('/policy?error=missing_id');

  const cur = await query<{ is_system: boolean; display_name: string }>(
    `SELECT is_system, display_name FROM perm.roles WHERE id = $1`,
    [id],
  );
  if (cur.rows.length === 0) return redirect('/policy?error=not_found');
  if (cur.rows[0].is_system) return redirect('/policy?error=system_role_protected');

  const result = await withTransaction(async (q) => {
    const members = await q<{ user_id: number }>(
      `DELETE FROM perm.user_roles WHERE role_id = $1 RETURNING user_id`,
      [id],
    );
    await q(`DELETE FROM perm.department_permissions WHERE department_id = $1`, [id]);
    await q(`DELETE FROM perm.roles WHERE id = $1`, [id]);
    return { cascaded_user_ids: members.rows.map((r) => r.user_id) };
  });

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.delete', $1, $2)`,
    [`user:${auth.actorId}`, { role_id: id, display_name: cur.rows[0].display_name, cascaded_user_ids: result.cascaded_user_ids, source: 'policy_admin' }],
  );
  revalidatePath('/policy');
  return redirect(`/policy?ok=role_deleted&members=${result.cascaded_user_ids.length}`);
}

export async function deleteDepartmentAction(formData: FormData) {
  const auth = await authz('edit');
  if (!auth.ok) return redirect(auth.redirect);

  const id = String(formData.get('id') ?? '').trim().toLowerCase();
  if (!isValidId(id)) return redirect('/policy?error=invalid_department');
  const permId = buildPerm({ domain: 'user', subject: 'dept', verb: id });

  const summary = await withTransaction(async (q) => {
    const affected = await q<{ user_id: number }>(
      `SELECT user_id FROM perm.user_permissions
        WHERE permission_id = $1 AND revoked_at IS NULL`,
      [permId],
    );
    await q(
      `UPDATE perm.user_permissions
          SET revoked_at = now(), revoked_by = $2
        WHERE permission_id = $1 AND revoked_at IS NULL`,
      [permId, `user:${auth.actorId}`],
    );
    await q(`DELETE FROM perm.permissions WHERE id = $1`, [permId]);
    await q(`DELETE FROM perm.department_permissions WHERE department_id IN ($1, $2)`, [id, `dept-${id}`]);
    await q(`DELETE FROM perm.roles WHERE id IN ($1, $2)`, [id, `dept-${id}`]);
    return { affected_user_ids: affected.rows.map((r) => r.user_id) };
  });

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('department.delete', $1, $2)`,
    [`user:${auth.actorId}`, { dept_id: id, ...summary, source: 'policy_admin' }],
  );
  revalidatePath('/policy');
  return redirect(`/policy?ok=department_deleted&members=${summary.affected_user_ids.length}`);
}

export async function assignUserAction(formData: FormData) {
  const auth = await authz('assign');
  if (!auth.ok) return redirect(auth.redirect);

  const userId = Number(formData.get('user_id'));
  if (!userId) return redirect('/policy?error=invalid_user');

  const deptRaw = String(formData.get('dept') ?? '').trim();
  const dept = deptRaw === '' || deptRaw === '__none__' ? null : deptRaw.toLowerCase();
  if (dept && !isValidId(dept)) return redirect('/policy?error=invalid_department');

  const roles = formData.getAll('roles').map((v) => String(v)).filter(Boolean);

  const grantedBy = `user:${auth.actorId}`;

  const userCheck = await query<{ id: number }>(`SELECT id FROM users WHERE id = $1`, [userId]);
  if (userCheck.rows.length === 0) return redirect('/policy?error=user_not_found');

  await withTransaction(async (q) => {
    await q(
      `UPDATE perm.user_permissions
          SET revoked_at = now(), revoked_by = $2
        WHERE user_id = $1
          AND permission_id LIKE 'user:dept:%::allow'
          AND revoked_at IS NULL`,
      [userId, grantedBy],
    );
    if (dept) {
      const permId = buildPerm({ domain: 'user', subject: 'dept', verb: dept });
      await q(
        `INSERT INTO perm.user_permissions (user_id, permission_id, granted_by)
         VALUES ($1, $2, $3)`,
        [userId, permId, grantedBy],
      );
    }

    if (roles.length === 0) {
      await q(`DELETE FROM perm.user_roles WHERE user_id = $1`, [userId]);
    } else {
      const existing = await q<{ role_id: string }>(
        `SELECT role_id FROM perm.user_roles WHERE user_id = $1`,
        [userId],
      );
      const before = new Set(existing.rows.map((r) => r.role_id));
      const after = new Set(roles);
      const removed = [...before].filter((r) => !after.has(r));
      const added = [...after].filter((r) => !before.has(r));
      if (removed.length) {
        await q(
          `DELETE FROM perm.user_roles WHERE user_id = $1 AND role_id = ANY($2::text[])`,
          [userId, removed],
        );
      }
      for (const r of added) {
        await q(
          `INSERT INTO perm.user_roles (user_id, role_id, granted_by) VALUES ($1, $2, $3)
           ON CONFLICT DO NOTHING`,
          [userId, r, grantedBy],
        );
      }
    }
  });

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('user.assignment.set', $1, $2)`,
    [grantedBy, { user_id: userId, dept, roles, source: 'policy_admin' }],
  );

  revalidatePath('/policy');
  return redirect(`/policy?ok=user_assigned&user=${userId}`);
}
