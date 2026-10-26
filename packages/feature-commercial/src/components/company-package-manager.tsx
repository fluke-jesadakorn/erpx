"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';

import { useEffect, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import {useProviderContext} from "@erpx/feature-identity_access/components/provider-context-boundary";
import {providerPackageReview,captureProviderPackageCommand,acknowledgeProviderPackageCommand,type ProviderPackageCapture} from "@erpx/server-commercial/public/rules/provider-package-command";
import { Button, Field, TextArea, TextInput, Select, Stack, Panel, InlineNotice, Badge } from "@erpx/ui/components/foundation";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { useHydrated } from "@erpx/ui/components/use-hydrated";
import { contextGeneration, contextIsSwitching, registerUnsavedWork } from "@erpx/feature-work_projection/lib/context-switch";
import type { providerCompanyPackageOverview } from "@erpx/server-commercial/public/provider-commercial";
import type { PackageGrantBatch, PackageGrantBatchPreview } from "@erpx/server-commercial/public/package-grants";
import type { CompanyPackageGrant } from "@erpx/server-commercial/public/rules/packages";

type Snapshot = Awaited<ReturnType<typeof providerCompanyPackageOverview>>;
type Company = { id: string; name: string; slug?: string; holdingId: string; holding: string };
type Draft = { packageId: string; packageVersion: number; state: CompanyPackageGrant["state"]; starts: string; ends: string; reason: string; evidence: string };
type Review = { comparison: PackageGrantBatchPreview; request: PackageGrantBatch; review: { token: string; expiresAt: number } };
const utc = (value: number | null) => value === null ? "" : new Date(value).toISOString().slice(0, 16);

export function CompanyPackageManager({ companies, canApply, language }: { companies: Company[]; canApply: boolean; language: "en" | "th" }) {
  const th = language === "th", ready = useHydrated(),{state:provider}=useProviderContext();
  const [selected, setSelected] = useState(""), [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [packageId, setPackageId] = useState(""), [draft, setDraft] = useState<Draft | null>(null), [baseline, setBaseline] = useState("");
  const [review, setReview] = useState<Review | null>(null), [confirmed, setConfirmed] = useState(false);
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [receipt, setReceipt] = useState("");
  const [mfa, setMfa] = useState(false), [code, setCode] = useState("");
  const controller = useRef<AbortController | null>(null), epoch = useRef(0), dirtyRef = useRef(false);
  const editVersion=useRef(0),draftImage=useRef(''),capture=useRef<ProviderPackageCapture|null>(null),providerContext=useRef(provider),[retry,setRetry]=useState(false);
  useLayoutEffect(()=>{providerContext.current=provider;draftImage.current=draft?JSON.stringify(draft):'';},[provider,draft]);
  const dirty = !!draft && (JSON.stringify(draft) !== baseline || !!review) || retry;
  useEffect(() => { dirtyRef.current = dirty; }, [dirty]);
  useEffect(() => {
    const unregister = registerUnsavedWork(() => dirtyRef.current);
    const unload = (event: BeforeUnloadEvent) => { if (dirtyRef.current && !contextIsSwitching()) event.preventDefault(); };
    const cancelRequest = () => { ++epoch.current; controller.current?.abort(); };
    const changed = () => { cancelRequest(); setPending(false); setSnapshot(null); setReview(null); setDraft(null);capture.current=null;setRetry(false);++editVersion.current; };
    window.addEventListener("beforeunload", unload); window.addEventListener("erpx:context-switch-started", changed); window.addEventListener("erpx:work-context-changed", changed);
    return () => { cancelRequest(); unregister(); window.removeEventListener("beforeunload", unload); window.removeEventListener("erpx:context-switch-started", changed); window.removeEventListener("erpx:work-context-changed", changed); };
  }, []);
  const company = companies.find(item => `${item.holdingId}:${item.id}` === selected);
  const current = snapshot?.grants.find(grant => grant.packageId === draft?.packageId);
  async function send<Result>(path: string, method: "GET" | "POST", input?: unknown, key?: string): Promise<Result | undefined> {
    controller.current?.abort(); const abort = new AbortController(); controller.current = abort;
    const ticket = ++epoch.current, generation = contextGeneration(); setPending(true); setError("");
    try {
      const response = await holdingFetch(path, { method, cache: "no-store", signal: abort.signal,
        ...(method === "POST" ? { headers: { "Content-Type": "application/json", ...(key ? { "Idempotency-Key": key } : {}) }, body: JSON.stringify(input) } : {}) });
      const body = await response.json();
      if (ticket !== epoch.current || generation !== contextGeneration() || abort.signal.aborted) return;
      if (!response.ok) { setMfa(body.error?.code === "FRESH_MFA_REQUIRED"); throw new Error(body.error?.message ?? "Unable to complete this request."); }
      return body.data as Result;
    } catch (cause) {
      if (ticket === epoch.current && !abort.signal.aborted) setError(cause instanceof Error ? cause.message : "Unable to complete this request.");
    } finally { if (ticket === epoch.current) setPending(false); }
  }
  async function refreshCompany(value: Company) {
    const data = await send<Snapshot>(`/api/v1/platform/commercial/packages?${new URLSearchParams({ holdingId: value.holdingId, companyId: value.id })}`, "GET");
    if (data?.companyId === value.id) setSnapshot(data);
  }
  function edit(values: Partial<Draft>) { ++editVersion.current; setDraft(previous => previous ? { ...previous, ...values } : null); setReview(null); setConfirmed(false); setError(""); }
  function openDraft(packageId: string, packageVersion: number, now: number, grant?: CompanyPackageGrant) {
    ++editVersion.current;capture.current=null;setRetry(false);
    const next: Draft = { packageId, packageVersion, state: grant?.state ?? "active", starts: utc(grant?.startsAt ?? now), ends: utc(grant?.endsAt ?? null), reason: "", evidence: "" };
    setDraft(next); setBaseline(JSON.stringify(next)); setReview(null); setConfirmed(false); setError(""); setReceipt(""); setMfa(false); setCode("");
  }
  async function preview(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!company || !snapshot || !draft || pending || capture.current || !event.currentTarget.reportValidity()) return;
    const request: PackageGrantBatch = { companyId: company.id, expectedRevision: snapshot.revision, idempotencyKey: crypto.randomUUID(), changes: [{ expectedVersion: current?.version ?? 0,
      grant: { companyId: company.id, packageId: draft.packageId, packageVersion: draft.packageVersion, state: draft.state,
        startsAt: current && utc(current.startsAt) === draft.starts ? current.startsAt : Date.parse(draft.starts + ":00Z"),
        endsAt: draft.ends ? current?.endsAt !== null && current?.endsAt !== undefined && utc(current.endsAt) === draft.ends ? current.endsAt : Date.parse(draft.ends + ":00Z") : null,
        version: (current?.version ?? 0) + 1, reason: draft.reason, evidence: draft.evidence } }] };
    const result = await send<Review>("/api/v1/platform/commercial/packages/preview", "POST", { holdingId: company.holdingId, request });
    if (result) {try{setReview(providerPackageReview(result,request));setConfirmed(false);}catch(cause){setError(cause instanceof Error?cause.message:"Could not verify package preview.");}}
  }
  async function apply() {
    if (!company || !canApply || pending || provider.stale || provider.pending || provider.retry || !capture.current&&(!review||!confirmed)) return;
    const submitted=capture.current??captureProviderPackageCommand(review!,company.holdingId,provider.seed.contextKey,editVersion.current,draftImage.current);
    if(submitted.contextKey!==provider.seed.contextKey){setError("The Provider context changed. Review before retrying this package command.");return;}
    capture.current=submitted;setRetry(true);
    const result=await send<unknown>("/api/v1/platform/commercial/packages/apply","POST",JSON.parse(submitted.body),submitted.key);
    if(result){try{const acknowledged=acknowledgeProviderPackageCommand(result,submitted,{contextKey:providerContext.current.seed.contextKey,editVersion:editVersion.current,draftImage:draftImage.current,blocked:providerContext.current.stale||providerContext.current.pending||providerContext.current.retry});setReceipt(submitted.key);capture.current=null;setRetry(false);if(acknowledged.clearDraft){setDraft(null);setReview(null);}await refreshCompany(company);}catch(cause){setError(cause instanceof Error?cause.message:"Could not verify package receipt.");}}
  }
  const available = snapshot?.packages.filter(definition => !snapshot.grants.some(grant => grant.packageId === definition.id)) ?? [];
  return <Stack>
    <Badge>Billing Sandbox</Badge>
    <Field appearance="standard" label={th ? "บริษัท" : "Company"}>
      <Select appearance="standard" value={selected} disabled={!ready || pending || !!draft} onChange={event => {
        const value = event.target.value; ++epoch.current; controller.current?.abort(); setSelected(value); setSnapshot(null); setPackageId(""); setError(""); setReceipt("");
        const chosen = companies.find(item => `${item.holdingId}:${item.id}` === value); if (chosen) void refreshCompany(chosen);
      }}><option value="">{th ? "เลือกบริษัท" : "Choose Company"}</option>{companies.map(item => <option key={`${item.holdingId}:${item.id}`} value={`${item.holdingId}:${item.id}`}>{item.holding} · {item.name} · {item.slug || item.id}</option>)}</Select>
    </Field>
    {pending && !draft && <p role="status">{th ? "กำลังโหลดแพ็กเกจ" : "Loading packages…"}</p>}
    {error && !draft && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    {receipt && <InlineNotice tone="success">{th ? "บันทึกแพ็กเกจแล้ว" : "Package change recorded."} {receipt}</InlineNotice>}
    {snapshot && company && <Panel appearance="standard"><Stack>
      <h2>{company.name}</h2>
      {!snapshot.grants.length && <p>{th ? "ยังไม่มีแพ็กเกจที่บันทึกไว้" : "No recorded packages."}</p>}
      {snapshot.grants.map(grant => <div key={grant.packageId} className="flex flex-wrap items-center gap-3">
        <strong>{grant.packageId}</strong><Badge>{grant.state}</Badge><span>v{grant.packageVersion} · {th ? "ครั้งที่" : "revision"} {grant.version}</span>
        <TimeText appearance="standard" value={grant.startsAt} />{grant.endsAt !== null && <>– <TimeText appearance="standard" value={grant.endsAt} /></>}
        <Button appearance="standard" variant="secondary" type="button" disabled={pending} onClick={() => openDraft(grant.packageId, grant.packageVersion, Date.now(), grant)}>{th ? "แก้ไข" : "Edit"}</Button>
      </div>)}
      <Field appearance="standard" label={th ? "เพิ่มแพ็กเกจ" : "Add package"}><Select appearance="standard" value={packageId} onChange={event => setPackageId(event.target.value)} disabled={pending}>
        <option value="">{th ? "เลือกแพ็กเกจ" : "Choose package"}</option>{available.map(definition => <option key={`${definition.id}:${definition.version}`} value={definition.id}>{definition.id} · v{definition.version}</option>)}
      </Select></Field>
      <div><Button appearance="standard" type="button" disabled={pending || !packageId} onClick={() => { const definition = available.find(item => item.id === packageId); if (definition) openDraft(definition.id, definition.version, Date.now()); }}>{th ? "ตรวจแพ็กเกจใหม่" : "Review new package"}</Button></div>
    </Stack></Panel>}
    <DraftFormDrawer appearance="standard" open={!!draft} dirty={dirty} dismissDisabled={pending} confirmationDescription={retry?"This command may already have completed. Discarding the local request does not undo a package change. Preview the current packages before continuing.":undefined} onClose={() => { setDraft(null); setCode(""); }} onDiscard={() => { ++editVersion.current;capture.current=null;setRetry(false);setDraft(null); setReview(null); setError(""); setCode(""); }} title={draft ? `${th ? "แพ็กเกจ" : "Package"} · ${draft.packageId}` : "Package"}>
      {draft && <Stack>
        {retry&&<InlineNotice tone="warning">{th?'ผลคำสั่งก่อนหน้ายังไม่ยืนยัน โปรดลองคำสั่งเดิมก่อนตรวจรายการใหม่':'The previous result is unconfirmed. Retry the captured command before reviewing another change.'}<Button appearance="standard" type="button" disabled={pending} onClick={()=>void apply()}>{th?'ลองคำสั่งเดิม':'Retry captured package command'}</Button></InlineNotice>}
        <form onSubmit={preview} data-work-dirty={dirty ? "true" : undefined}><Stack>
          <p>{th ? "เวลาเริ่มและสิ้นสุดเป็น UTC" : "Start and end times are UTC."}</p>
          <Field appearance="standard" label={th ? "สถานะ" : "State"}><Select appearance="standard" value={draft.state} disabled={pending} onChange={event => edit({ state: event.target.value as Draft["state"] })}>
            <option value="active">active</option><option value="draining" disabled={!current}>draining</option><option value="inactive" disabled={!current || current.state === "active"}>inactive</option>
          </Select></Field>
          <Field appearance="standard" label={th ? "เริ่ม (UTC)" : "Start (UTC)"} required><TextInput appearance="standard" type="datetime-local" required value={draft.starts} disabled={pending} onChange={event => edit({ starts: event.target.value })} /></Field>
          <Field appearance="standard" label={th ? "สิ้นสุด (UTC) · เว้นว่างหากไม่กำหนด" : "End (UTC) · leave empty for no expiry"}><TextInput appearance="standard" type="datetime-local" min={draft.starts} value={draft.ends} disabled={pending} onChange={event => edit({ ends: event.target.value })} /></Field>
          <Field appearance="standard" label={th ? "เหตุผล" : "Reason"} required><TextArea appearance="standard" required minLength={3} maxLength={1000} value={draft.reason} disabled={pending} onChange={event => edit({ reason: event.target.value })} /></Field>
          <Field appearance="standard" label={th ? "หลักฐานอ้างอิง" : "Evidence reference"} required><TextInput appearance="standard" required minLength={3} maxLength={1000} value={draft.evidence} disabled={pending} onChange={event => edit({ evidence: event.target.value })} /></Field>
          <div><Button appearance="standard" type="submit" clientManaged pending={pending} disabled={retry}>{th ? "ดูผลก่อนบันทึก" : "Preview change"}</Button></div>
        </Stack></form>
        {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
        {review && <Panel appearance="standard"><Stack>
          <h3>{th ? "ผลเปรียบเทียบ" : "Before and after"}</h3>
          <p><strong>{company?.holding} · {company?.name}</strong> · {review.request.companyId}</p>
          <p>{current ? `${current.state} · v${current.version}` : th ? "ยังไม่มีแพ็กเกจ" : "No grant"} → {review.request.changes[0].grant.state} · v{review.request.changes[0].grant.version}</p>
          {review.request.changes.map(change => <div key={change.grant.packageId}>
            <p><strong>{change.grant.packageId}</strong> · {th ? "เวอร์ชันแพ็กเกจ" : "Package version"} {change.grant.packageVersion}</p>
            <p>{th ? "เริ่ม" : "Start"}: <TimeText appearance="standard" value={change.grant.startsAt} /> · {th ? "สิ้นสุด" : "End"}: {change.grant.endsAt === null ? th ? "ไม่กำหนด" : "No expiry" : <TimeText appearance="standard" value={change.grant.endsAt} />}</p>
            <p>{th ? "เหตุผล" : "Reason"}: {change.grant.reason}</p><p>{th ? "หลักฐาน" : "Evidence"}: {change.grant.evidence}</p>
            <p>{th ? "ต้องมีแพ็กเกจ" : "Dependencies"}: {snapshot?.packages.find(definition => definition.id === change.grant.packageId && definition.version === change.grant.packageVersion)?.dependencies.join(", ") || "—"}</p>
            <details><summary>{th ? "ขอบเขตแพ็กเกจ" : "Package contents"}</summary>
              <p>{th ? "ผลิตภัณฑ์" : "Product"}: {snapshot?.packages.find(definition => definition.id === change.grant.packageId && definition.version === change.grant.packageVersion)?.visibleCapabilities.join(", ")}</p>
              <p>{th ? "ความสามารถที่รวมมา" : "Included capabilities"}: {snapshot?.packages.find(definition => definition.id === change.grant.packageId && definition.version === change.grant.packageVersion)?.includedCapabilities.join(", ") || "—"}</p>
            </details>
          </div>)}
          <p>{th ? "ตรวจทานได้ถึง" : "Review expires"} <TimeText appearance="standard" value={review.review.expiresAt} /></p>
          <Field appearance="standard" orientation="inline"><input type="checkbox" checked={confirmed} onChange={event => setConfirmed(event.target.checked)} />{th ? "ตรวจบริษัท แพ็กเกจ ช่วงเวลา และเหตุผลแล้ว" : "I reviewed the Company, package, period and reason."}</Field>
          {canApply ? <div><Button appearance="standard" type="button" pending={pending} disabled={!confirmed} onClick={() => void apply()}>{th ? "บันทึกตามผลตรวจ" : "Apply reviewed change"}</Button></div> : <p>{th ? "สิทธิ์นี้ตรวจดูได้ ผู้มีสิทธิ์จัดการต้องตรวจและบันทึกด้วยบัญชีของตน" : "This role can preview. A management operator must review and apply from their own account."}</p>}
        </Stack></Panel>}
        {mfa && <form onSubmit={async event => { event.preventDefault(); const result = await send("/api/v1/account/iam/mfa/verify", "POST", { code }); if (result) { setMfa(false); setCode(""); } }}><Stack>
          <Field appearance="standard" label={th ? "รหัสจากแอปยืนยันตัวตน" : "Authenticator code"}><TextInput appearance="standard" autoComplete="one-time-code" inputMode="numeric" pattern="[0-9]{6}" required maxLength={6} value={code} disabled={pending} onChange={event => setCode(event.target.value)} /></Field>
          <div><Button appearance="standard" type="submit" clientManaged pending={pending}>{th ? "ยืนยัน MFA" : "Verify MFA"}</Button></div>
        </Stack></form>}
      </Stack>}
    </DraftFormDrawer>
  </Stack>;
}
