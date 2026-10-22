"use client";
import Link from "@erpx/workplace/components/workspace-link";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { type HTMLAttributes } from "react";
import { RecordDrawer } from "@erpx/feature-records/components/record-drawer";

/** A Front owner keeps its actual record view and commands in the current scope. */
export function RecordSurface({ recordId, children, as: Tag = "div", ...props }: HTMLAttributes<HTMLElement> & { recordId: string; as?: "div" | "article" }) {
  const query = useSearchParams();
  const open = query.get("detailKind") === "pms_record" && query.get("detailId") === recordId;
  const search = new URLSearchParams(query.toString());
  search.set("detailKind", "pms_record"); search.set("detailId", recordId);
  for (const key of ["detailArgs","detailPage","detailMode"]) search.delete(key);
  const full = new URLSearchParams(query.toString());
  for (const key of ["detailKind","detailId","detailArgs","detailPage"]) full.delete(key);
  full.set("record", recordId);
  return <Tag {...props}>
    {!open && children}
    <Link href={`?${search}`} scroll={false} data-record-link="true" onClick={event => {
      if(event.button!==0||event.metaKey||event.ctrlKey||event.altKey||event.shiftKey)return;
      if(window.getSelection()?.toString()){event.preventDefault();return;}
      event.currentTarget.setAttribute("data-record-trigger", "true");
      if(document.querySelector('[data-record-drawer-content][data-record-dirty="true"]')) {event.preventDefault();window.dispatchEvent(new CustomEvent("erpx:record-navigate",{detail:{href:`${window.location.pathname}?${search}`}}));}
    }}>View details</Link>
    {open && <RecordDrawer title="Property record" fullHref={`?${full}`}>{children}</RecordDrawer>}
  </Tag>;
}
