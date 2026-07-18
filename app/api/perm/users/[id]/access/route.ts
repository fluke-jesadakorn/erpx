import { NextResponse } from 'next/server';
import { loadActivePermSession } from '@/perm/server';
import { AccessError, setUserAccess } from '@folio-lib/perm/access';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function PUT(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });

  const { id } = await ctx.params;
  const userId = Number(id);
  if (!Number.isInteger(userId) || userId < 1) {
    return NextResponse.json({ error: 'invalid user id' }, { status: 400 });
  }

  const body = await req.json().catch(() => null) as {
    departmentId?: unknown;
    hierarchyRoleId?: unknown;
    systemRoleIds?: unknown;
  } | null;
  if (
    !body ||
    typeof body.departmentId !== 'string' ||
    typeof body.hierarchyRoleId !== 'string' ||
    !Array.isArray(body.systemRoleIds) ||
    !body.systemRoleIds.every((id) => typeof id === 'string')
  ) {
    return NextResponse.json({ error: 'invalid access payload' }, { status: 400 });
  }

  try {
    const result = await setUserAccess(
      {
        id: out.session.user.id,
        permissions: out.session.permissions,
        deptId: out.session.user.department,
        departmentId: out.session.user.department,
        level: out.session.user.rank ?? undefined,
        roleName: out.session.user.role,
      },
      userId,
      {
        departmentId: body.departmentId,
        hierarchyRoleId: body.hierarchyRoleId,
        systemRoleIds: body.systemRoleIds as string[],
      },
    );
    return NextResponse.json({ ok: true, access: result.after, before: result.before });
  } catch (error) {
    const status = error instanceof AccessError ? error.status : 500;
    const message = error instanceof Error ? error.message : 'access update failed';
    return NextResponse.json({ error: message }, { status });
  }
}
