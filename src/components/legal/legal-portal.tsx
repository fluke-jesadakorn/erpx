"use client";
import WorkspaceLink from '@/components/workplace/workspace-link';
import {legalPortalSnapshot,legalPortalSnapshotMatches,type LegalPortalSnapshot} from "@/lib/legal/portal-snapshot";
import {createLegalPortalCommandClient,type PortalCommandSource,type PortalCommandResult} from '@/lib/legal/portal-command-client';
import {DraftFormDrawer} from '@/components/ui/draft-form-drawer';
import {LegalPortalDraftBoundary} from './legal-portal-draft-boundary';
import { Card } from "@/components/ui/foundation";

import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { QuickAccessShelf } from "@/components/work_projection/quick-access/shelf";
import type { QuickAccessItem, QuickAccessPreference } from "@/lib/work_projection/quick-access";
import rail from "@/components/work_projection/quick-access/area-rail.module.css";
import s from "./legal.module.css";
type Shared=LegalPortalSnapshot;
const emptyBilling={entryType:'time',description:'',minutes:'0',amount:'0',evidence:''};
export function LegalPortal({
  slug,
  commandScope,
  initial,
  initialShareId,
  initialDetail,
  quickAccessItems,
  quickAccessPreferences,
}: {
  slug: string;
  commandScope:string;
  initial: Record<string, unknown>[];
  initialShareId?: string;
  initialDetail?: LegalPortalSnapshot;
  quickAccessItems: QuickAccessItem[];
  quickAccessPreferences: QuickAccessPreference;
}) {
  const [selected, setSelected] = useState<Shared | null>(initialDetail??null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [body, setBody] = useState("");
  const [notice,setNotice]=useState(''),[file,setFile]=useState<File|null>(null),[billing,setBilling]=useState(emptyBilling),[billingOpen,setBillingOpen]=useState(false);
  const selectedRef=useRef(selected),bodyRef=useRef(body),billingRef=useRef(billing),fileRef=useRef(file),fileInput=useRef<HTMLInputElement>(null),alive=useRef(true),readRevision=useRef(0),commandInFlight=useRef(false);
  useEffect(()=>{selectedRef.current=selected;bodyRef.current=body;billingRef.current=billing;fileRef.current=file;},[selected,body,billing,file]);
  const editBody=(value:string)=>{bodyRef.current=value;setBody(value);},editBilling=(value:typeof emptyBilling)=>{billingRef.current=value;setBilling(value);},editFile=(value:File|null)=>{fileRef.current=value;setFile(value);};
  const [seededDetail,setSeededDetail]=useState(initialDetail);
  if(seededDetail!==initialDetail){setSeededDetail(initialDetail);setSelected(initialDetail??null);}
  const api = `/api/legal-portal/${encodeURIComponent(slug)}`;
  const send=useMemo(()=>createLegalPortalCommandClient(api,commandScope),[api,commandScope]);
  const [pendingAction,setPendingAction]=useState<ReturnType<typeof send.pending>>(null);
  useEffect(()=>{let active=true;const reads=readRevision;alive.current=true;queueMicrotask(()=>{if(active)setPendingAction(send.pending());});return()=>{active=false;alive.current=false;reads.current++;};},[send]);
  const call = useCallback(async(path:string)=>{
      const r = await fetch(api+path,{cache:'no-store'});
      const j = await r.json();
      if (!r.ok) throw new Error(j.error?.message ?? "Request failed.");
      return j.data;
    },[api]);
  const open = useCallback(
    async (id: string) => {
      const revision=++readRevision.current;
      try {
        const next=legalPortalSnapshot(await call(`/${encodeURIComponent(id)}`));if(next.shareId!==id)throw Error('The shared-document source changed.');
        if(alive.current&&revision===readRevision.current){setSelected(next);setError('');}
      } catch (e) {
        if(alive.current&&revision===readRevision.current)setError((e as Error).message);
      }
    },
    [call],
  );
  useEffect(() => {
    if (!initialShareId||legalPortalSnapshotMatches(initialDetail,initialShareId)) return;
    let active=true;const reads=readRevision;void Promise.resolve().then(()=>{if(active)return open(initialShareId);});
    return()=>{active=false;reads.current++;};
  }, [initialShareId, initialDetail,open]);
  const source=(current:Shared):PortalCommandSource=>({shareId:current.shareId,version:current.doc.version,checksum:current.doc.checksum});
  async function acknowledged(result:PortalCommandResult){
    if(!alive.current||selectedRef.current?.shareId!==result.attempt.source.shareId)return;
    if(result.attempt.kind==='command'){
      const input=JSON.parse(result.attempt.body) as Record<string,unknown>;
      if(input.action==='comment'&&bodyRef.current===input.body)editBody('');
      if(input.action==='billing.submit'&&Object.entries(billingRef.current).every(([key,value])=>input[key]===value)){editBilling(emptyBilling);setBillingOpen(false);}
    }else if(result.file===fileRef.current){editFile(null);if(fileInput.current)fileInput.current.value='';}
    setPendingAction(send.pending());setNotice('The exact shared-document action was saved.');
    await open(result.attempt.source.shareId);
  }
  async function act(action: string, extra = {}) {
    const current=selectedRef.current;if (!current||current.purchase?.settleExisting===false||commandInFlight.current) return false;commandInFlight.current=true;
    setBusy(true);
    setError("");
    setNotice('');
    try {
      await acknowledged(await send.command(source(current),{action,shareId:current.shareId,...extra}));return true;
    } catch (e) {
      if(alive.current)setError((e as Error).message);return false;
    } finally {
      commandInFlight.current=false;if(alive.current){setBusy(false);setPendingAction(send.pending());}
    }
  }
  async function upload(){const current=selectedRef.current,original=fileRef.current;if(!current||!original||commandInFlight.current||current.purchase?.settleExisting===false)return;commandInFlight.current=true;setBusy(true);setError('');setNotice('');try{await acknowledged(await send.upload(source(current),original));}catch(error){if(alive.current)setError((error as Error).message);}finally{commandInFlight.current=false;if(alive.current){setBusy(false);setPendingAction(send.pending());}}}
  async function recover(){if(commandInFlight.current||selectedRef.current?.purchase?.history===false)return;commandInFlight.current=true;setBusy(true);setError('');setNotice('');try{const result=await send.recover(send.pending()?.needsFile?fileRef.current??undefined:undefined);if(result)await acknowledged(result);}catch(error){if(alive.current)setError((error as Error).message);}finally{commandInFlight.current=false;if(alive.current){setBusy(false);setPendingAction(send.pending());}}}
  const billingDirty=JSON.stringify(billing)!==JSON.stringify(emptyBilling),dirty=Boolean(body||file||billingDirty||pendingAction);
  return (
    <LegalPortalDraftBoundary dirty={dirty}>
      <a href="#legal-portal-content" className="skip-link">
        Skip to shared documents
      </a>
      <main className={s.workspace} style={{ padding: 32, maxWidth: 1360 }}>
        <header className={s.header}>
          <div>
            <span className={s.eyebrow}>ERPX · Secure collaboration</span>
            <h1>Legal portal</h1>
            <p>Only document versions explicitly shared with your account appear here.</p>
          </div>
        </header>
        <div className={rail.layout}>
          <aside className={rail.rail} aria-label="Quick access">
            <QuickAccessShelf appearance="standard"
              title="Quick access"
              items={quickAccessItems}
              initialPreferences={quickAccessPreferences}
              defaultIds={quickAccessItems.slice(0, 6).map((item) => item.id)}
              endpoint={`${api}/quick-access`}
              requestMode="plain"
              embedded
              placement="rail"
              showAllMode={false}
            />
          </aside>
          <div id="legal-portal-content" tabIndex={-1} className={rail.content}>
            {error && (
              <InlineNotice tone="danger" className={s.error} role="alert">
                {error}
              </InlineNotice>
            )}
            {notice&&<InlineNotice role="status">{notice}</InlineNotice>}
            {pendingAction&&<InlineNotice tone="warning" role="status"><p>The previous {pendingAction.kind==='upload'?'upload':'action'} has no confirmed result. Check its original request before submitting different work.{pendingAction.needsFile?` Reselect the same file: ${pendingAction.name}.`:''}</p><Button appearance="standard" type="button" disabled={busy||selected?.purchase?.history===false||pendingAction.needsFile&&!file} onClick={()=>void recover()}>Check previous result</Button></InlineNotice>}
            {selected ? (
              <Card as="section" appearance="standard" className={s.panel}>
                <a data-slot="button" className={['ui-standard-control', (s.button)].filter(Boolean).join(' ')} href={`/legal-portal/${encodeURIComponent(slug)}`}>
                  ← Shared documents
                </a>
                <h2>
                  {selected.doc.name} · v{selected.doc.version}
                </h2>
                <div className={s.small}>SHA-256: {selected.doc.checksum}</div>
                <pre className={s.body}>{selected.doc.body}</pre>
                <a download href={`${api}/${encodeURIComponent(selected.shareId)}/download`}>Download original</a>
                <div className={s.actions}>
                  <Button appearance="standard"
                    type="submit"
                    variant="ghost"
                    className={s.button}
                    disabled={busy||Boolean(pendingAction)||selected.purchase?.settleExisting===false}
                    onClick={() => act("accept")}
                  >
                    Accept invitation
                  </Button>
                </div>
                <Field appearance="standard" className={s.field}>
                  Submit a revised PDF or DOCX (up to 10 MB)
                  <TextInput appearance="standard"
                    ref={fileInput}
                    type="file"
                    accept=".pdf,.docx"
                    disabled={selected.purchase?.settleExisting===false&&!pendingAction?.needsFile}
                    onChange={event=>editFile(event.target.files?.[0]??null)}
                  />
                </Field>
                <Button appearance="standard" type="button" disabled={busy||!file||Boolean(pendingAction)||selected.purchase?.settleExisting===false} onClick={()=>void upload()}>Submit revised document</Button>
                <h3>Shared discussion</h3>
                {selected.comments.map((c, i) => (
                  <p key={i} className={s.body}>
                    {c.body}
                  </p>
                ))}
                <Field appearance="standard" className={s.field}>
                  Comment
                  <TextArea appearance="standard" value={body} onChange={(e) => editBody(e.target.value)} />
                </Field>
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  className={s.button}
                  disabled={busy || !body.trim()||Boolean(pendingAction)||selected.purchase?.settleExisting===false}
                  onClick={() => act("comment", { body })}
                >
                  Send comment
                </Button>
                {selected.kind === "counsel" && (
                  <>
                    <Button appearance="standard" type="button" disabled={Boolean(pendingAction)||selected.purchase?.settleExisting===false&&!billingDirty} onClick={()=>setBillingOpen(true)}>Submit time or fees for review</Button>
                    <DraftFormDrawer open={billingOpen} title="Submit time or fees for review" dirty={billingDirty} dismissDisabled={busy||Boolean(pendingAction)} onClose={()=>setBillingOpen(false)} onDiscard={()=>{editBilling(emptyBilling);setBillingOpen(false);}}>
                    {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
                    {notice&&<InlineNotice role="status">{notice}</InlineNotice>}
                    {pendingAction&&<InlineNotice tone="warning" role="status"><p>The previous {pendingAction.kind==='upload'?'upload':'action'} has no confirmed result. Check its original request before submitting different work.{pendingAction.needsFile?` Reselect the same file: ${pendingAction.name}.`:''}</p><Button appearance="standard" type="button" disabled={busy||selected.purchase?.history===false||pendingAction.needsFile&&!file} onClick={()=>void recover()}>Check previous result</Button></InlineNotice>}
                    <form
                      className={s.detail}
                      onSubmit={(e) => {
                        e.preventDefault();
                        void act(
                          "billing.submit",
                          Object.fromEntries(new FormData(e.currentTarget)),
                        );
                      }}
                    >
                      <Field appearance="standard" className={s.field}>
                        Type
                        <Select appearance="standard" name="entryType" value={billing.entryType} onChange={event=>editBilling({...billing,entryType:event.target.value})}>
                          <option value="time">Time</option>
                          <option value="fee">Fee</option>
                          <option value="invoice">Invoice</option>
                        </Select>
                      </Field>
                      <Field appearance="standard" className={s.field}>
                        Work description
                        <TextArea appearance="standard" name="description" required minLength={3} value={billing.description} onChange={event=>editBilling({...billing,description:event.target.value})} />
                      </Field>
                      <Field appearance="standard" className={s.field}>
                        Minutes
                        <TextInput appearance="standard" name="minutes" type="number" min={0} value={billing.minutes} onChange={event=>editBilling({...billing,minutes:event.target.value})} />
                      </Field>
                      <Field appearance="standard" className={s.field}>
                        Amount in engagement currency minor units
                        <TextInput appearance="standard" name="amount" type="number" min={0} value={billing.amount} onChange={event=>editBilling({...billing,amount:event.target.value})} />
                      </Field>
                      <Field appearance="standard" className={s.field}>
                        Timesheet / invoice evidence
                        <TextArea appearance="standard" name="evidence" required minLength={3} value={billing.evidence} onChange={event=>editBilling({...billing,evidence:event.target.value})} />
                      </Field>
                      <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy||Boolean(pendingAction)||selected.purchase?.settleExisting===false}>
                        Submit for internal review
                      </Button>
                    </form>
                    </DraftFormDrawer>
                    {selected.billing.map((b) => (
                      <p key={b.id}>
                        {b.title} · {b.state} · {b.data.minutes} minutes · {b.data.amount} minor
                        units
                      </p>
                    ))}
                  </>
                )}
                {selected.envelopes.map((e) => (
                  <div className={s.notice} key={e.id}>
                    <h3>Signature · {e.state}</h3>
                    <p className={s.body}>{e.consent}</p>
                    <p>
                      <WorkspaceLink href="/security">Verify MFA</WorkspaceLink> before signing.
                    </p>
                    {e.state === "sent" && e.signer_state === "pending" && (
                      <div className={s.actions}>
                        <Button appearance="standard"
                          type="submit"
                          variant="ghost"
                          className={s.button}
                          disabled={busy||Boolean(pendingAction)||selected.purchase?.settleExisting===false}
                          onClick={() => act("view", { envelopeId: e.id })}
                        >
                          I reviewed this document
                        </Button>
                        {selected.signingEnabled&&<><Button appearance="standard"
                          type="submit"
                          variant="ghost"
                          className={s.button}
                          disabled={busy||selected.purchase?.settleExisting===false}
                          onClick={() => act("sign", { envelopeId: e.id, consent: e.consent })}
                        >
                          Accept and sign
                        </Button>
                        <Button appearance="standard"
                          type="submit"
                          variant="ghost"
                          className={s.button}
                          disabled={busy||selected.purchase?.settleExisting===false}
                          onClick={() => act("decline", { envelopeId: e.id })}
                        >
                          Decline
                        </Button></>}
                      </div>
                    )}
                  </div>
                ))}
              </Card>
            ) : initialShareId && !error ? (
              <Card as="section" appearance="standard" className={s.panel} role="status" aria-busy="true">
                Opening shared document…
              </Card>
            ) : (
              <Card as="section" appearance="standard" className={s.panel}>
                {initial.length ? (
                  initial.map((x) => (
                    <div className={s.event} key={String(x.id)}>
                      <a
                        className={s.link}
                        href={`/legal-portal/${encodeURIComponent(slug)}?share=${encodeURIComponent(String(x.id))}`}
                      >
                        {String(x.name)} · v{String(x.version)}
                      </a>
                      <p className={s.small}>
                        Access expires <TimeText appearance="standard" value={Number(x.expires_at)} />
                      </p>
                    </div>
                  ))
                ) : (
                  <div className={s.empty}>
                    No documents have been shared with this verified account.
                  </div>
                )}
              </Card>
            )}
          </div>
        </div>
      </main>
    </LegalPortalDraftBoundary>
  );
}
