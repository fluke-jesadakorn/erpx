"use client";
import Link from "next/link";
import { useContext, type ComponentProps } from "react";
import { useRouter } from "next/navigation";
import { recordTarget, recordDrawerHref, recordCanonicalHref, type RecordTarget } from "@/lib/record-drawer";
import { RecordScope } from "./record-context";

export function RecordLink({ record, onClick, children, ...props }: ComponentProps<typeof Link> & { record?: RecordTarget }) {
  const base = useContext(RecordScope), router = useRouter();
  const target = record ?? (typeof props.href === "string" && base ? recordTarget(props.href, base) : null);
  return <Link {...props} href={record && base ? recordCanonicalHref(base, record) : props.href} data-record-link={target ? "true" : undefined} title={target ? "View details" : props.title} onClick={event => {
    onClick?.(event);
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || props.target === "_blank") return;
    const dirty = document.querySelector('[data-record-drawer-content][data-record-dirty="true"]');
    if(!target){
      if(dirty&&event.currentTarget.closest('[data-record-drawer-content]')&&typeof props.href==='string'&&!props.href.startsWith('/api/')){
        const href=new URL(props.href,window.location.href);
        if(href.origin===window.location.origin){event.preventDefault();window.dispatchEvent(new CustomEvent('erpx:record-navigate',{detail:{href:href.pathname+href.search+href.hash}}));}
      }
      return;
    }
    if (window.getSelection()?.toString()) {event.preventDefault();return;}
    event.preventDefault();
    event.currentTarget.setAttribute("data-record-trigger", "true");
    if (dirty) { window.dispatchEvent(new CustomEvent("erpx:record-navigate", { detail: { target } })); return; }
    router.push(recordDrawerHref(window.location.href, target), { scroll: false });
  }}>{children}</Link>;
}
