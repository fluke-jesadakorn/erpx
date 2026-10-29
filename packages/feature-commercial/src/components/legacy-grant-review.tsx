"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button, Field, TextArea, TextInput, Select, Stack, Panel, InlineNotice, Badge } from "@erpx/ui/components/foundation";
import {useProviderContext} from '@erpx/feature-identity_access/components/provider-context-boundary';
import {providerLegacyPackageReview,captureProviderLegacyPackageCommand,acknowledgeProviderPackageCommand,type ProviderLegacyPackageReview,type ProviderPackageCapture} from '@erpx/server-commercial/public/rules/provider-package-command';
import { contextGeneration, contextIsSwitching, registerUnsavedWork } from "@erpx/feature-work_projection/lib/context-switch";
import styles from "@erpx/feature-commercial/components/legacy-grant-review.module.css";
import {Dialog} from "@erpx/ui/components/dialog";
import { useHydrated } from "@erpx/ui/components/use-hydrated";

type Company = { id: string; name: string; slug?: string; holdingId: string; holding: string };
type Preview = ProviderLegacyPackageReview;
export function LegacyGrantReview({ companies, canApply, language }: { companies: Company[]; canApply: boolean; language: "en" | "th" }) {
  const hydrated = useHydrated(),{state:provider}=useProviderContext();
  const th = language === "th", [selected, setSelected] = useState(""), [reason, setReason] = useState(""), [evidence, setEvidence] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null), [pending, setPending] = useState(false), [error, setError] = useState(""), [receipt, setReceipt] = useState("");
  const [code, setCode] = useState(""), [mfa, setMfa] = useState(false), [confirmed, setConfirmed] = useState(false);
  const [roleQuery, setRoleQuery] = useState(""), [rolePage, setRolePage] = useState(0);
  const [retry,setRetry]=useState(false),[discarding,setDiscarding]=useState(false),capture=useRef<ProviderPackageCapture|null>(null),editVersion=useRef(0),draftImage=useRef(''),providerContext=useRef(provider);
  useLayoutEffect(()=>{providerContext.current=provider;draftImage.current=JSON.stringify({selected,reason,evidence});},[provider,selected,reason,evidence]);
  const dirty = !!reason || !!evidence || !!preview || retry;
  const dirtyRef = useRef(dirty), controller = useRef<AbortController | null>(null), epoch = useRef(0), form = useRef<HTMLFormElement>(null);
  useEffect(() => { dirtyRef.current = dirty; }, [dirty]);
  useEffect(() => {
    const unregister = registerUnsavedWork(() => dirtyRef.current);
    const unload = (event: BeforeUnloadEvent) => { if (dirtyRef.current && !contextIsSwitching()) event.preventDefault(); };
    const changed = () => { epoch.current++; controller.current?.abort(); setPreview(null); capture.current=null;setRetry(false);++editVersion.current;setConfirmed(false); setPending(false); setError(th ? "สิทธิ์หรือบริบทเปลี่ยน โปรดโหลดหน้าใหม่" : "Access or context changed. Reload before continuing."); };
    window.addEventListener("beforeunload", unload); window.addEventListener("erpx:work-context-changed", changed); window.addEventListener("erpx:context-switch-started", changed);
    return () => { unregister(); controller.current?.abort(); window.removeEventListener("beforeunload", unload); window.removeEventListener("erpx:work-context-changed", changed); window.removeEventListener("erpx:context-switch-started", changed); };
  }, [th]);
  function invalidate() { ++editVersion.current;epoch.current++; controller.current?.abort(); setPreview(null); setConfirmed(false); setReceipt(""); setError(""); setPending(false); setRoleQuery(""); setRolePage(0); }
  async function send(path: string, input: unknown, key?: string) {
    controller.current?.abort(); const abort = new AbortController(); controller.current = abort;
    const current = ++epoch.current, generation = contextGeneration(); setPending(true); setError("");
    try {
      const response = await holdingFetch(path, { method: "POST", headers: { "Content-Type": "application/json", ...(key ? { "Idempotency-Key": key } : {}) }, body: JSON.stringify(input), signal: abort.signal });
      const result = await response.json();
      if (current !== epoch.current || generation !== contextGeneration() || abort.signal.aborted) return;
      if (!response.ok) { setMfa(result.error?.code === "FRESH_MFA_REQUIRED"); throw new Error(result.error?.message ?? "Unable to complete this request."); }
      return result.data;
    } catch (failure) { if (current === epoch.current && !abort.signal.aborted) setError(failure instanceof Error ? failure.message : "Unable to complete this request."); }
    finally { if (current === epoch.current) setPending(false); }
  }
  const company = companies.find(item => `${item.holdingId}:${item.id}` === selected);
  async function apply(){
    if(!company||!canApply||pending||provider.stale||provider.pending||provider.retry||!capture.current&&(!preview?.request||!preview.review||!confirmed))return;
    const submitted=capture.current??captureProviderLegacyPackageCommand(preview!,company.holdingId,provider.seed.contextKey,editVersion.current,draftImage.current);
    if(submitted.contextKey!==provider.seed.contextKey){setError('The Provider context changed. Review before retrying this migration.');return;}
    capture.current=submitted;setRetry(true);const data=await send('/api/v1/platform/commercial/legacy-grants/apply',JSON.parse(submitted.body),submitted.key);
    if(data){try{const acknowledged=acknowledgeProviderPackageCommand(data,submitted,{contextKey:providerContext.current.seed.contextKey,editVersion:editVersion.current,draftImage:draftImage.current,blocked:providerContext.current.stale||providerContext.current.pending||providerContext.current.retry});setReceipt(submitted.key);capture.current=null;setRetry(false);if(acknowledged.clearDraft){setPreview(null);setReason('');setEvidence('');setConfirmed(false);}}catch(cause){setError(cause instanceof Error?cause.message:'Could not verify the migration receipt.');}}
  }
  const roles = preview?.comparison.roles.filter(role => `${role.roleId} ${role.scopeKind} ${role.scopeNodeId ?? ""} ${role.assignmentId}`.toLowerCase().includes(roleQuery.toLowerCase())) ?? [];
  const page = Math.min(rolePage, Math.max(0, Math.ceil(roles.length / 20) - 1));
  return <Stack data-work-dirty={dirty ? "true" : undefined}>
    <p>{th ? "เปรียบเทียบแพ็กเกจจากขอบเขตงานเดิมก่อนบันทึก สิทธิ์ผู้ใช้ ขอบเขต rollout และราคายังคงเดิม" : "Review package coverage from existing Company access before applying. User authority, rollout and prices remain unchanged."}</p>
    <form ref={form} onSubmit={async event => { event.preventDefault(); if (!company || capture.current || !form.current?.reportValidity()) return;
      setReceipt(""); setConfirmed(false); const data = await send("/api/v1/platform/commercial/legacy-grants/preview", { holdingId: company.holdingId, companyId: company.id, reason, evidence }); if (data) {try{setPreview(providerLegacyPackageReview(data,{companyId:company.id,reason,evidence}));setRoleQuery('');setRolePage(0);}catch(cause){setError(cause instanceof Error?cause.message:'Could not verify the migration preview.');}}
    }}><Stack>
      <Field appearance="standard" label={th ? "บริษัท" : "Company"} required><Select appearance="standard" required value={selected} disabled={!hydrated || pending || retry} onChange={event => { invalidate(); setSelected(event.target.value); }}>
        <option value="">{th ? "เลือกบริษัท" : "Choose a Company"}</option>{companies.map(item => <option key={`${item.holdingId}:${item.id}`} value={`${item.holdingId}:${item.id}`}>{item.holding} · {item.name} · {item.slug || item.id}</option>)}
      </Select></Field>
      <Field appearance="standard" label={th ? "เหตุผล" : "Reason"} required><TextArea appearance="standard" required disabled={!hydrated || pending} minLength={3} maxLength={1000} value={reason} onChange={event => { invalidate(); setReason(event.target.value); }} /></Field>
      <Field appearance="standard" label={th ? "หลักฐานอ้างอิง" : "Evidence reference"} required><TextInput appearance="standard" required disabled={!hydrated || pending} minLength={3} maxLength={1000} value={evidence} onChange={event => { invalidate(); setEvidence(event.target.value); }} /></Field>
      <div><Button appearance="standard" type="submit" clientManaged pending={pending} disabled={!company || retry}>{th ? "ตรวจรายการเปลี่ยนแปลง" : "Preview migration"}</Button></div>
    </Stack></form>
    {retry&&<InlineNotice tone="warning">{th?'ผลคำสั่งก่อนหน้ายังไม่ยืนยัน โปรดลองคำสั่งเดิมก่อนตรวจรายการใหม่':'The previous result is unconfirmed. Retry the captured migration before reviewing another change.'}<Button appearance="standard" type="button" disabled={pending} onClick={()=>void apply()}>{th?'ลองคำสั่งเดิม':'Retry captured migration'}</Button><Button appearance="standard" variant="secondary" type="button" disabled={pending} onClick={()=>setDiscarding(true)}>{th?'ทิ้งคำขอในเครื่อง':'Discard captured request'}</Button></InlineNotice>}
    <Dialog appearance="standard" open={discarding} title={th?'ทิ้งคำขอในเครื่อง?':'Discard captured request?'} onClose={()=>setDiscarding(false)}><p>{th?'คำสั่งอาจบันทึกแล้ว การทิ้งคำขอในเครื่องไม่ย้อนการเปลี่ยนแพ็กเกจ โปรดตรวจรายการใหม่ก่อนทำต่อ':'This command may already have completed. Discarding the local request does not undo a package change. Preview the current packages before continuing.'}</p><Button appearance="standard" variant="secondary" type="button" onClick={()=>setDiscarding(false)}>{th?'เก็บคำขอเดิม':'Keep captured request'}</Button><Button appearance="standard" variant="danger" type="button" onClick={()=>{++editVersion.current;capture.current=null;setRetry(false);setDiscarding(false);setPreview(null);setConfirmed(false);setError('');}}>{th?'ทิ้งคำขอในเครื่อง':'Discard captured request'}</Button></Dialog>
    {error && <InlineNotice tone="danger">{error}</InlineNotice>}
    {receipt && <InlineNotice tone="success">{th ? "บันทึกแล้ว เลขอ้างอิง: " : "Applied. Receipt: "}{receipt}</InlineNotice>}
    {preview && <Panel appearance="standard" className={styles.comparison}><Stack><h2>{th ? "ผลเปรียบเทียบ" : "Reviewed comparison"}</h2>
      <p>{preview.comparison.before.length} → {preview.comparison.after.length} {th ? "แพ็กเกจ" : "packages"} · {preview.comparison.roles.length} {th ? "สิทธิ์ที่มีอยู่" : "existing assignments"}</p>
      <div className="flex flex-wrap gap-2">{preview.comparison.requiredPackages.map(id => <Badge key={id}>{id}</Badge>)}</div>
      <Field appearance="standard" label={th ? "ค้นหาบทบาทและขอบเขต" : "Find role or scope"}><TextInput appearance="standard" type="search" value={roleQuery} onChange={event => { setRoleQuery(event.target.value); setRolePage(0); }} /></Field>
      {roles.slice(page * 20, page * 20 + 20).map(role => <details key={role.assignmentId}><summary>{role.roleId} · {role.scopeKind} · {role.assignmentId}</summary>
        <p>{th ? "โมดูลก่อนและหลัง" : "Modules before and after"}: {role.modulesBefore.join(", ") || "—"}</p><p>{th ? "ช่องว่างแพ็กเกจหลังย้าย" : "Missing purchased coverage after migration"}: {role.missingPurchasedCoverage.join(", ") || "0"}</p>
        {role.compatibilityLabels.length > 0 && <p>{th ? "labels เดิมที่มีหลักฐานสิทธิ์ปัจจุบันแล้ว" : "Legacy labels with current access evidence"}: {role.compatibilityLabels.join(", ")}</p>}
        {role.compatibilityEvidence.map(proof => <p key={proof.label}>{proof.label} → {proof.modules.join(", ")} · {th ? "policy สิทธิ์ที่เผยแพร่ปัจจุบัน" : "current published permission policy"}</p>)}
      </details>)}
      <nav aria-label={th ? "หน้าผลเปรียบเทียบสิทธิ์" : "Assignment comparison pages"} className={styles.pagination}>
        <Button appearance="standard" variant="secondary" disabled={page === 0} onClick={() => setRolePage(page - 1)}>{th ? "ก่อนหน้า" : "Previous assignments"}</Button>
        <span>{roles.length ? page * 20 + 1 : 0}–{Math.min(roles.length, page * 20 + 20)} / {roles.length}</span>
        <Button appearance="standard" variant="secondary" disabled={(page + 1) * 20 >= roles.length} onClick={() => setRolePage(page + 1)}>{th ? "ถัดไป" : "Next assignments"}</Button>
      </nav>
      <InlineNotice tone="info">{th ? "ผลนี้ตรวจความครอบคลุมของแพ็กเกจ การตรวจรับ workflow และ shadow enforcement ยังแยกต่างหาก" : "This preview checks package coverage. Owner workflow acceptance and shadow enforcement require their own evidence."}</InlineNotice>
      {preview.comparison.blockers.length > 0 && <InlineNotice tone="warning">{th ? "แพ็กเกจเดิมต้องตรวจทานเพิ่มเติม" : "Existing packages require explicit review"}: {preview.comparison.blockers.map(item => item.packageId).join(", ")}</InlineNotice>}
      {!preview.comparison.changes.length && <InlineNotice tone="info">{th ? "ไม่มีแพ็กเกจที่ต้องเพิ่ม" : "No additional packages are required."}</InlineNotice>}
      {preview.review && preview.request && canApply && <>
        <Field appearance="standard" orientation="inline"><input type="checkbox" checked={confirmed} onChange={event => setConfirmed(event.target.checked)} />{th ? "ตรวจบริษัท แพ็กเกจ และสิทธิ์ข้างต้นแล้ว" : "I reviewed the Company, packages and existing authority above."}</Field>
        <div><Button appearance="standard" pending={pending} disabled={!confirmed} onClick={()=>void apply()}>{th ? "บันทึกการย้ายแพ็กเกจ" : "Apply reviewed migration"}</Button></div>
      </>}
    </Stack></Panel>}
    {mfa && <form onSubmit={async event => { event.preventDefault(); const data = await send("/api/v1/account/iam/mfa/verify", { code }); if (data) { setMfa(false); setCode(""); } }}><Stack>
      <Field appearance="standard" label={th ? "รหัสจากแอปยืนยันตัวตน" : "Authenticator code"}><TextInput appearance="standard" autoComplete="one-time-code" inputMode="numeric" pattern="[0-9]{6}" required maxLength={6} value={code} onChange={event => setCode(event.target.value)} /></Field>
      <div><Button appearance="standard" type="submit" clientManaged pending={pending}>{th ? "ยืนยัน MFA" : "Verify MFA"}</Button></div>
    </Stack></form>}
  </Stack>;
}
