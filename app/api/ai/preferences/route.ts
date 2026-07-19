import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import {
  clearUserAiPreference,
  listSelectableModels,
  loadUserAiPreference,
  saveUserAiPreference,
  type ThinkingLevel,
} from '@/ai/preferences';

export const dynamic = 'force-dynamic';

const TASKS = new Set(['embed', 'chat', 'vision']);
const THINK_LEVELS = new Set(['auto', 'low', 'medium', 'high']);
const OCR_PREFERENCE_SECTION = 'staff:ocr';
const OCR_PREFERENCE_TASK = 'vision';

function paramsFrom(req: Request) {
  const url = new URL(req.url);
  const sectionKey = url.searchParams.get('sectionKey') || 'chat:global';
  return {
    sectionKey,
    task: (url.searchParams.get('task') || (sectionKey === OCR_PREFERENCE_SECTION ? OCR_PREFERENCE_TASK : 'chat')) as 'embed' | 'chat' | 'vision',
  };
}

function preferencePermission(sectionKey: string, task: string, operation: 'read' | 'update'): string | null {
  if (sectionKey === OCR_PREFERENCE_SECTION && task === OCR_PREFERENCE_TASK) {
    return null;
  }
  return operation === 'read' ? 'ai:preference:read::allow' : 'ai:preference:update::allow';
}

function guardForPreference(req: Request, sectionKey: string, task: string, operation: 'read' | 'update') {
  const perm = preferencePermission(sectionKey, task, operation);
  return apiGuard(req, perm ? { perm } : {});
}

export async function GET(req: Request) {
  const { sectionKey, task } = paramsFrom(req);
  const guard = await guardForPreference(req, sectionKey, task, 'read');
  if (guard.response) return guard.response;
  if (!TASKS.has(task)) return NextResponse.json({ error: 'invalid task' }, { status: 400 });
  try {
    const [models, preference] = await Promise.all([
      listSelectableModels(sectionKey, task),
      loadUserAiPreference(guard.actor.id, sectionKey),
    ]);
    return NextResponse.json({ models, preference });
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : 'failed to load AI preferences' },
      { status: 500 },
    );
  }
}

export async function PUT(req: Request) {
  const body = await req.json().catch(() => ({})) as {
    sectionKey?: string;
    modelId?: number;
    thinkLevel?: ThinkingLevel;
    task?: 'embed' | 'chat' | 'vision';
  };
  const guard = await guardForPreference(req, body.sectionKey ?? 'chat:global', body.task ?? 'chat', 'update');
  if (guard.response) return guard.response;
  if (!body.sectionKey || !Number.isInteger(body.modelId) || !body.thinkLevel || !THINK_LEVELS.has(body.thinkLevel) || (body.task && !TASKS.has(body.task))) {
    return NextResponse.json({ error: 'sectionKey, modelId and thinkLevel are required' }, { status: 400 });
  }
  try {
    const preference = await saveUserAiPreference(
      guard.actor.id,
      body.sectionKey,
      body.modelId as number,
      body.thinkLevel as ThinkingLevel,
      body.task ?? 'chat',
    );
    return NextResponse.json({ ok: true, preference });
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : 'preference update failed' }, { status: 400 });
  }
}

export async function DELETE(req: Request) {
  const { sectionKey } = paramsFrom(req);
  const guard = await guardForPreference(req, sectionKey, OCR_PREFERENCE_TASK, 'update');
  if (guard.response) return guard.response;
  await clearUserAiPreference(guard.actor.id, sectionKey);
  return NextResponse.json({ ok: true });
}
