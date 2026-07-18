import 'server-only';
import { headers } from 'next/headers';
import { redirect } from 'next/navigation';
import { hasPermission, loadActivePermSession } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { DesignSystemShowcase } from './DesignSystemShowcase';

export const dynamic = 'force-dynamic';

export default async function DesignSystemPage() {
  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal/design-system', { headers: h as unknown as HeadersInit }),
  );
  if (!out) redirect('/login');

  const allowed = hasPermission(out.session, 'admin:system:bypass::allow');

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Design system' }]} />
      <PageLayout
        title="Executive Crystal"
        subtitle="Tokens, glass elevations, interaction states, and accessibility patterns used across Folio."
        category={{ label: 'Admin system', icon: 'Settings', href: '/design-system' }}
        width="wide"
      >
        {allowed ? (
          <DesignSystemShowcase />
        ) : (
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/design-system"
            reason="admin:system:bypass required."
          />
        )}
      </PageLayout>
    </>
  );
}
