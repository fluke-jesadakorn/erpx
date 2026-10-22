"use client";

import { useEffect, useRef, useState, useSyncExternalStore, type ComponentPropsWithRef, type HTMLAttributes, type ReactNode } from "react";
import { ChevronDown, ChevronRight, Search, X } from "lucide-react";
import { Button, TextInput } from "@erpx/ui/components/foundation";
import { Dialog } from "@erpx/ui/components/dialog";
import { cn } from "@erpx/ui/lib/cn";
import styles from "@erpx/ui/components/workspace.module.css";

export function WorkspaceToolbar({ search, onSearch, searchLabel, actions, children, ...props }: HTMLAttributes<HTMLElement> & {
  search?: string; onSearch?: (value: string) => void; searchLabel?: string; actions?: ReactNode;
}) {
  return <header {...props} className={cn(styles.toolbar, props.className)}>
    {onSearch && <div className={styles.search}><Search size={18} aria-hidden="true" /><TextInput appearance="standard" type="search" aria-label={searchLabel} placeholder={searchLabel} value={search ?? ""} onChange={event => onSearch(event.target.value)} /></div>}
    {children}{actions && <div className={styles.actions}>{actions}</div>}
  </header>;
}

export function WorkspaceFrame(props: HTMLAttributes<HTMLDivElement>) {
  return <div {...props} className={cn(styles.frame, props.className)} />;
}

/** Owners may retain their existing controlled mobile picker and focus state. */
export function WorkspaceNavigationSurface(props: ComponentPropsWithRef<"aside">) {
  return <aside {...props} className={cn(styles.surface, props.className)}/>;
}
export function WorkspaceDetailPanel(props: ComponentPropsWithRef<"aside">) {
  return <aside {...props} className={cn(styles.surface, props.className)}/>;
}

export function WorkspaceNavigation({ title, label, children, actions, inert, ...props }: HTMLAttributes<HTMLElement> & { title: string; label?: string; actions?: ReactNode }) {
  const [open, setOpen] = useState(false);
  const picker = useRef<HTMLButtonElement>(null);
  return <>
    <Button appearance="standard" variant="plain" type="button" ref={picker} className={styles.picker} aria-expanded={open} inert={inert} onClick={() => setOpen(value => !value)}>Choose {title.toLowerCase()} · {label ?? title}<ChevronDown size={16} /></Button>
    <WorkspaceNavigationSurface {...props} onClick={event => {props.onClick?.(event); if((event.target as HTMLElement).closest("nav button, nav a, [aria-pressed]")) {setOpen(false);picker.current?.focus();}}} inert={inert} className={cn(styles.navigation, props.className)} data-mobile-open={open || undefined}>
      <div className={styles.navigationHeading}><strong>{title}</strong>{actions}</div>{children}
    </WorkspaceNavigationSurface>
  </>;
}

export type WorkspaceSection = { id: string; title: string; description?: string; href?: string };
export function WorkspaceSections({ items, selected, onSelect, label }: { items: WorkspaceSection[]; selected: string; onSelect?: (id: string) => void; label: string }) {
  return <nav className={styles.sections} aria-label={label}>{items.map(item => item.href
    ? <a key={item.id} href={item.href} aria-current={selected === item.id ? "page" : undefined}><strong>{item.title}</strong>{item.description && <small>{item.description}</small>}</a>
    : <Button appearance="standard" variant="plain" type="button" key={item.id} aria-current={selected === item.id ? "page" : undefined} onClick={() => onSelect?.(item.id)}><strong>{item.title}</strong>{item.description && <small>{item.description}</small>}</Button>)}</nav>;
}

export function WorkspaceContent(props: HTMLAttributes<HTMLDivElement>) {
  return <div {...props} className={cn(styles.content, props.className)} />;
}
export function WorkspaceCollection({ detailOpen, ...props }: HTMLAttributes<HTMLDivElement> & { detailOpen?: boolean }) {
  return <div {...props} className={cn(styles.collection, props.className)} data-detail-open={detailOpen || undefined}/>;
}
export type WorkspaceRecord = { id: string; title: string; description?: ReactNode; icon?: ReactNode };
export function WorkspaceRecordList({ items, selected, onSelect, label, empty = "No records." }: { items: WorkspaceRecord[]; selected?: string | null; onSelect: (id: string) => void; label: string; empty?: ReactNode }) {
  return <div className={styles.recordList} role="region" aria-label={label}>{items.map(item => <Button appearance="standard" type="button" variant="plain" key={item.id} className={styles.record} aria-pressed={selected === item.id} onClick={() => onSelect(item.id)}>{item.icon}<span><strong>{item.title}</strong>{item.description && <small>{item.description}</small>}</span><ChevronRight size={16} aria-hidden="true"/></Button>)}{!items.length && <p>{empty}</p>}</div>;
}

export function WorkspaceContext({ label = "Viewing", children, ...props }: HTMLAttributes<HTMLDivElement> & { label?: string }) {
  return <div {...props} className={cn(styles.context, props.className)}><small>{label}</small>{children}</div>;
}

const subscribe = (listener: () => void) => {
  const media = window.matchMedia("(max-width:1250px)");
  media.addEventListener("change", listener);
  return () => media.removeEventListener("change", listener);
};
export function useWorkspaceDetailOverlay() {
  return useSyncExternalStore(subscribe, () => window.matchMedia("(max-width:1250px)").matches, () => false);
}
export function WorkspaceDetail({ open, title, onClose, children, "data-testid": testId, suspended = false, ...props }: HTMLAttributes<HTMLElement> & { suspended?: boolean; open: boolean; title: string; onClose: () => void; "data-testid"?: string }) {
  const overlay = useWorkspaceDetailOverlay();
  const panel = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!open || overlay) return;
    const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.current?.focus({preventScroll:true});
    return () => { if(trigger?.isConnected) trigger.focus({preventScroll:true}); };
  }, [open, overlay]);
  if (!open) return null;
  if (overlay) return <Dialog workspaceRole="detail" suspended={suspended} appearance="standard" open title={title} onClose={onClose} closeLabel="Close details" presentation="form-drawer" className={styles.overlay} data-testid={testId}>{children}</Dialog>;
  return <WorkspaceDetailPanel {...props} ref={panel} aria-hidden={suspended || undefined} data-suspended={suspended || undefined} tabIndex={-1} onKeyDown={event => {props.onKeyDown?.(event); if(event.key === "Escape") {event.stopPropagation();onClose();}}} data-testid={testId} className={cn(styles.detail, props.className)} aria-label={title}><header><h2>{title}</h2><Button appearance="standard" variant="plain" type="button" aria-label="Close details" onClick={onClose}><X size={20} /></Button></header><div className={styles.detailBody}>{children}</div></WorkspaceDetailPanel>;
}
