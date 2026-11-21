'use client';
import { useEffect, useRef, useState } from 'react';
import { holdingFetch } from '@erpx/contracts/holding-fetch';
import { contextGeneration, contextIsSwitching } from '@erpx/feature-work_projection/lib/context-switch';
import { announceWorkChange } from '@erpx/feature-work_projection/lib/work-refresh';
import { useRouter } from '@erpx/feature-shared/lib/destination-router';
import { Button, InlineNotice } from '@erpx/ui/components/foundation';
import { captureFinanceForm, acknowledgeFinanceForm, useFinanceDraftCommandPending } from '@erpx/feature-finance/components/finance-draft-boundary';
import { financeDocumentFormUnchanged } from '@erpx/feature-finance/lib/document-post-control';
import { confirmedReadback, postedReceipt, type PostAttempt, type PostContext } from '../application/native-journal-post';
import type { FinanceJournalWorkspaceReadResponse } from '@erpx/contracts/finance-journal-read';

type Held = { attempt: PostAttempt; url: string; token: object; node: HTMLElement; generation: number; drafts: { form: HTMLFormElement; version: string | null }[]; completion: ReturnType<typeof captureFinanceForm> };
// Retain an unresolved command across a drawer remount in this browser session.
const unresolved = new Map<string, Held>();
const sameContext = (a: PostContext, b: PostContext) => {
  const sameSet = (x: readonly string[], y: readonly string[]) => x.length === y.length && new Set(x).size === new Set(y).size && x.every(value => y.includes(value));
  return Object.entries(a).every(([key, value]) => key === 'modules' || key === 'permissions' || b[key as keyof PostContext] === value) && sameSet(a.modules, b.modules) && sameSet(a.permissions, b.permissions);
};
const stamp = (node: HTMLElement) => [node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workRevision,node.dataset.workAssignmentVersion];
const matches = (node: HTMLElement, c: PostContext) => JSON.stringify(stamp(node)) === JSON.stringify([c.userId,c.companyId,c.assignmentId,String(c.revision),String(c.assignmentVersion)]);
export function NativeJournalPost({ url, id, version, context, allowed }: { url: string; id: string; version: unknown; context: PostContext; allowed: boolean }) {
  const [initial] = useState(() => unresolved.get(url) ?? null);
  const held = useRef<Held | null>(initial), running = useRef(false);
  const [busy,setBusy] = useState(false), [message,setMessage] = useState(''), [saved,setSaved] = useState(!!initial?.attempt.receipt), [hasAttempt,setHasAttempt] = useState(!!initial);
  const track = useFinanceDraftCommandPending(), router = useRouter();
  useEffect(() => { const existing = held.current; if (existing) track(existing.token,existing.completion,true); }, [track]); // Restore the guard after a drawer remount.
  async function send() {
    if (running.current) return;
    let current = held.current;
    if (!current) {
      const node = document.querySelector<HTMLElement>('[data-work-revision]');
      if (!allowed || !node || node.dataset.workStale === 'true' || contextIsSwitching() || !matches(node, context) || typeof version !== 'number' || !Number.isSafeInteger(version) || version < 1 || version >= Number.MAX_SAFE_INTEGER) { setMessage('This Finance action is unavailable. Your draft is retained.'); return; }
      const input = { expectedVersion: version };
      const attempt: PostAttempt = { id, input, body: JSON.stringify(input), key: crypto.randomUUID(), context: { ...context, modules: [...context.modules], permissions: [...context.permissions] } };
      current = { attempt, url, token: {}, node, generation: contextGeneration(), drafts: [...document.querySelectorAll<HTMLFormElement>("form[data-finance-draft]")].map(form => ({form, version: form.getAttribute("data-erpx-edit-version")})), completion: captureFinanceForm() };
      held.current = current; unresolved.set(url,current); track(current.token,current.completion,true); setHasAttempt(true);
    }
    // A drawer may remount without a work-context switch. Adopt only the new
    // rendered boundary for the exact original context, before any request.
    if (!current.node.isConnected && current.generation === contextGeneration() && !contextIsSwitching() && sameContext(current.attempt.context, context)) {
      const replacement = document.querySelector<HTMLElement>('[data-work-revision]');
      if (replacement?.isConnected && replacement.dataset.workStale !== 'true' && matches(replacement, current.attempt.context)) current.node = replacement;
    }
    const { attempt, url: originalUrl, node, generation, completion } = current;
    const stillCurrent = () => generation === contextGeneration() && sameContext(attempt.context, context) && !contextIsSwitching() && node.isConnected && node.dataset.workStale !== 'true' && matches(node, attempt.context);
    if (!stillCurrent()) { setMessage('Your work context changed. The original request is retained.'); return; }
    running.current = true; setBusy(true); setMessage('');
    const headers = new Headers({ 'x-erpx-context-company': attempt.context.companyId, 'x-erpx-context-assignment': attempt.context.assignmentId, 'x-erpx-context-revision': String(attempt.context.revision), 'x-erpx-context-assignment-version': String(attempt.context.assignmentVersion) });
    if (attempt.context.branchSlug) headers.set('x-erpx-workspace-branch', attempt.context.branchSlug);
    if (attempt.context.departmentSlug) headers.set('x-erpx-workspace-department', attempt.context.departmentSlug);
    try {
      if (!attempt.receipt) {
        const postHeaders = new Headers(headers); postHeaders.set('content-type','application/json'); postHeaders.set('idempotency-key',attempt.key);
        const response = await holdingFetch(`${originalUrl}/post`, { method: 'POST', headers: postHeaders, body: attempt.body });
        const value: unknown = await response.json();
        const receipt = response.ok && value && typeof value === 'object' ? postedReceipt(attempt,(value as {data?:unknown}).data) : undefined;
        if (!receipt) throw new Error('Unable to confirm posting. Retry the original request.');
        attempt.receipt = receipt; setSaved(true);
      }
      if (!stillCurrent()) throw new Error('Journal saved; readback is pending until the original work context is restored.');
      const response = await holdingFetch(`${originalUrl}/workspace`, { headers, cache: 'no-store' });
      const value: unknown = await response.json();
      if (!response.ok || !confirmedReadback(attempt,value as FinanceJournalWorkspaceReadResponse) || !stillCurrent()) throw new Error('Journal saved; readback is pending. Retry confirmation.');
      acknowledgeFinanceForm(completion); announceWorkChange(); track(current.token,completion,false); unresolved.delete(current.url); held.current = null; setHasAttempt(false); setSaved(false);
      const drafts = [...document.querySelectorAll<HTMLFormElement>("form[data-finance-draft]")];
      if (!financeDocumentFormUnchanged(completion) || current.drafts.some(d => d.version !== null || !d.form.isConnected || d.form.getAttribute("data-erpx-edit-version") !== d.version) || drafts.some(form => !current.drafts.some(d => d.form === form) && form.getAttribute("data-erpx-edit-version") !== null)) { setMessage('Journal saved. Your newer draft is retained.'); return; }
      router.refresh();
    } catch (error) { setMessage(attempt.receipt ? 'Journal saved; readback is pending. Retry confirmation.' : error instanceof Error ? error.message : 'Unable to confirm posting. Retry the original request.'); }
    finally { running.current = false; setBusy(false); }
  }
  if (!allowed && !hasAttempt && !message) return null;
  return <>{message && <InlineNotice tone="danger" role="alert">{message}</InlineNotice>}<Button appearance="standard" type="button" variant="primary" disabled={busy || (!allowed && !hasAttempt)} onClick={() => void send()}>{busy ? 'Confirming…' : saved ? 'Retry readback' : hasAttempt ? 'Retry original posting' : 'Approve and post journal'}</Button></>;
}
