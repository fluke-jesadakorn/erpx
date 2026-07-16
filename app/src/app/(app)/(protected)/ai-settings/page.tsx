import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@folio-lib/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import { NoPermissionView } from '@/components/NoPermissionView';
import { query } from '@folio-lib/db';
import { AiSettingsClient } from './AiSettingsClient';
import { PERM } from '@folio-lib/perm';

export const dynamic = 'force-dynamic';

interface ProviderRow {
  id: number;
  name: string;
  type: 'ollama' | 'openai_compat' | 'minimax';
  base_url: string;
  enabled: boolean;
  preset: string | null;
  notes: string | null;
  has_api_key: boolean;
  created_at: string;
  updated_at: string;
}

async function loadProviders(): Promise<ProviderRow[]> {
  const res = await query<ProviderRow>(
    `SELECT id, name, type, base_url, enabled, preset, notes,
            CASE WHEN api_key_enc IS NULL THEN false ELSE true END AS has_api_key,
            created_at, updated_at
       FROM ai_providers
      ORDER BY id`,
  );
  return res.rows;
}

export default async function AiSettingsPage() {
  const h = await headers();
  const req = new Request('http://internal/ai-settings', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: 'AI Settings' }]} />
        <PageLayout title="AI Settings" subtitle="AI provider catalog">
          <NoPermissionView kind="locked" actor={null} attemptedPath="/ai-settings" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.ai.provider.read)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: 'AI Settings' }]} />
        <PageLayout title="AI Settings" subtitle="AI provider catalog">
          <NoPermissionView
            kind="locked"
            actor={out.session.user as { id: number; fullname?: string; name?: string }}
            attemptedPath="/ai-settings"
            reason="ai:provider:read required."
          />
        </PageLayout>
      </>
    );
  }

  const providers = await loadProviders();
  const canEdit = hasPermission(out.session, PERM.ai.provider.update);
  const canCreate = hasPermission(out.session, PERM.ai.provider.create);
  const canDelete = hasPermission(out.session, PERM.ai.provider.delete);
  const canTest = hasPermission(out.session, PERM.ai.provider.test);

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: 'AI Settings' }]} />
      <PageLayout
        title="AI Settings"
        subtitle={`AI provider catalog — ${providers.length} provider(s). Encrypted keys stored in ai_providers.api_key_enc (pgcrypto).`}
      >
        <AiSettingsClient
          initialProviders={providers}
          canEdit={canEdit}
          canCreate={canCreate}
          canDelete={canDelete}
          canTest={canTest}
        />
      </PageLayout>
    </>
  );
}