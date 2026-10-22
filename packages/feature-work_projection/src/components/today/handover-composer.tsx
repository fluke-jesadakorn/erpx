"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import { Button, Field, TextArea, TextInput } from "@erpx/ui/components/foundation";

import Link from "@erpx/workplace/components/workspace-link";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { handoverSuggestion } from "@erpx/feature-shared/lib/handover";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { HandoverView } from "@erpx/server-operations/public/handover";
import styles from "@erpx/feature-work_projection/components/today/handover-composer.module.css";
import {createHandoverPublishCommands,handoverViewMatches,type HandoverInput,type HandoverPublishAttempt} from '@erpx/feature-work_projection/lib/handover-command';
import {handoverReceiptContext} from '@erpx/feature-work_projection/lib/handover-acknowledgement';
import {useSocialRenderedContext} from '@erpx/feature-collaboration/components/social-command-context';
import {registerUnsavedWork,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';

async function responseData<T>(response: Response): Promise<T> {
  const body = (await response.json()) as { data?: T; error?: { message?: string } };
  if (!response.ok || body.data === undefined)
    throw new Error(body.error?.message ?? "The handover could not be loaded. Try again.");
  return body.data;
}
function draftKey(base: string, ownerKey: string, date: string) {
  return `erpx:handover-draft:${base}:${ownerKey}:${date}`;
}

export function HandoverComposer({
  base,
  onPublished,
  initialOpen = false,
}: {
  base: string;
  initialOpen?: boolean;
  onPublished: () => Promise<void>;
}) {
  const messageField = useRef<HTMLTextAreaElement>(null);
  const [open, setOpen] = useState(initialOpen);
  const [view, setView] = useState<HandoverView | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [recipients, setRecipients] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [body, setBody] = useState("");
  const [edited, setEdited] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [loading, setLoading] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [error, setError] = useState("");
  const [publishedId, setPublishedId] = useState<string | null>(null);
  const renderedContext=useSocialRenderedContext(),rendered=useRef(renderedContext);useLayoutEffect(()=>{rendered.current=renderedContext;},[renderedContext]);
  const commands=useRef(createHandoverPublishCommands()),editRevision=useRef(0),inFlight=useRef(false),sourceSnapshot=useRef<HandoverView|null>(null),draftContext=useRef<string|null>(null),storage=useRef<{draft:string;pending:string}|null>(null),form=useRef<HTMLFormElement>(null);
  const [dirty,setDirty]=useState(false),[pending,setPending]=useState<HandoverPublishAttempt|null>(null),[notice,setNotice]=useState(''),[saved,setSaved]=useState(false),[sourceVersions,setSourceVersions]=useState(new Map<string,HandoverView['items'][number]>());
  const context=()=>handoverReceiptContext(rendered.current?.token??'');
  const markEdited=()=>{editRevision.current++;setDirty(true);};
  useEffect(()=>registerUnsavedWork(()=>dirty||pending!==null||publishing),[dirty,pending,publishing]);
  useEffect(()=>{if(!dirty&&!pending&&!publishing)return;const unload=(event:BeforeUnloadEvent)=>{if(!contextIsSwitching())event.preventDefault();};window.addEventListener('beforeunload',unload);return()=>window.removeEventListener('beforeunload',unload);},[dirty,pending,publishing]);

  const load = useCallback(async () => {
    if(commands.current.pending()){setError('Retry the submitted handover before refreshing its source facts. Your draft is retained.');return;}
    const captured=handoverReceiptContext(rendered.current?.token??'');
    if(draftContext.current&&captured!==draftContext.current){setError('Return to the original account, Company and role to review this handover draft.');return;}
    setLoading(true);setError('');
    try {
      const response=await workFetch(`/api/v1${base}/handover`,{cache:'no-store'}),next=await responseData<unknown>(response);
      if(!handoverViewMatches(next))throw Error('The handover source snapshot is incomplete. Refresh before preparing a handover.');
      if(captured!==handoverReceiptContext(rendered.current?.token??''))throw Error('Your account or role changed while the handover was loading.');
      const previous=sourceSnapshot.current;sourceSnapshot.current=next;setView(next);setReviewing(false);
      if(previous){setNotice('Source facts refreshed. Your message and recipients are retained; reselect any changed work before review.');return;}
      draftContext.current=captured;storage.current={draft:draftKey(base,next.ownerKey,next.date),pending:`erpx:handover-submitted:${base}:${next.ownerKey}:${captured}`};
      let ids=next.defaultIds,restoredBody:string|null=null;
      try {const stored=sessionStorage.getItem(storage.current.draft);if(stored){const draft=JSON.parse(stored) as {selected?:unknown;body?:unknown};if(Array.isArray(draft.selected))ids=draft.selected.filter((id):id is string=>typeof id==='string'&&next.items.some(item=>item.id===id)).slice(0,5);if(typeof draft.body==='string'&&draft.body.length<=3000)restoredBody=draft.body;}}
      catch{/* Private in-tab backup is optional. */}
      let message=restoredBody??handoverSuggestion(next.date,next.items.filter(item=>ids.includes(item.id))),recipients:string[]=[];
      const versions=new Map(next.items.filter(item=>ids.includes(item.id)).map(item=>[item.id,item]));
      try {const stored=sessionStorage.getItem(storage.current.pending),restored=stored?commands.current.restore(JSON.parse(stored),`/api/v1${base}/handover`,captured):null;
        if(restored){setPending(restored.attempt);const later=restoredBody!==null&&(restoredBody!==restored.input.body||JSON.stringify(ids)!==JSON.stringify(restored.input.sources.map(source=>source.id)));editRevision.current=restored.attempt.editRevision+(later?1:0);if(!later){message=restored.input.body;ids=restored.input.sources.map(source=>source.id);recipients=restored.input.recipientIds;for(const source of restored.input.sources){const item=next.items.find(item=>item.id===source.id);if(item)versions.set(source.id,{...item,sourceVersion:source.sourceVersion,sourceKind:source.kind});}}setNotice('The original submitted handover has an uncertain result. Retry that exact request before publishing another.');}}
      catch{/* A malformed local value never creates command authority. */}
      setSourceVersions(versions);setSelected(ids);setBody(message);setRecipients(recipients);setEdited(restoredBody!==null);setDirty(restoredBody!==null||commands.current.pending()!==null);
    } catch(cause){setError(cause instanceof Error?cause.message:'The handover could not be loaded.');}
    finally{setLoading(false);}
  },[base]);

  useEffect(() => {
    const fromHash = () => {
      if (window.location.hash === "#handover") setOpen(true);
    };
    const fromEvent = () => setOpen(true);
    window.addEventListener("erpx:open-handover", fromEvent);
    fromHash();
    window.addEventListener("hashchange", fromHash);
    return () => { window.removeEventListener("hashchange", fromHash); window.removeEventListener("erpx:open-handover", fromEvent); };
  }, []);
  useEffect(() => {
    if (!open || view || loading || error) return;
    const timer = window.setTimeout(() => void load(), 0);
    return () => window.clearTimeout(timer);
  }, [open, view, loading, error, load]);
  useEffect(() => {
    if (!view || !open) return;
    try {
      sessionStorage.setItem(
        draftKey(base, view.ownerKey, view.date),
        JSON.stringify({ selected, body }),
      );
    } catch {
      /* Publishing remains available without browser storage. */
    }
  }, [base, view, open, selected, body]);

  useEffect(() => {
    if (open && view) messageField.current?.focus({ preventScroll: true });
  }, [open, view]);

  function toggleSource(id: string) {
    markEdited();
    const next = selected.includes(id)
      ? selected.filter((value) => value !== id)
      : selected.length < 5
        ? [...selected, id]
        : selected;
    setSelected(next);
    setSourceVersions(prior=>{const versions=new Map(prior);if(next.includes(id)){const item=view?.items.find(item=>item.id===id);if(item)versions.set(id,item);}else versions.delete(id);return versions;});
    setReviewing(false);
    setError("");
    if (!edited && view)
      setBody(
        handoverSuggestion(
          view.date,
          view.items.filter((item) => next.includes(item.id)),
        ),
      );
  }

  function discard(){if(inFlight.current||commands.current.pending())return;try{if(storage.current)sessionStorage.removeItem(storage.current.draft);}catch{/* Optional backup. */}sourceSnapshot.current=null;setSourceVersions(new Map());draftContext.current=null;storage.current=null;editRevision.current++;setDirty(false);setView(null);setBody('');setSelected([]);setRecipients([]);setSearch('');setEdited(false);setReviewing(false);setError('');setNotice('');setSaved(false);setOpen(false);}
  async function execute(attempt:HandoverPublishAttempt){
    if(inFlight.current)return;inFlight.current=true;setPublishing(true);setError('');
    try{if(storage.current)sessionStorage.setItem(storage.current.pending,JSON.stringify(attempt));}catch{/* In-memory retry survives when optional storage is unavailable. */}
    try {const {receipt,unchanged}=await commands.current.execute(attempt,context,()=>editRevision.current);setPending(null);try{if(storage.current)sessionStorage.removeItem(storage.current.pending);}catch{/* Optional backup. */}setPublishedId(receipt.id);announceWorkChange();
      if(unchanged){if(form.current)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form:form.current,formEditVersion:form.current.getAttribute('data-erpx-edit-version')}}));try{if(storage.current)sessionStorage.removeItem(storage.current.draft);}catch{/* Optional backup. */}sourceSnapshot.current=null;draftContext.current=null;storage.current=null;setDirty(false);setView(null);setRecipients([]);setReviewing(false);setOpen(false);}
      else{setSaved(true);setReviewing(false);setNotice('Your submitted handover was published. Your newer draft remains here for review.');}
      await onPublished().catch(()=>{/* The exact published post link remains available. */});
    }catch(cause){const unresolved=commands.current.pending();setPending(unresolved);try{if(storage.current){if(unresolved)sessionStorage.setItem(storage.current.pending,JSON.stringify(unresolved));else sessionStorage.removeItem(storage.current.pending);}}catch{/* Exact in-memory recovery remains available. */}setError(cause instanceof Error?cause.message:'The handover was not acknowledged. Retry the submitted handover.');}
    finally{inFlight.current=false;setPublishing(false);}
  }
  async function publish(){if(!view||!selected.length||!recipients.length||!body.trim()||inFlight.current||saved)return;try{if(draftContext.current!==context())throw Error('Return to the original account, Company and role before publishing this handover.');const input:HandoverInput={date:view.date,body,sources:selected.flatMap(id=>{const item=sourceVersions.get(id);return item?[{id,sourceVersion:item.sourceVersion,kind:item.sourceKind??'staff_task'}]:[];}),recipientIds:recipients};if(input.sources.length!==selected.length)throw Error('Reselect the missing source before publishing.');const attempt=commands.current.prepare(`/api/v1${base}/handover`,input,context(),editRevision.current);await execute(attempt);}catch(cause){setError(cause instanceof Error?cause.message:'Review the original submitted handover.');}}
  const staleSources=selected.some(id=>{const current=view?.items.find(item=>item.id===id),original=sourceVersions.get(id);return !current||!original||current.sourceVersion!==original.sourceVersion||current.sourceKind!==original.sourceKind;}),staleRecipients=recipients.some(id=>!view?.people.some(person=>person.id===id));

  const selectedPeople = view?.people.filter((person) => recipients.includes(person.id)) ?? [];
  const people =
    view?.people.filter((person) =>
      `${person.name} ${person.id}`.toLowerCase().includes(search.toLowerCase()),
    ) ?? [];
  return (
    <section
      id="handover"
      className={styles.handover}
      data-open={open}
      aria-labelledby="handover-title"
    >
      <div className={styles.header}>
        <div>
          <h2 id="handover-title">{open ? "Pass work to the next person" : "Pass on work"}</h2>
          {open && (
            <p>
              Prepare a shift handover from tasks you can open. Publishing leaves task status
              unchanged.
            </p>
          )}
        </div>
        <Button appearance="standard"
          variant="ghost"
          type="button"
          aria-expanded={open}
          aria-controls="handover-editor"
          hidden={open}
          onClick={() => setOpen(true)}
        >
          {open ? "Close" : "Draft handover"}
        </Button>
      </div>
      {publishedId && (
        <InlineNotice tone="info" className={styles.success} role="status">
          Handover published. <Link href={canonicalUiHref(`${base}/posts/${publishedId}`)}>Open post</Link>
        </InlineNotice>
      )}
      <DraftFormDrawer appearance="standard" open={open} dirty={dirty} onClose={()=>setOpen(false)} onDiscard={discard} title="Pass work to the next person" finalFocus={()=>document.querySelector<HTMLButtonElement>('#handover button')} dismissDisabled={publishing||pending!==null} retainFormContents={dirty||pending!==null}><form ref={form} id="handover-editor" className={styles.editor} data-handover-draft onSubmit={event=>event.preventDefault()}>
        {pending&&<InlineNotice tone="warning"><p>The submitted handover has an uncertain result. Retry its original message, sources and recipients before publishing another. Your newer edits are retained.</p><Button appearance="standard" type="button" disabled={publishing} onClick={()=>void execute(pending)}>Retry submitted handover</Button></InlineNotice>}
        {notice&&<InlineNotice tone="info" role="status">{notice}</InlineNotice>}
        {saved&&<Button appearance="standard" type="button" onClick={()=>{setSaved(false);setNotice('Review your retained draft and current sources before publishing it.');}}>Review retained draft</Button>}
        {(staleSources||staleRecipients)&&<InlineNotice tone="warning">Selected work or recipients changed. Your message is retained. Reselect the affected work and recipients before review.</InlineNotice>}
        {loading && <InlineNotice tone="info" role="status">Loading readable tasks…</InlineNotice>}
        {error && (
          <InlineNotice tone="danger" className={styles.error} role="alert">
            {error}{" "}
            <Button appearance="standard" variant="ghost" type="button" disabled={publishing||loading||pending!==null} onClick={() => void load()}>
              Refresh sources
            </Button>
          </InlineNotice>
        )}
        {!loading && view?.unavailable && <p className={styles.notice}>{view.reason}</p>}
        {!loading && view && !view.unavailable && (
          <>
            {!reviewing ? (
              <>
                <p className={styles.note}>
                  Select up to five linked tasks. The recipients must already be able to open every
                  selected task.
                </p>
                <fieldset className={styles.tasks}>
                  <legend>Work to include · {selected.length}/5</legend>
                  {selected.filter(id=>!view.items.some(item=>item.id===id)).map(id=><Button appearance="standard" type="button" key={id} onClick={()=>toggleSource(id)}>Remove unavailable selected work</Button>)}
                  {view.items.map((item) => (
                    <Field appearance="standard" orientation="inline" key={item.id} className={styles.choice}>
                      <TextInput appearance="standard"
                        type="checkbox"
                        checked={selected.includes(item.id)}
                        disabled={!selected.includes(item.id) && selected.length >= 5}
                        onChange={() => toggleSource(item.id)}
                      />
                      <span>
                        <strong>{item.title}</strong>
                        <small>
                          {item.kind === "blocked"
                            ? "Needs help"
                            : item.kind === "completed"
                              ? "Completed"
                              : "Still in progress"}
                          {item.planned ? " · In today's plan" : ""} ·{" "}
                          <Link href={`${base}${item.href}`}>Open task</Link>
                        </small>
                      </span>
                    </Field>
                  ))}
                </fieldset>
                <Field appearance="standard" className={styles.message}>
                  Handover message
                  <TextArea ref={messageField} appearance="standard"
                    rows={8}
                    maxLength={3000}
                    value={body}
                    onChange={(event) => {
                      markEdited();
                      setBody(event.target.value);
                      setEdited(true);
                      setReviewing(false);
                    }}
                  />
                </Field>
                <fieldset className={styles.people}>
                  <legend>Who should receive this? · {recipients.length}/20</legend>
                  <TextInput appearance="standard"
                    type="search"
                    aria-label="Find recipients"
                    value={search}
                    onChange={(event) => setSearch(event.target.value)}
                    placeholder="Find a colleague"
                  />
                  {view.people.length === 0 && (
                    <p>No eligible colleagues are available in this work scope.</p>
                  )}
                  {recipients.filter(id=>!view.people.some(person=>person.id===id)).map(id=><Button appearance="standard" type="button" key={id} onClick={()=>{markEdited();setRecipients(current=>current.filter(value=>value!==id));}}>Remove unavailable recipient</Button>)}
                  {people.map((person) => (
                    <Field appearance="standard" orientation="inline" key={person.id} className={styles.person}>
                      <TextInput appearance="standard"
                        type="checkbox"
                        checked={recipients.includes(person.id)}
                        disabled={!recipients.includes(person.id) && recipients.length >= 20}
                        onChange={() => {
                          markEdited();
                          setRecipients((current) =>
                            current.includes(person.id)
                              ? current.filter((id) => id !== person.id)
                              : [...current, person.id],
                          );
                          setReviewing(false);
                        }}
                      />
                      {person.name}
                    </Field>
                  ))}
                </fieldset>
                <div className={styles.actions}>
                  <Button appearance="standard"
                    variant="ghost"
                    type="button"
                    disabled={!selected.length || !recipients.length || !body.trim() || publishing || pending!==null || saved || staleSources || staleRecipients}
                    onClick={() => setReviewing(true)}
                  >
                    Review handover
                  </Button>
                </div>
              </>
            ) : (
              <div className={styles.review} aria-label="Review handover">
                <h3>Review before publishing</h3>
                <p>
                  <strong>Audience:</strong>{" "}
                  {selectedPeople.map((person) => person.name).join(", ")}
                </p>
                <p>
                  <strong>Linked work:</strong> {selected.length}{" "}
                  {selected.length === 1 ? "task" : "tasks"}
                </p>
                <pre>{body}</pre>
                <p className={styles.note}>
                  Recipients will see this post only while they can open its linked work. Reading or
                  thanking it does not acknowledge the handover.
                </p>
                <div className={styles.actions}>
                  <Button appearance="standard"
                    variant="ghost"
                    type="button"
                    disabled={publishing}
                    onClick={() => setReviewing(false)}
                  >
                    Edit
                  </Button>
                  <Button appearance="standard"
                    variant="ghost"
                    type="button"
                    disabled={publishing || pending!==null || saved || staleSources || staleRecipients}
                    onClick={() => void publish()}
                  >
                    {publishing ? "Publishing…" : "Publish handover"}
                  </Button>
                </div>
              </div>
            )}
          </>
        )}
      </form></DraftFormDrawer>
    </section>
  );
}
