'use server';

import 'server-only';
import { revalidatePath } from 'next/cache';
import { listSessions, loadSession, createSession, deleteSession, renameSession, appendMessage, maybeAutoRename } from './history';
import { suggestTitle, isPlaceholderTitle } from './titleGenerator';

export async function listChatSessions() {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();
  if (!actor) return [];
  return listSessions(actor.id);
}

export async function createChatSession(title: string, model: string, seed?: string) {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();
  if (!actor) throw new Error('unauthorized');
  const source = title && !isPlaceholderTitle(title) ? title : seed ?? '';
  const finalTitle = source ? suggestTitle(source) : 'New chat';
  const session = await createSession(actor.id, finalTitle, model);
  revalidatePath('/chat');
  return session;
}

export async function loadChatSession(id: string) {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();
  if (!actor) throw new Error('unauthorized');
  return loadSession(actor.id, id);
}

export async function deleteChatSession(id: string) {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();
  if (!actor) return;
  await deleteSession(actor.id, id);
  revalidatePath('/chat');
}

export async function appendAssistantMessage(
  sessionId: string,
  msg: { content: string; blocks?: any; modelName?: string | null; latencyMs?: number | null },
  opts: { lastUserText?: string } = {},
) {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();
  if (!actor) return null;
  const inserted = await appendMessage(actor.id, sessionId, { role: 'assistant', ...msg });
  let newTitle: string | null = null;
  if (opts.lastUserText) {
    newTitle = await maybeAutoRename(actor.id, sessionId, opts.lastUserText);
    if (newTitle) revalidatePath('/chat');
  }
  return newTitle ? { message: inserted, title: newTitle } : { message: inserted };
}

export async function appendUserMessageAndAutoRename(
  sessionId: string,
  content: string,
) {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();
  if (!actor) return null;
  const inserted = await appendMessage(actor.id, sessionId, { role: 'user', content });
  const newTitle = await maybeAutoRename(actor.id, sessionId, content);
  if (newTitle) revalidatePath('/chat');
  return newTitle ? { message: inserted, title: newTitle } : { message: inserted };
}

export async function renameChatSession(id: string, title: string) {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();
  if (!actor) return;
  await renameSession(actor.id, id, title);
}
