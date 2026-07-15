import 'server-only';
import { redirect } from 'next/navigation';
import { loadActor } from '@folio-lib/server/guard';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { TodaysBrief } from '@/components/cockpit/TodaysBrief';

export const dynamic = 'force-dynamic';

export default async function CockpitPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const perms = actor.permissions ?? [];
  const canView = perms.some((p) => p === 'tile:cockpit:view::allow' || p === 'admin:system:bypass::allow');

  if (!canView) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={[
            { label: 'Folio', href: '/', icon: 'home' },
            { label: 'Cockpit' },
          ]}
        />
        <PageLayout title="Cockpit" subtitle="Executive dashboard" category={{ label: 'Cockpit', icon: 'gauge', href: '/cockpit' }}>
          <NoPermissionView
            kind="locked"
            actor={actor as any}
            attemptedPath="/cockpit"
            reason="tile:cockpit:view required."
          />
        </PageLayout>
      </>
    );
  }

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: 'Folio', href: '/', icon: 'home' },
          { label: 'Cockpit' },
        ]}
      />
      <PageLayout
        title="Cockpit"
        subtitle={`${actor.fullname} · ${actor.role_name}`}
        category={{ label: 'Cockpit', icon: 'gauge', href: '/cockpit' }}
      >
        <TodaysBrief actor={actor as any} />
      </PageLayout>
    </>
  );
}