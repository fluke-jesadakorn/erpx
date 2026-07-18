import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import { NoPermissionView } from '@/components/NoPermissionView';
import { query } from '@/db';
import { AiSettingsClient } from './AiSettingsClient';
import { ModelsTab } from './ModelsTab';
import { AssignmentsTab } from './AssignmentsTab';
import { TabStrip, type AiTabKey } from './TabStrip';
import { PERM } from '@/perm';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

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

interface ModelRow {
  id: number;
  name: string;
  provider_id: number;
  capabilities: string[];
  context_window: number | null;
  enabled: boolean;
  description: string | null;
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

async function loadModels(): Promise<ModelRow[]> {
  const res = await query<ModelRow>(
    `SELECT id, name, provider_id, capabilities, context_window, enabled, description
       FROM ai_models
       ORDER BY id`,
  );
  return res.rows;
}

interface AssignmentRow {
  id: number;
  section_key: string;
  task_type: string;
  provider_id: number;
  model_id: number;
  enabled: boolean;
  priority: number;
  params_json: Record<string, unknown> | null;
  created_at: string;
}

async function loadAssignments(): Promise<AssignmentRow[]> {
  const res = await query<AssignmentRow>(
    `SELECT id, section_key, task_type, provider_id, model_id, enabled, priority, params_json, created_at
       FROM ai_assignments
       ORDER BY priority, id`,
  );
  return res.rows;
}

export default async function AiSettingsPage({
  searchParams,
}: {
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
  const h = await headers();
  const req = new Request('http://internal/ai-settings', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();
  const sp = searchParams ? await searchParams : undefined;
  const tabParam = typeof sp?.tab === 'string' ? sp.tab : 'providers';
  const active: AiTabKey = ['providers', 'models', 'assignments', 'my-defaults'].includes(tabParam)
    ? (tabParam as AiTabKey)
    : 'providers';

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.aiSettings" locale={locale} /> }]} />
        <PageLayout title={<T id="aiSettings.title" locale={locale} />} subtitle={<T id="aiSettings.subtitle" locale={locale} />}>
          <NoPermissionView kind="locked" actor={null} attemptedPath="/ai-settings" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.ai.provider.read)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.aiSettings" locale={locale} /> }]} />
        <PageLayout title={<T id="aiSettings.title" locale={locale} />} subtitle={<T id="aiSettings.subtitle" locale={locale} />}>
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

  const [providers, models, assignments] = await Promise.all([loadProviders(), loadModels(), loadAssignments()]);
  const canEdit = hasPermission(out.session, PERM.ai.provider.update);
  const canCreate = hasPermission(out.session, PERM.ai.provider.create);
  const canDelete = hasPermission(out.session, PERM.ai.provider.delete);
  const canTest = hasPermission(out.session, PERM.ai.provider.test);
  const canEditModels = hasPermission(out.session, PERM.ai.model.update);

  const tabs = [
    { key: 'providers' as const, labelKey: 'aiSettings.tabProviders', count: providers.length },
    { key: 'models' as const, labelKey: 'aiSettings.tabModels', count: models.length },
    { key: 'assignments' as const, labelKey: 'aiSettings.tabAssignments', count: assignments.length },
  ];

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.aiSettings" locale={locale} /> }]} />
      <PageLayout
        title={<T id="aiSettings.title" locale={locale} />}
        subtitle={<T id="aiSettings.subtitleCount" locale={locale} values={{ n: providers.length }} />}
      >
        <TabStrip tabs={tabs} active={active} />
        {active === 'providers' ? (
          <AiSettingsClient
            initialProviders={providers}
            canEdit={canEdit}
            canCreate={canCreate}
            canDelete={canDelete}
            canTest={canTest}
          />
        ) : active === 'models' ? (
          <ModelsTab
            initialModels={models}
            providers={providers.map((p) => ({ id: p.id, name: p.name }))}
            canEdit={canEditModels}
            canDelete={canEditModels}
          />
        ) : (
          <AssignmentsTab
            initialAssignments={assignments.map((a) => ({
              id: a.id,
              section_key: a.section_key,
              task_type: a.task_type as 'embed' | 'chat' | 'vision',
              provider_id: a.provider_id,
              model_id: a.model_id,
              priority: a.priority,
              enabled: a.enabled,
              provider_name: providers.find((p) => p.id === a.provider_id)?.name ?? null,
              model_name: models.find((m) => m.id === a.model_id)?.name ?? null,
            }))}
            providers={providers.map((p) => ({ id: p.id, name: p.name }))}
            models={models.map((m) => ({ id: m.id, name: m.name, capabilities: m.capabilities, provider_id: m.provider_id }))}
            canCreate={canEditModels}
            canEdit={canEditModels}
            canDelete={canEditModels}
          />
        )}
      </PageLayout>
    </>
  );
}