import Link from 'next/link';
import { headers } from 'next/headers';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { UploadForm } from '../_components/UploadForm';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { NoPermissionView } from '@/components/NoPermissionView';

export default async function LawUploadPage() {
  const h = await headers();
  const req = new Request('http://internal/law/upload', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law', href: '/law' }, { label: 'Upload' }]} />
        <PageLayout title="Upload contract" subtitle="Store the source in MinIO">
          <NoPermissionView kind="locked" actor={null} attemptedPath="/law/upload" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.law.contract.upload)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law', href: '/law' }, { label: 'Upload' }]} />
        <PageLayout title="Upload contract" subtitle="Store the source in MinIO">
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/law/upload"
            reason="law:contract:upload required."
          />
        </PageLayout>
      </>
    );
  }

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law', href: '/law' }, { label: 'Upload' }]} />
      <PageLayout
        title="Upload contract"
        subtitle="Store the source in MinIO and queue text chunking with bge-m3 embeddings"
        category={{ label: 'Law', icon: 'scale', href: '/law' }}
        actions={<Link href="/law" className="rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:border-slate-500">Back to contracts</Link>}
      >
        <UploadForm />
      </PageLayout>
    </>
  );
}