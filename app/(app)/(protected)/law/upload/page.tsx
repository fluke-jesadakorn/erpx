import Link from 'next/link';
import { redirect } from 'next/navigation';
import { headers } from 'next/headers';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { UploadForm } from '../_components/UploadForm';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export default async function LawUploadPage() {
  const h = await headers();
  const req = new Request('http://internal/law/upload', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    redirect('/?login=1&next=/law/upload');
  }

  if (!hasPermission(out.session, PERM.law.contract.upload)) {
    redirect('/forbidden?path=/law/upload&reason=law:contract:upload');
  }

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: <T id="law.title" locale={locale} />, href: '/law' }, { label: <T id="law.uploadCrumb" locale={locale} /> }]} />
      <PageLayout
        title={<T id="law.uploadTitle" locale={locale} />}
        subtitle={<T id="law.uploadSubtitle" locale={locale} />}
        category={{ label: <T id="law.title" locale={locale} />, icon: 'Scale', href: '/law' }}
        actions={<Link href="/law" className="rounded-lg border border-rule px-3 py-2 text-xs text-ink-2 hover:border-rule"><T id="law.backToContracts" locale={locale} /></Link>}
      >
        <UploadForm />
      </PageLayout>
    </>
  );
}