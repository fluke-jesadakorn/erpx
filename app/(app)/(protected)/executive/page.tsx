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

export default async function ExecutivePage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  const locale = await getSecondaryLocale();

  const perms = actor.permissions ?? [];
  const canView =
    perms.includes('tile:executive:view::allow') ||
    perms.includes('finance:report:executive::allow') ||
    perms.includes('admin:system:bypass::allow');

  if (!canView) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={[
            { label: 'Folio', href: '/', icon: 'home' },
            { label: <T id="executive.title" locale={locale} /> },
          ]}
        />
        <PageLayout
          title={<T id="executive.title" locale={locale} />}
          subtitle={<T id="executive.subtitle" locale={locale} />}
          category={{ label: <T id="executive.title" locale={locale} />, icon: 'star', href: '/executive' }}
        >
          <NoPermissionView
            kind="locked"
            actor={actor as any}
            attemptedPath="/executive"
            reason="tile:executive:view required."
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
          { label: <T id="executive.title" locale={locale} /> },
        ]}
      />
      <PageLayout
        title={<T id="executive.title" locale={locale} />}
        subtitle={`${actor.fullname} · ${actor.role_name}`}
        category={{ label: <T id="executive.title" locale={locale} />, icon: 'star', href: '/executive' }}
      >
        <TodaysBrief actor={actor as any} />
      </PageLayout>
    </>
  );
}