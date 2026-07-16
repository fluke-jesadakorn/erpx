// /policy — RBAC policy management surface.
// Renders the stage-chain permission matrix. The chain order is sourced from
// STAGE_ORDER (single source of truth). Edits write via PUT
// /api/perm/roles/[id]/permissions (audit-logged server-side).
//
// Behind perm: rbac:matrix:view (gated server-side via session).

import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import { PolicyMatrixPage } from '@/components/PolicyMatrixPage';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { query } from '@/db';
import { getSecondaryLocale } from '@/server/locale';

interface Persona {
  id: string;
  display_name: string;
  sort_order: number;
  level: number;
  user_count: number;
  grants: string[];
}

interface Stage {
  perm: string;
  stage: string;
}

export const dynamic = 'force-dynamic';

async function loadMatrix(): Promise<{ stages: Stage[]; personas: Persona[] }> {
  const STAGE_PERMS: string[] = [
    PERM.stage.dept_verification.act,
    PERM.stage.dept_authorization.act,
    PERM.stage.accounting_verification.act,
    PERM.stage.accounting_supervision.act,
    PERM.stage.accounting_authorization.act,
    PERM.stage.disbursement_authorization.act,
    PERM.stage.cfo_authorization.act,
    PERM.stage.ceo_authorization.act,
  ];

  const personasRes = await query<{
    id: string;
    display_name: string;
    display_name_th: string | null;
    display_name_de: string | null;
    sort_order: number;
    level: number;
  }>(
    `SELECT id, display_name, display_name_th, display_name_de, sort_order,
            COALESCE(NULLIF(split_part(id, '::', 2), ''), '0')::int AS level
       FROM perm.roles
       ORDER BY sort_order, display_name`,
  );
  const grantsRes = await query<{ role_id: string; permission_id: string }>(
    `SELECT role_id, permission_id
       FROM perm.role_permissions
       WHERE permission_id = ANY($1::text[])`,
    [STAGE_PERMS],
  );
  const usersRes = await query<{ role_id: string; count: string }>(
    `SELECT ur.role_id, COUNT(*)::text AS count
       FROM perm.user_roles ur
       GROUP BY ur.role_id`,
  );

  const grantsByRole: Record<string, Set<string>> = {};
  for (const g of grantsRes.rows) {
    if (!grantsByRole[g.role_id]) grantsByRole[g.role_id] = new Set();
    grantsByRole[g.role_id].add(g.permission_id);
  }
  const userCountByRole: Record<string, number> = {};
  for (const r of usersRes.rows) userCountByRole[r.role_id] = Number(r.count);

  return {
    stages: STAGE_PERMS.map((id) => {
      const [, stage] = id.match(/^stage:([^:]+):/) ?? [];
      return { perm: id, stage: stage ?? id };
    }),
    personas: personasRes.rows.map((p) => ({
      ...p,
      user_count: userCountByRole[p.id] ?? 0,
      grants: Array.from(grantsByRole[p.id] ?? []),
    })),
  };
}

export default async function PolicyPage() {
  const h = await headers();
  const req = new Request('http://internal/policy', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.policy" locale={locale} /> }]} />
        <PageLayout
          title={<T id="policy.title" locale={locale} />}
          subtitle={<T id="policy.subtitle" locale={locale} />}
        >
          <NoPermissionView
            kind="locked"
            actor={null}
            attemptedPath="/policy"
            reason="Sign in to view this page."
          />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.rbac.matrix.view)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.policy" locale={locale} /> }]} />
        <PageLayout
          title={<T id="policy.title" locale={locale} />}
          subtitle={<T id="policy.subtitle" locale={locale} />}
        >
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/policy"
            reason="rbac:matrix:view required."
          />
        </PageLayout>
      </>
    );
  }

  const matrix = await loadMatrix();
  const policiesRes = await query<{ id: string; name: string }>(
    `SELECT id, name FROM perm.policies WHERE enabled = TRUE ORDER BY name ASC, id ASC`,
  );
  const policies = policiesRes.rows.map((p) => ({ id: p.id, name: p.name }));
  const canEdit = hasPermission(out.session, PERM.rbac.matrix.edit);
  const actor = out.session.user;

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.policy" locale={locale} /> }]} />
      <PageLayout
        title={<T id="policy.title" locale={locale} />}
        subtitle={<T id="policy.subtitle" locale={locale} />}
      >
        <PolicyMatrixPage
          stages={matrix.stages}
          personas={matrix.personas}
          canEdit={canEdit}
          actorName={(actor as any)?.fullname ?? (actor as any)?.id?.toString() ?? ''}
          policies={policies}
        />
      </PageLayout>
    </>
  );
}