"use client";
import { useHydrated } from "@/components/ui/use-hydrated";
import Link from "next/link";
import { useState } from "react";
import { Dialog } from "@/components/ui/dialog";
import { newWorkHref, type WorkCreation } from "@/lib/work-cards";
const descriptions: Record<string, [string,string]> = {
 task:["Create a task","Assign and track a piece of work, such as preparing a team report."],
 overtime:["Request overtime","Request authorization or submit hours you have worked."],
 leave:["Request time off","Choose dates and a handover plan for annual leave."],
 expense:["Claim an expense","Attach a receipt and request reimbursement for a business expense."],
 service_request:["Get help","Ask a team for help, such as fixing a laptop or workplace issue."],
 equipment_request:["Request equipment","Request a laptop, tools or other equipment for your work."],
 purchase_request:["Request a purchase","Describe what you need and an estimated cost for review."],
 stock_count:["Count stock","Record physical quantities for independent inventory review."],
 transfer:["Move stock","Transfer items between warehouses and confirm receipt."],
 conversation:["Start a conversation","Discuss the work with your team before creating a task."],
};
export function NewWork({
  base,
  creation,
  current,
}: {
  base: string;
  creation: WorkCreation[];
  current: string;
}) {
  const ready = useHydrated();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        className="button"
        disabled={!ready}
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        New
      </button>
      <Dialog title="New work" open={open} onClose={() => setOpen(false)}>
        <div className="action-menu">
          {[false,true].map(operations => <section key={String(operations)}><h2>{operations ? "Operations" : "Personal work & requests"}</h2>{creation.filter(item => ["stock_count","transfer"].includes(item.kind) === operations).map(item => <Link key={item.kind} href={newWorkHref(base,item,current)} onClick={() => setOpen(false)}><strong>{descriptions[item.kind]?.[0] ?? item.label}</strong><small>{descriptions[item.kind]?.[1]}</small></Link>)}</section>)}
        </div>
      </Dialog>
    </>
  );
}
