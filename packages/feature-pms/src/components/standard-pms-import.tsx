"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {FrontLocalizedTree,useFrontLanguage} from "@erpx/front/components/front-language";
import { Button, Card, Field, InlineNotice, TextArea, TextInput } from "@erpx/ui/components/foundation";
import styles from "@erpx/feature-pms/components/standard-pms-import.module.css";

import { useCallback, useLayoutEffect, useRef, useState } from "react";
import {usePathname} from 'next/navigation';
import {useSearchParams} from '@erpx/feature-shared/lib/destination-search';
import {installPmsPendingNavigation} from '@erpx/feature-pms/lib/pending-navigation';
import {createPmsImportDraft} from '@erpx/feature-pms/lib/import-draft';
import Link from "@erpx/workplace/components/workspace-link";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import {createPmsImportCommands,type PmsImportAction,type PmsImportBatch,type PmsImportSource} from "@erpx/feature-pms/lib/import-command";
export function StandardPmsImport({
  api,
  base,
  propertyId,
  propertyVersion,
  businessDate,
  initial,
  userId,
  companyId,
  canPrepare,
  canApprove,
}: {
  api: string;
  base: string;
  propertyId: string;
  propertyVersion: number;
  businessDate: string;
  initial: PmsImportBatch[];
  userId: string;
  companyId?: string;
  canPrepare: boolean;
  canApprove: boolean;
}) {
  const language=useFrontLanguage(),pathname=usePathname(),query=useSearchParams().toString();
  const [owner,setOwner]=useState({api,propertyId,userId,companyId}),[draft]=useState(createPmsImportDraft);
  const isCurrentOwner=owner.api===api&&owner.propertyId===propertyId&&owner.userId===userId&&owner.companyId===companyId;
  const router = useRouter(),
    [batches, setBatches] = useState(initial),
    [body, setBody] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [uncertain, setUncertain] = useState(false),
    [journal, setJournal] = useState("");
  const source=useRef<PmsImportSource|null>(null),attempts=useRef<ReturnType<typeof createPmsImportCommands>|null>(null),flight=useRef(false);
  const pendingCommand=useCallback(()=>flight.current||Boolean(attempts.current?.hasPending()),[]),
    localDraftPending=useCallback(()=>draft.isDirty(),[draft]);
  useLayoutEffect(()=>installPmsPendingNavigation(pendingCommand,()=>window.confirm(language==='th'
    ? 'แบบร่างหรือคำขอนำเข้าเดิมยังไม่สิ้นสุด หากส่งแล้วอาจบันทึกสำเร็จ ออกจากหน้านี้และปิดการกู้คืนหรือไม่?'
    : 'Your import draft or submitted action is still open. A submitted action may already be saved. Leave this view and close its local recovery?'),localDraftPending),[pendingCommand,localDraftPending,language]);
  useLayoutEffect(()=>{
    source.current=isCurrentOwner?{api,propertyId,companyId,propertyVersion,href:new URL(pathname+(query?'?'+query:''),window.location.origin).href}:null;
    attempts.current??=createPmsImportCommands(()=>source.current);
    return ()=>{source.current=null;};
  },[api,propertyId,companyId,propertyVersion,isCurrentOwner,pathname,query]);
  function editBody(value:string){draft.editBody(value);setBody(value);}
  function editJournal(value:string){draft.editJournal(value);setJournal(value);}
  function adoptCurrentOwner(){
    if(flight.current||attempts.current?.hasPending())return;
    if(draft.isDirty()&&!window.confirm(language==='th'?'ทิ้งแบบร่างนำเข้าเดิมและใช้โรงแรมที่เลือกปัจจุบันหรือไม่?':'Discard the previous local import draft and use the currently selected hotel?'))return;
    draft.discard();setBody('');setJournal('');setError('');setBatches(initial);setOwner({api,propertyId,userId,companyId});
  }
  const template = {
    propertyId,
    expectedVersion: propertyVersion,
    cutoverDate: businessDate,
    currency: "THB",
    source: "legacy-cutover",
    roomTypes: [],
    rooms: [],
    rates: [],
    guests: [],
    reservations: [],
  };
  async function run(action?:PmsImportAction,input?:unknown) {
    const command=attempts.current;
    if (busy||flight.current||!command||!isCurrentOwner) return;
    flight.current=true;
    setBusy(true);
    setError("");
    try {
      const result=action?await command.run(action,input,draft.capture(action)):await command.retry();
      if(!result)return;
      if(!result.acknowledge())throw Error("The import acknowledgement is incomplete. Retry the submitted import action.");
      setBatches(result.batches);
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Please try again.");
    } finally {
      flight.current=false;
      setUncertain(command.hasPending());
      setBusy(false);
    }
  }
  return (
    <FrontLocalizedTree><div className={styles.workspace}>
      <p>
        Preview rooms, rates, guests and open stays before saving. Amounts are THB satang. Cutover
        must match the hotel&apos;s business date: <strong>{businessDate}</strong>.
      </p>
      <p>
        Finance reviews the guest opening totals against an independently posted opening journal.
        Applying the approved batch commits all records together. An interrupted attempt can be
        retried with the same key.
      </p>
      <p>
        <Link href={canonicalUiHref(`${base}/finance/accounting`)}>Opening journal in Finance →</Link> ·{" "}
        <Link href={canonicalUiHref(`${base}/finance/hotels?property=${propertyId}`)}>Hotel reconciliation →</Link> ·{" "}
        <a href="/pms-cutover-example.json" download>
          Example JSON
        </a>
      </p>
      {!isCurrentOwner&&<InlineNotice tone="warning"><p>{language==='th'?'แบบร่างหรือคำขอนำเข้าเดิมเป็นของโรงแรมหรือบริบทก่อนหน้า กลับไปยังบริบทเดิมเพื่อตรวจผล':'The retained import draft or action belongs to the previous hotel or work context. Return to that source to recover it.'}</p>{!busy&&!uncertain&&<Button appearance="standard" variant="secondary" onClick={adoptCurrentOwner}>{language==='th'?'ใช้โรงแรมที่เลือกปัจจุบัน':'Use currently selected hotel'}</Button>}</InlineNotice>}
      {isCurrentOwner&&error && (
        <InlineNotice tone="danger" role="alert">
          {error}
        </InlineNotice>
      )}
      {(busy||uncertain)&&<p role="status">The submitted import action is not yet confirmed in this workspace. <Button appearance="standard" type="button" variant="secondary" disabled={busy||!isCurrentOwner} onClick={()=>void run()}>Retry submitted import action</Button></p>}
      {isCurrentOwner&&(canPrepare||body.length>0) && (
        <Card appearance="standard" className={styles.section}>
          <h2>Preview / ตรวจข้อมูลก่อนนำเข้า</h2>
          <Button appearance="standard"
            variant="secondary"
            type="button"
            disabled={busy||uncertain||!canPrepare}
            onClick={() => editBody(JSON.stringify(template, null, 2))}
          >
            Start with this hotel&apos;s template
          </Button>
          <Field appearance="standard" className={styles.field}>
            Import JSON
            <TextArea appearance="standard"
              aria-label="Import JSON"
              rows={12}
              value={body}
              disabled={busy||!canPrepare}
              onChange={(event) => editBody(event.target.value)}
            />
          </Field>
          <Button appearance="standard"
            type="submit"
            variant="primary"
            disabled={busy || uncertain || !canPrepare || !body.trim()}
            onClick={() => {
              try {
                void run("preview", JSON.parse(body));
              } catch {
                setError("Enter valid JSON.");
              }
            }}
          >
            Preview only
          </Button>
        </Card>
      )}
      {isCurrentOwner&&(canApprove||journal.length>0) && (
        <Field appearance="standard" className={styles.field}>
          Posted opening journal ID / สมุดรายวันยอดยกมา
          <TextInput appearance="standard"
            value={journal}
            disabled={busy||!canApprove}
            onChange={(event) => editJournal(event.target.value)}
            placeholder="Leave empty only for a batch with zero opening balances"
          />
        </Field>
      )}
      {isCurrentOwner&&batches.map((batch) => (
        <Card appearance="standard" className={styles.section} key={batch.id}>
          <div className={styles.batchHeader}>
            <h2>
              {batch.id.slice(-8)} · {batch.state}
            </h2>
            <span>Version {batch.version}</span>
          </div>
          <p>
            {Object.entries(batch.report.counts)
              .map(([kind, count]) => `${kind}: ${count}`)
              .join(" · ")}
          </p>
          <p>
            Opening: receivable {(batch.report.totals.due / 100).toFixed(2)} · advance{" "}
            {(batch.report.totals.advance / 100).toFixed(2)} · deposit{" "}
            {(batch.report.totals.deposit / 100).toFixed(2)} THB · prepaid tax{" "}
            {(batch.report.totals.advanceTax / 100).toFixed(2)}
          </p>
          {batch.report.errors.length > 0 && (
            <ul>
              {batch.report.errors.map((row, index) => (
                <li key={index}>
                  <strong>{row.row}</strong> · {row.message} ({row.code})
                </li>
              ))}
            </ul>
          )}
          {batch.opening_journal_id && (
            <Link href={canonicalUiHref(`${base}/finance/journals/${batch.opening_journal_id}`)}>
              Approved opening journal →
            </Link>
          )}
          {batch.state === "preview" && canApprove && batch.created_by !== userId && (
            <Button appearance="standard"
              type="submit"
              variant="primary"
              disabled={busy || uncertain || !batch.report.canApprove}
              onClick={() =>
                void run("approve", {
                  id: batch.id,
                  expectedVersion: batch.version,
                  openingJournalId: journal.trim() || null,
                })
              }
            >
              Finance: approve this preview
            </Button>
          )}
          {batch.state === "approved" && canPrepare && (
            <Button appearance="standard"
              type="submit"
              variant="primary"
              disabled={busy||uncertain}
              onClick={() => void run("apply", { id: batch.id, expectedVersion: batch.version })}
            >
              Apply approved batch / นำเข้าชุดที่อนุมัติ
            </Button>
          )}
          {batch.state === "applied" && (
            <p role="status">
              Applied successfully. Original keys and the approved opening ledger remain linked.
            </p>
          )}
        </Card>
      ))}
      {isCurrentOwner&&!batches.length && <p>No import batches for this hotel yet.</p>}
    </div></FrontLocalizedTree>
  );
}
