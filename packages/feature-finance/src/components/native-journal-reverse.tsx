'use client';
import { useEffect, useRef, useState } from 'react';
import { holdingFetch } from '@erpx/contracts/holding-fetch';
import { contextGeneration, contextIsSwitching } from '@erpx/feature-work_projection/lib/context-switch';
import { announceWorkChange } from '@erpx/feature-work_projection/lib/work-refresh';
import { useRouter } from '@erpx/feature-shared/lib/destination-router';
import { Button, InlineNotice, Field, Select, TextInput, FormLayout } from '@erpx/ui/components/foundation';
import { acknowledgeFinanceForm, useFinanceDraftCommandPending } from '@erpx/feature-finance/components/finance-draft-boundary';
import { financeDocumentFormUnchanged } from '@erpx/feature-finance/lib/document-post-control';
import type { PostContext } from '../application/native-journal-post';
import { confirmedReversal, reversalReceipt, type ReverseAttempt, type ReverseInput } from '../application/native-journal-reverse';
import type { FinanceJournalWorkspaceReadResponse } from '@erpx/contracts/finance-journal-read';

type PeriodView = { id?: string | number | null; name?: string | number | null; state?: string | number | null };
type Held = { attempt: ReverseAttempt; url: string; token: object; node: HTMLElement; generation: number; drafts: { form: HTMLFormElement; version: string | null }[]; completion: {form: HTMLFormElement; formEditVersion: string | null} };
// Retain an unresolved command across a drawer remount in this browser session.
const unresolved = new Map<string, Held>();
const sameContext = (a: PostContext, b: PostContext) => {
  const sameSet = (x: readonly string[], y: readonly string[]) => x.length === y.length && new Set(x).size === new Set(y).size && x.every(value => y.includes(value));
  return Object.entries(a).every(([key, value]) => key === 'modules' || key === 'permissions' || b[key as keyof PostContext] === value) && sameSet(a.modules, b.modules) && sameSet(a.permissions, b.permissions);
};
const stamp = (node: HTMLElement) => [node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workRevision,node.dataset.workAssignmentVersion];
const matches = (node: HTMLElement, c: PostContext) => JSON.stringify(stamp(node)) === JSON.stringify([c.userId,c.companyId,c.assignmentId,String(c.revision),String(c.assignmentVersion)]);
export function NativeJournalReverse({ url, id, version, context, allowed, periods, draftScope }: { url: string; id: string; version: unknown; context: PostContext; allowed: boolean; periods: readonly PeriodView[]; draftScope?: string }) {
  const [initial] = useState(() => unresolved.get(url) ?? null);
  const held = useRef<Held | null>(initial), running = useRef(false);
  const [busy,setBusy] = useState(false), [message,setMessage] = useState(''), [saved,setSaved] = useState(!!initial?.attempt.receipt), [hasAttempt,setHasAttempt] = useState(!!initial);
  const track = useFinanceDraftCommandPending(), router = useRouter();
  useEffect(() => { const existing = held.current; if (existing) track(existing.token,existing.completion,true); }, [track]); // Restore the guard after a drawer remount.
  async function send(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (running.current) return;
    let current = held.current;
    if (!current) {
      const node = document.querySelector<HTMLElement>('[data-work-revision]');
      if (!allowed || !node || node.dataset.workStale === 'true' || contextIsSwitching() || !matches(node, context) || typeof version !== 'number' || !Number.isSafeInteger(version) || version < 1 || version >= Number.MAX_SAFE_INTEGER) { setMessage('This Finance action is unavailable. Your draft is retained.'); return; }
      const form = event.currentTarget, values = new FormData(form);
      const periodId=values.get('periodId'), entryDate=values.get('entryDate'), memo=values.get('memo');
      if (typeof periodId !== 'string' || typeof entryDate !== 'string' || memo !== null && typeof memo !== 'string') { setMessage('This Finance action is unavailable. Your draft is retained.'); return; }
      const input: ReverseInput = { periodId, entryDate, ...(memo === null ? {} : {memo}) };
      const attempt: ReverseAttempt = { id, version, input, body: JSON.stringify(input), key: crypto.randomUUID(), context: { ...context, modules: [...context.modules], permissions: [...context.permissions] } };
      current = { attempt, url, token: {}, node, generation: contextGeneration(), drafts: [...document.querySelectorAll<HTMLFormElement>("form[data-finance-draft]")].map(form => ({form, version: form.getAttribute("data-erpx-edit-version")})), completion: {form,formEditVersion:form.getAttribute("data-erpx-edit-version")} };
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
        const response = await holdingFetch(`${originalUrl}/reverse`, { method: 'POST', headers: postHeaders, body: attempt.body });
        const value: unknown = await response.json();
        const receipt = response.status === 201 && value && typeof value === 'object' ? reversalReceipt(attempt,(value as {data?:unknown}).data) : undefined;
        if (!receipt) throw new Error('Unable to confirm reversal. Retry the original request.');
        attempt.receipt = receipt; setSaved(true);
      }
      if (!stillCurrent()) throw new Error('Journal saved; readback is pending until the original work context is restored.');
      const original = await holdingFetch(`${originalUrl}/reversal-workspace`, { headers, cache: 'no-store' });
      const originalValue: unknown = await original.json();
      const childUrl = `${originalUrl.slice(0, originalUrl.lastIndexOf('/') + 1)}${encodeURIComponent(attempt.receipt.reversalJournalId)}/reversal-workspace`;
      const child = await holdingFetch(childUrl, { headers, cache: 'no-store' });
      const childValue: unknown = await child.json();
      if (!original.ok || !child.ok || !confirmedReversal(attempt,originalValue as FinanceJournalWorkspaceReadResponse,childValue as FinanceJournalWorkspaceReadResponse) || !stillCurrent()) throw new Error('Journal saved; readback is pending. Retry confirmation.');
      acknowledgeFinanceForm(completion); announceWorkChange(); track(current.token,completion,false); unresolved.delete(current.url); held.current = null; setHasAttempt(false); setSaved(false);
      const drafts = [...document.querySelectorAll<HTMLFormElement>("form[data-finance-draft]")];
      if (!financeDocumentFormUnchanged(completion) || current.drafts.some(d => d.version !== null && d.form !== completion.form || !d.form.isConnected || d.form.getAttribute("data-erpx-edit-version") !== d.version) || drafts.some(form => !current.drafts.some(d => d.form === form) && form.getAttribute("data-erpx-edit-version") !== null)) { setMessage('Journal saved. Your newer draft is retained.'); return; }
      router.refresh();
    } catch (error) { setMessage(attempt.receipt ? 'Journal saved; readback is pending. Retry confirmation.' : error instanceof Error ? error.message : 'Unable to confirm reversal. Retry the original request.'); }
    finally { running.current = false; setBusy(false); }
  }
  if (!allowed && !hasAttempt && !message) return null;
  return <FormLayout data-finance-draft={draftScope} className="ui-form-grid" onSubmit={event => void send(event)}>
    {message && <InlineNotice tone="danger" role="alert">{message}</InlineNotice>}
    <Field appearance="standard">Reversal period<Select appearance="standard" name="periodId" required>{periods.filter(p => p.state === 'open').map(p => <option key={String(p.id)} value={String(p.id)}>{String(p.name)}</option>)}</Select></Field>
    <Field appearance="standard">Reversal date<TextInput appearance="standard" name="entryDate" type="date" required defaultValue={new Date().toISOString().slice(0,10)} /></Field>
    <Field appearance="standard">Reason<TextInput appearance="standard" name="memo" required minLength={3} /></Field>
    <Button appearance="standard" clientManaged type="submit" formNoValidate={hasAttempt} variant="secondary" disabled={busy || (!allowed && !hasAttempt)}>{busy ? 'Confirming…' : saved ? 'Retry readback' : hasAttempt ? 'Retry original reversal' : 'Post reversing journal'}</Button>
  </FormLayout>;
}
