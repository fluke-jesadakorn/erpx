"use client";

import { useEffect, useRef, useState, type DragEvent } from "react";
import Link from "next/link";
import { BriefcaseBusiness, ChevronRight, Inbox, Search, Trash2, X } from "lucide-react";
import type { HoldingPeopleDirectory } from "@/server/holding-people-directory";
import { moveTrayKey, readMoveTray, useMoveTray, writeMoveTray, type MoveTrayItem } from "./people-move-tray-state";
import styles from "./people-move-tray.module.css";

const dragMime = "application/erpx-people-structure";

type Props = { base: string; actorId: string; companyId: string; directory: HoldingPeopleDirectory; draggingPersonId: string | null; onAddReady: (add: (engagementId: string) => void) => void };

export function PeopleMoveTray({ base, actorId, companyId, directory, draggingPersonId, onAddReady }: Props) {
  const key = moveTrayKey(base, companyId, actorId);
  const items = useMoveTray(key);
  const [open, setOpen] = useState(false);
  const [over, setOver] = useState(false);
  const [notice, setNotice] = useState("");
  const [search, setSearch] = useState<Record<string, string>>({});
  const [checked, setChecked] = useState<string[]>([]);
  const [sharedSearch, setSharedSearch] = useState("");
  const [sharedPosition, setSharedPosition] = useState("");
  const toggleRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  function setItems(change: (previous: MoveTrayItem[]) => MoveTrayItem[]) { writeMoveTray(key, change(readMoveTray(key))); }
  useEffect(() => {
    if (!open) return;
    closeRef.current?.focus();
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") { setOpen(false); toggleRef.current?.focus(); } };
    document.addEventListener("keydown", escape);
    return () => document.removeEventListener("keydown", escape);
  }, [open]);

  function add(engagementId: string) {
    const person = directory.people.find(value => value.id === engagementId && value.companyId === companyId);
    if (!person) { setNotice("This employee is not current in your Company."); return; }
    setItems(previous => {
      if (previous.some(value => value.engagementId === engagementId)) { setNotice(`${person.legalName} is already in the tray.`); return previous; }
      if (previous.length >= 20) { setNotice("A batch can hold up to 20 employees."); return previous; }
      setNotice(`${person.legalName} added to the tray.`);
      return [...previous, { engagementId, positionId: null }];
    });
  }
  useEffect(() => { onAddReady(add); });

  function receive(event: DragEvent<HTMLElement>) {
    event.preventDefault(); setOver(false);
    try {
      const source = JSON.parse(event.dataTransfer.getData(dragMime)) as { kind?: string; id?: string };
      if (source.kind === "person" && source.id) { add(source.id); setOpen(true); }
    } catch { setNotice("Drag a current employee into the tray."); }
  }

  const companyPositions = directory.positions.filter(position => position.companyId === companyId);
  const personById = (id: string) => directory.people.find(person => person.id === id && person.companyId === companyId);
  const positionById = (id: string | null) => companyPositions.find(position => position.id === id);
  const positionLabel = (position: HoldingPeopleDirectory["positions"][number]) => [position.title, directory.nodes.find(node => node.id === position.departmentId)?.name, directory.nodes.find(node => node.id === position.branchId)?.name].filter(Boolean).join(" · ");
  const options = (query: string) => companyPositions.filter(position => `${position.title} ${directory.nodes.find(node => node.id === position.departmentId)?.name ?? ""} ${directory.nodes.find(node => node.id === position.branchId)?.name ?? ""}`.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
  const invalid = items.filter(item => !personById(item.engagementId) || item.positionId && (!positionById(item.positionId) || personById(item.engagementId)?.positionId === item.positionId));
  const ready = items.length > 0 && !invalid.length && items.every(item => !!item.positionId);

  return <>
    <button ref={toggleRef} type="button" className={styles.launcher} data-dragging={!!draggingPersonId} data-over={over} data-testid="people-move-tray" onClick={() => setOpen(true)} onDragOver={event => { if (draggingPersonId && event.dataTransfer.types.includes(dragMime)) { event.preventDefault(); event.dataTransfer.dropEffect = "move"; setOver(true); } }} onDragLeave={() => setOver(false)} onDrop={receive}>
      <Inbox size={20} aria-hidden="true" /><span>{draggingPersonId ? "Drop employee here" : "Move tray"}</span><strong>{items.length}</strong>
    </button>
    {open && <div className={styles.backdrop} onMouseDown={event => { if (event.target === event.currentTarget) { setOpen(false); toggleRef.current?.focus(); } }}>
      <section className={styles.drawer} role="dialog" aria-modal="true" aria-label="Employee move tray" data-testid="people-move-tray-drawer" onKeyDown={event => {
        if (event.key !== "Tab") return;
        const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), a[href]:not([aria-disabled="true"]), input:not(:disabled), select:not(:disabled)'));
        const first = focusable[0], last = focusable.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }}>
        <header className={styles.header}><div><span>PEOPLE & ORGANIZATION</span><h2>Move tray · {items.length} employees</h2><p>Choose a destination for each employee, then review them together.</p></div><button ref={closeRef} type="button" aria-label="Close move tray" onClick={() => { setOpen(false); toggleRef.current?.focus(); }}><X size={20} /></button></header>
        <div className={styles.body}>
          {items.length > 1 && <div className={styles.group}><strong>Set one position for selected employees</strong><div className={styles.groupFields}><label><Search size={16} aria-hidden="true" /><input aria-label="Search shared destination" type="search" value={sharedSearch} onChange={event => setSharedSearch(event.target.value)} placeholder="Search all positions" /></label><select aria-label="Shared destination" value={sharedPosition} onChange={event => setSharedPosition(event.target.value)}><option value="">Choose position</option>{options(sharedSearch).map(position => <option key={position.id} value={position.id}>{positionLabel(position)}</option>)}</select><button type="button" disabled={!sharedPosition || checked.length === 0} onClick={() => { setItems(previous => previous.map(item => checked.includes(item.engagementId) ? { ...item, positionId: sharedPosition } : item)); setChecked([]); }}>Apply to {checked.length}</button></div></div>}
          {items.length === 0 && <p className={styles.empty}>Drag an employee here, or use Add to tray on a person in the structure or list.</p>}
          {items.map(item => {
            const person = personById(item.engagementId);
            const current = positionById(person?.positionId ?? null);
            const selected = positionById(item.positionId);
            const issue = !person ? "Employee unavailable in this Company." : item.positionId && !selected ? "Destination no longer available." : item.positionId === person.positionId ? "Choose a different position." : null;
            return <article className={styles.item} key={item.engagementId} data-testid={`people-move-tray-item-${item.engagementId}`}>
              <div className={styles.itemHead}><label><input type="checkbox" aria-label={`Select ${person?.legalName ?? item.engagementId} for shared position`} checked={checked.includes(item.engagementId)} onChange={event => setChecked(previous => event.target.checked ? [...previous, item.engagementId] : previous.filter(value => value !== item.engagementId))} /><span><strong>{person?.legalName ?? "Employee unavailable"}</strong><small>{current?.title ?? "No current position"}</small></span></label><button type="button" aria-label={`Remove ${person?.legalName ?? item.engagementId}`} onClick={() => { setItems(previous => previous.filter(value => value.engagementId !== item.engagementId)); setChecked(previous => previous.filter(value => value !== item.engagementId)); }}><Trash2 size={17} /></button></div>
              <div className={styles.destination}><BriefcaseBusiness size={18} aria-hidden="true" /><div><input type="search" aria-label={`Search destination for ${person?.legalName ?? item.engagementId}`} value={search[item.engagementId] ?? ""} onChange={event => setSearch(previous => ({ ...previous, [item.engagementId]: event.target.value }))} placeholder="Search positions across the Company" /><select aria-label={`Destination for ${person?.legalName ?? item.engagementId}`} value={item.positionId ?? ""} onChange={event => setItems(previous => previous.map(value => value.engagementId === item.engagementId ? { ...value, positionId: event.target.value || null } : value))}><option value="">Choose destination</option>{selected && !options(search[item.engagementId] ?? "").some(position => position.id === selected.id) && <option value={selected.id}>{positionLabel(selected)}</option>}{options(search[item.engagementId] ?? "").map(position => <option key={position.id} value={position.id}>{positionLabel(position)}</option>)}</select></div></div>
              {issue && <p className={styles.error} role="alert">{issue}</p>}
            </article>;
          })}
          {notice && <p className={styles.notice} role="status">{notice}</p>}
        </div>
        <footer className={styles.footer}><button type="button" disabled={!items.length} onClick={() => { setItems(() => []); setChecked([]); setNotice("Tray cleared."); }}>Clear tray</button><Link className={ready ? styles.continue : styles.disabled} href={ready ? `${base}/people/move/batch` : "#"} aria-disabled={!ready} onClick={event => { if (!ready) event.preventDefault(); }}>{ready ? "Review all moves" : "Choose every destination"}<ChevronRight size={17} /></Link></footer>
      </section>
    </div>}
  </>;
}
