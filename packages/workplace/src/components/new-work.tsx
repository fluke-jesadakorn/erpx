"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Button } from "@erpx/ui/components/foundation";

import { useHydrated } from "@erpx/ui/components/use-hydrated";
import Link from "@erpx/workplace/components/workspace-link";
import { useCallback, useEffect, useRef, useState, type ComponentProps } from "react";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import dynamic from "next/dynamic";
const ExpenseEditor = dynamic(() => import("@erpx/feature-people_payroll/components/expense-editor").then(module => module.ExpenseEditor), { ssr: false });
const LeaveEditor = dynamic(() => import("@erpx/feature-people_payroll/components/self-service/leave-editor").then(module => module.LeaveEditor), { ssr: false });
const RequestEditor = dynamic(() => import("@erpx/workplace/components/request-controls").then(module => module.RequestEditor), { ssr: false });
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
type Intake = {kind:"expense";options:ComponentProps<typeof ExpenseEditor>["options"]}|{kind:"leave";summary:{balances:ComponentProps<typeof LeaveEditor>["types"];setupMessage:string|null;purchase:ComponentProps<typeof LeaveEditor>["purchase"]};workContext:string;companyId:string;userId:string;assignmentId:string}|{kind:"purchase_request"|"service_request";options:ComponentProps<typeof RequestEditor>["options"]};
import { Dialog } from "@erpx/ui/components/dialog";
import { newWorkHref, type WorkCreation } from "@erpx/feature-work_projection/lib/work-cards";
import { requestActions } from "@erpx/feature-work_projection/lib/request-actions";
const descriptions: Record<string, [string, string]> = {
  ...Object.fromEntries(Object.entries(requestActions).map(([kind,action])=>[kind,[action.label,action.objective]])),
  task: ["Create a task", "Assign and track a piece of work, such as preparing a team report."],
  overtime: ["Request overtime", "Request authorization or submit hours you have worked."],
  equipment_request: [
    "Request equipment",
    "Request a laptop, tools or other equipment for your work.",
  ],
  stock_count: ["Count stock", "Record physical quantities for independent inventory review."],
  transfer: ["Move stock", "Transfer items between warehouses and confirm receipt."],
  conversation: ["Start a conversation", "Discuss the work with your team before creating a task."],
};
export function NewWork({
  base,
  creation,
  current,
  launcherOnly = false,
}: {
  base: string;
  creation: WorkCreation[];
  current: string;
  launcherOnly?: boolean;
}) {
  const ready = useHydrated();
  const [open, setOpen] = useState(false);
  const [selected,setSelected]=useState<WorkCreation|null>(null);
  const [intake,setIntake]=useState<Intake|null>(null);
  const [error,setError]=useState("");
  const [dirty,setDirty]=useState(false);
  const intakeGeneration=useRef(0),opener=useRef<HTMLElement|null>(null);
  const closeIntake=()=>{intakeGeneration.current++;setSelected(null);setIntake(null);requestAnimationFrame(()=>{const fallback=[...document.querySelectorAll<HTMLButtonElement>("button")].find(b=>b.textContent?.trim()==="New")??[...document.querySelectorAll<HTMLButtonElement>("button")].find(b=>b.textContent?.trim()==="Start");(opener.current?.isConnected?opener.current:fallback)?.focus();});};
  const launch=useCallback(async(item:WorkCreation)=>{
    opener.current=document.activeElement instanceof HTMLElement?document.activeElement:null;
    const generation=++intakeGeneration.current;
    setOpen(false);setSelected(item);setIntake(null);setError("");setDirty(false);
    try {const response=await workFetch(`/api/v1${base}/work/intake?kind=${encodeURIComponent(item.kind)}`);const body=await response.json();if(!response.ok)throw Error(body.error?.message??"Unable to open this request.");if(generation===intakeGeneration.current)setIntake(body.data);}
    catch(error){if(generation===intakeGeneration.current)setError(error instanceof Error?error.message:"Unable to open this request.");}
  },[base]);
  useEffect(()=>{
    if(!launcherOnly)return;
    const intercept=(event:MouseEvent)=>{
      if(event.defaultPrevented||event.button!==0||event.ctrlKey||event.metaKey||event.shiftKey||event.altKey)return;
      const link=(event.target as Element)?.closest?.("a");if(!link||link.target==="_blank"||link.hasAttribute("download"))return;
      const url=new URL(link.href,location.origin);if(url.origin!==location.origin)return;
      const kind=url.pathname===canonicalUiHref(`${base}/expenses/new`)?"expense":url.pathname===canonicalUiHref(`${base}/leave/new`)?"leave":url.pathname===canonicalUiHref(`${base}/requests/new`)&&!url.searchParams.has("from")?(url.searchParams.get("kind")==="service"?"service_request":url.searchParams.get("kind")==="purchase"?"purchase_request":null):null;
      if(!kind)return;
      event.preventDefault();void launch({kind,label:descriptions[kind][0],href:link.href});
    };
    document.addEventListener("click",intercept,true);return()=>document.removeEventListener("click",intercept,true);
  },[base,current,launcherOnly,launch]);
  return (
    <>
      {!launcherOnly && <Button appearance="standard"
        type="submit"
        variant="primary"

        disabled={!ready}
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        New
      </Button>}
      <Dialog appearance="standard" title="New work" open={open} onClose={() => setOpen(false)}>
        <div className="action-menu">
          {[false, true].map((operations) => (
            <section key={String(operations)}>
              <h2>{operations ? "Operations" : "Personal work & requests"}</h2>
              {creation
                .filter((item) => ["stock_count", "transfer"].includes(item.kind) === operations)
                .map((item) => (
                  <Link
                    key={item.kind}
                    aria-label={item.kind === "expense" ? "Expense claim" : item.label}
                    href={newWorkHref(base, item, current)}
                    onClick={event => {
                      if(event.defaultPrevented){setOpen(false);return;}
                      if(["expense","leave","purchase_request","service_request"].includes(item.kind)){event.preventDefault();void launch(item);}else setOpen(false);
                    }}
                  >
                    <strong>{descriptions[item.kind]?.[0] ?? item.label}</strong>
                    <small>{descriptions[item.kind]?.[1]}</small>
                  </Link>
                ))}
            </section>
          ))}
        </div>
      </Dialog>
      <DraftFormDrawer open={!!selected} title={selected?.label??"New request"} dirty={dirty} onClose={closeIntake} onDiscard={()=>{closeIntake();setDirty(false);}} discardLabel="Close and keep local draft" confirmationDescription="Your unsent details are kept on this device. Selected files must be chosen again.">
        <div className="request-page" onChangeCapture={()=>setDirty(true)}>
          {error&&<p role="alert">{error}</p>}
          {!intake&&!error&&<p role="status">Loading your request…</p>}
          {intake?.kind==="expense"&&<ExpenseEditor base={base} options={intake.options} initialOpen returnTo={current}/>}
          {intake?.kind==="leave"&&<>{intake.summary.setupMessage&&<p role="status">{intake.summary.setupMessage} Contact HR to link your engagement and policy.</p>}<LeaveEditor key={intake.workContext} base={base} workContext={intake.workContext} purchase={intake.summary.purchase} companyId={intake.companyId} userId={intake.userId} assignmentId={intake.assignmentId} types={intake.summary.balances} returnTo={current}/></>}
          {(intake?.kind==="purchase_request"||intake?.kind==="service_request")&&<RequestEditor base={base} kind={intake.kind==="purchase_request"?"purchase":"service"} options={intake.options} returnTo={current}/>}
        </div>
      </DraftFormDrawer>
    </>
  );
}
