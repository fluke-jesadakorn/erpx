"use client";

import { useRef, useState } from "react";
import { usePathname } from "next/navigation";
import { ChevronDown } from "lucide-react";
import { Button, TextInput } from "@erpx/ui/components/foundation";
import { Dialog } from "@erpx/ui/components/dialog";
import Link from "@erpx/workplace/components/workspace-link";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { canonicalUiHref, destinationForPath, workspacePathParts } from "@erpx/feature-work_projection/lib/destination-registry";
import { navigationWorkEntry, workEntries, workEntryRoots, navigationTopicHref as topicHref } from "@erpx/feature-work_projection/lib/navigation-presentation";
import { workplaceNavigation, type NavigationContext } from "@erpx/feature-work_projection/lib/workplace-navigation";
import styles from "@erpx/workplace/components/section-workspace.module.css";

export type TopicLink = { id: string; title: string; href: string; description?: string; group?: string };

/** A normal link picker preserves URL history, SSO and the shell's existing
 * pending-request/dirty-draft guard. Picking never issues a business command. */
export function TopicNavigation({ label, items, selected, language = "en" }: {label:string; items:readonly TopicLink[]; selected?:string; language?:"th"|"en"}) {
  const [open,setOpen] = useState(false), [search,setSearch] = useState("");
  const trigger = useRef<HTMLButtonElement>(null), th = language === "th";
  const current = items.find(item => item.id === selected);
  if (items.length < 2) return null;
  return <div className={styles.topicPicker}>
    <Button ref={trigger} appearance="standard" variant="secondary" type="button" aria-haspopup="dialog" aria-expanded={open} aria-label={`${label}: ${current?.title ?? (th ? "เลือกหัวข้อ" : "Choose topic")}`} onClick={() => {setSearch("");setOpen(true);}}>
      <span>{label}</span><strong>{current?.title ?? (th ? "เลือกหัวข้อ" : "Choose topic")}</strong><ChevronDown size={16} aria-hidden="true" />
    </Button>
    <Dialog appearance="standard" title={label} open={open} onClose={() => setOpen(false)} finalFocus={() => trigger.current}>
      {items.length > 6 && <TextInput appearance="standard" type="search" data-autofocus aria-label={th ? "ค้นหาหัวข้อ" : "Search topics"} placeholder={th ? "ค้นหาหัวข้อ…" : "Search topics…"} value={search} onChange={event => setSearch(event.target.value)}/>}
      <TopicDirectory label={label} items={items} selected={selected} search={search} onNavigate={() => setOpen(false)} language={language}/>
    </Dialog>
  </div>;
}

export function TopicDirectory({label,items,selected,search="",onNavigate,language="en"}:{label:string;items:readonly TopicLink[];selected?:string;search?:string;onNavigate?:()=>void;language?:"th"|"en"}) {
  const visible = items.filter(item => `${item.title} ${item.description ?? ""} ${item.group ?? ""}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()));
  const groups = [...new Set(visible.map(item => item.group ?? ""))];
  return <nav className={styles.topicDirectory} aria-label={label}>{groups.map(group => <div key={group}>{group && <h3>{group}</h3>}{visible.filter(item => (item.group ?? "") === group).map(item => <Link data-product-switch key={item.id} href={item.href} aria-current={item.id === selected ? "page" : undefined} onClick={onNavigate}><strong>{item.title}</strong>{item.description && <small>{item.description}</small>}</Link>)}</div>)}{!visible.length && <p role="status">{language === "th" ? "ไม่พบหัวข้อที่ตรงกัน" : "No matching topics."}</p>}</nav>;
}

export function WorkEntryNavigation({context}:{context:NavigationContext}) {
  const pathname = usePathname(), query = useSearchParams();
  if (context.beta || context.scopeKind === "platform") return null;
  const canonicalPath = new URL(canonicalUiHref(pathname + "?" + query), "https://topics.invalid").pathname;
  const path = workspacePathParts(canonicalPath)?.path;
  // Record-only continuations must never acquire collection discovery here.
  if (!path || !destinationForPath(path) && !Object.hasOwn(workEntryRoots,path) && !["work","calendar","inbox"].includes(path)) return null;
  const model = workplaceNavigation(context), root = navigationWorkEntry(model,context.workspaceBase,canonicalPath);
  const daily = path === "work" || path.startsWith("work/") ? model.primary.find(item => item.id === "work") : path === "calendar" || path.startsWith("calendar/") ? model.primary.find(item => item.id === "calendar") : path === "inbox" || path.startsWith("inbox/") ? model.primary.find(item => item.id === "inbox") : undefined;
  const children = daily?.submodules ?? root?.submodules ?? [];
  const selected = children.find(item => new URL(item.href,"https://topics.invalid").pathname === canonicalPath)?.id ?? (daily?.id === "calendar" ? children.find(item=>item.href.endsWith("/calendar/team"))?.id : undefined) ?? children[0]?.id;
  const label = root?.label ?? daily?.label;
  const items = children.map(item => {
    const destination = destinationForPath(workspacePathParts(new URL(item.href,"https://topics.invalid").pathname)?.path ?? "");
    const tool = destination?.source === "legal" && destination.selection.tool;
    const target = new URL(topicHref(item.href,query),"https://topics.invalid");
    if (daily) for (const key of daily.id === "work" ? ["q","type"] : daily.id === "calendar" ? ["date","view","timezone"] : []) for (const value of query.getAll(key)) target.searchParams.append(key,value);
    return {id:item.id, title:destination?.source === "finance/contracts" ? "Billing contracts" : destination?.source === "legal" && destination.selection.kind === "contract" ? "Legal contracts" : destination?.label ?? item.label, href:target.pathname+target.search+target.hash, group:tool ? "Tools" : destination?.source === "legal" ? destination.selection.kind ? "Registers" : "Work" : undefined};
  });
  if (!label || items.length < 2) return null;
  const definition = workEntries.find(entry => entry.id === root?.id);
  if (definition?.mode === "index" && Object.hasOwn(workEntryRoots,path)) return null;
  return <div className={styles.entryNavigation} data-work-entry-navigation={root?.id ?? daily?.id}>
    <TopicNavigation label={label} items={items} selected={selected} language={context.language}/>
    {definition?.mode === "index" && <details className={styles.toolsIndex}><summary>{context.language === "th" ? "สารบัญ" : "Contents"}</summary><SearchableTopicDirectory label={label} items={items} selected={selected} language={context.language}/></details>}
  </div>;
}

export function SearchableTopicDirectory({label,items,selected,language}:{label:string;items:readonly TopicLink[];selected?:string;language?:"th"|"en"}) {
  const [search,setSearch] = useState("");
  return <div className={styles.directoryContents}><TextInput appearance="standard" type="search" aria-label={language === "th" ? "ค้นหาหัวข้อ" : "Search topics"} placeholder={language === "th" ? "ค้นหาหัวข้อ…" : "Search topics…"} value={search} onChange={event => setSearch(event.target.value)}/><TopicDirectory label={label} items={items} search={search} selected={selected} language={language}/></div>;
}
