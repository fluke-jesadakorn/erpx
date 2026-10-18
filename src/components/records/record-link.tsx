"use client";
import {useProductNavigationScope} from "@/components/workplace/product-link-context";
import {productNavigationHref,productHrefString} from "@/lib/work_projection/product-navigation";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import Link from "@/components/workplace/workspace-link";
import { useContext, type ComponentProps } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import {usePathname} from 'next/navigation';
import {useSearchParams} from '@/lib/shared/destination-search';
import { recordTarget, recordCanonicalHref, recordDrawerHref, type RecordTarget } from "@/lib/records/record-drawer";
import { RecordScope } from "./record-context";

export function RecordLink({ record, onClick, children, ...props }: ComponentProps<typeof Link> & { record?: RecordTarget }) {
  const base = useContext(RecordScope), router = useRouter(),query=useSearchParams(),pathname=usePathname();
  const parsed = record ?? (typeof props.href === "string" && base ? recordTarget(props.href, base) : null);
  const target=parsed?.kind==='order'?{...parsed,args:{...parsed.args,...(query.get('view')==='purchase'?{view:'purchase'}:query.get('type')==='quote'||query.get('view')==='quote'?{view:'quote'}:{})}}:parsed;
  const scope=useProductNavigationScope(),rawHref=productHrefString(props.href);
  let crossHref=rawHref===null?null:canonicalUiHref(rawHref);
  if(record&&target&&base){const full=new URL(recordCanonicalHref(base,target),"https://record.invalid"),original=rawHref?new URL(rawHref,"https://record.invalid"):null;if(original){for(const [key,value] of original.searchParams)if(!Object.hasOwn(record.args??{},key))full.searchParams.append(key,value);full.hash=original.hash;}crossHref=full.pathname+full.search+full.hash;}
  const crossing=crossHref!==null&&productNavigationHref(crossHref,scope).handoff;
  return <Link {...props} prefetch={target||crossing ? false : props.prefetch} href={crossing?crossHref!:target && base ? recordDrawerHref(pathname+(query.size?'?'+query:''), target) : typeof props.href === "string" ? canonicalUiHref(props.href) : props.href} data-record-link={target ? "true" : undefined} title={target ? "View details" : props.title} onClick={event => {
    onClick?.(event);
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || props.target === "_blank") return;
    if(crossing){if(window.getSelection()?.toString())event.preventDefault();return;}
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
