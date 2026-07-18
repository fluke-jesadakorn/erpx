import 'server-only';
import { headers } from 'next/headers';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { TodaysBrief } from '@/components/cockpit/TodaysBrief';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

export default async function ExecutivePage() {
  const h = await headers();
  const session = await loadActivePermSession(
    new Request('http://internal/executive', { headers: h as unknown as HeadersInit }),
  );
  const actor = await loadActor();
  if (!session || !actor) redirect('/login');
  const locale = await getSecondaryLocale();

  const canView = hasPermission(session.session, 'tile:executive:view::allow')
    || hasPermission(session.session, PERM.finance.report.executive);

  if (!canView) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={[
            { label: 'Folio', href: '/', icon: 'Home' },
            { label: <T id="executive.title" locale={locale} /> },
          ]}
        />
        <PageLayout
          title={<T id="executive.title" locale={locale} />}
          subtitle={<T id="executive.subtitle" locale={locale} />}
          category={{ label: <T id="executive.title" locale={locale} />, icon: 'Star', href: '/executive' }}
          width="wide"
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
          { label: 'Folio', href: '/', icon: 'Home' },
          { label: <T id="executive.title" locale={locale} /> },
        ]}
      />
      <PageLayout
        title={<T id="executive.title" locale={locale} />}
        subtitle={`${actor.fullname} · ${actor.role_name}`}
        category={{ label: <T id="executive.title" locale={locale} />, icon: 'Star', href: '/executive' }}
        width="wide"
      >
        <TodaysBrief actor={actor as any} />
      </PageLayout>
    </>
  );
}
