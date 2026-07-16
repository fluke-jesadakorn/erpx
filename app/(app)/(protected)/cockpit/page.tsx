import 'server-only';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { TodaysBrief } from '@/components/cockpit/TodaysBrief';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

export default async function CockpitPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  const locale = await getSecondaryLocale();

  const perms = actor.permissions ?? [];
  const canView = perms.some((p) => p === 'tile:cockpit:view::allow' || p === 'admin:system:bypass::allow');

  if (!canView) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={[
            { label: 'Folio', href: '/', icon: 'home' },
            { label: <T id="nav.cockpit" locale={locale} /> },
          ]}
        />
        <PageLayout
          title={<T id="cockpit.title" locale={locale} />}
          subtitle={<T id="cockpit.subtitle" locale={locale} />}
          category={{ label: <T id="cockpit.title" locale={locale} />, icon: 'gauge', href: '/cockpit' }}
        >
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
          { label: <T id="nav.cockpit" locale={locale} /> },
        ]}
      />
      <PageLayout
        title={<T id="cockpit.title" locale={locale} />}
        subtitle={`${actor.fullname} · ${actor.role_name}`}
        category={{ label: <T id="cockpit.title" locale={locale} />, icon: 'gauge', href: '/cockpit' }}
      >
        <TodaysBrief actor={actor as any} />
      </PageLayout>
    </>
  );
}