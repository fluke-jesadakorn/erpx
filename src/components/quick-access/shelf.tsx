"use client";

import Link from "next/link";
import {
  ArrowDown, ArrowUp, Banknote, BedDouble, BriefcaseBusiness, Building2, CalendarDays, CalendarRange, Check, ChevronDown,
  ClipboardCheck, ClipboardList, Clock3, CreditCard, FileText,
  HandCoins, History, House, LayoutGrid, Moon, Package, Plus,
  Receipt, RotateCcw, Settings2, ShieldCheck, ShoppingCart,
  Store, Trash2, Users, Utensils, Wrench,
} from "lucide-react";
import { useId, useMemo, useRef, useState } from "react";
import { visibleQuickAccessItems, type QuickAccessItem, type QuickAccessPreference } from "@/lib/quick-access";
import { workFetch } from "@/lib/work-fetch";
import { Button } from "@/components/ui/foundation";
import { Dialog } from "@/components/ui/dialog";
import styles from "./shelf.module.css";

const icons = {
  tasks: ClipboardList, expenses: HandCoins, shifts: CalendarDays,
  leave: CalendarDays, overtime: Clock3, services: Wrench,
  equipment: Package, purchases: ShoppingCart, approvals: ClipboardCheck,
  team: Users, counts: ClipboardCheck, transfers: Package,
  orders: ShoppingCart, documents: FileText, security: ShieldCheck,
  account: Users, workspace: BriefcaseBusiness, pos: Store,
  pms: Store, payments: CreditCard, settings: Settings2,
  sale: ShoppingCart, kitchen: Utensils, "cash-shifts": Banknote,
  history: History, reservations: CalendarRange, "front-desk": Store,
  rooms: BedDouble, housekeeping: House, folios: Receipt,
  cashier: CreditCard, "night-audit": Moon, company: Building2,
} as const;

function iconFor(name?: string) {
  return icons[name as keyof typeof icons] ?? LayoutGrid;
}

type Props = {
  items: QuickAccessItem[];
  initialPreferences: QuickAccessPreference;
  defaultIds: string[];
  endpoint: string;
  title?: string;
  requestMode?: "work" | "plain";
  loading?: boolean;
  error?: string;
  onRetry?: () => void;
  onSaved?: (preference: QuickAccessPreference) => void;
  embedded?: boolean;
  showAllMode?: boolean;
  locale?: "en" | "th";
  placement?: "inline" | "rail";
};

const copy = {
  en: {
    all: "All actions", customize: "Customize", retry: "Retry", loading: "Loading shortcuts",
    empty: "No shortcuts selected. Use Customize to add actions.",
    showAll: (count: number) => `Show all ${count}`, showFewer: "Show fewer",
    editorTitle: (title: string) => title === "Your shortcuts" ? "Customize shortcuts" : `Customize ${title.toLowerCase()}`,
    editorIntro: "Choose up to six shortcuts and arrange them in the order you want.",
    selected: "Selected", available: "Available actions", none: "No shortcuts selected.",
    otherRole: "Saved in another work role", otherRoleNote: "Kept for another scope or when available again.", moved: "Your shortcuts changed on another device. Compare the current order and confirm before saving yours.",
    currentOrder: "Saved order on your other device", reviewed: "I reviewed the current order and want to save my choice.",
    moveUp: "Move up", moveDown: "Move down", remove: "Remove", restore: "Restore defaults",
    cancel: "Cancel", save: "Save shortcuts", allNav: "All available actions",
    showShortcuts: "Show shortcuts", hideShortcuts: "Hide shortcuts",
  },
  th: {
    all: "การดำเนินการทั้งหมด", customize: "ปรับแต่ง", retry: "ลองอีกครั้ง", loading: "กำลังโหลดทางลัด",
    empty: "ยังไม่ได้เลือกทางลัด กดปรับแต่งเพื่อเพิ่มรายการ",
    showAll: (count: number) => `แสดงทั้งหมด ${count}`, showFewer: "แสดงน้อยลง",
    editorTitle: (title: string) => `ปรับแต่ง${title}`,
    editorIntro: "เลือกทางลัดได้สูงสุดหกรายการ แล้วจัดเรียงตามต้องการ",
    selected: "ที่เลือก", available: "รายการที่ใช้ได้", none: "ยังไม่ได้เลือกทางลัด",
    otherRole: "บันทึกไว้ในบทบาทอื่น", otherRoleNote: "เก็บไว้สำหรับขอบเขตอื่นหรือเมื่อกลับมาใช้งานได้", moved: "ทางลัดถูกเปลี่ยนจากอุปกรณ์อื่น ตรวจสอบลำดับล่าสุดก่อนบันทึก",
    currentOrder: "ลำดับที่บันทึกจากอุปกรณ์อื่น", reviewed: "ฉันตรวจสอบลำดับล่าสุดแล้วและต้องการบันทึกรายการนี้",
    moveUp: "เลื่อนขึ้น", moveDown: "เลื่อนลง", remove: "นำออก", restore: "คืนค่าเริ่มต้น",
    cancel: "ยกเลิก", save: "บันทึกทางลัด", allNav: "การดำเนินการที่ใช้ได้ทั้งหมด",
    showShortcuts: "แสดงทางลัด", hideShortcuts: "ซ่อนทางลัด",
  },
};

function selectedIds(draft: string[] | null, defaults: string[], available: Set<string>, preserveUnavailable = false) {
  return (draft ?? defaults).filter((id) => preserveUnavailable || available.has(id)).slice(0, 6);
}

function readPreference(body: unknown): QuickAccessPreference | null {
  const data = (body as { data?: unknown } | null)?.data as QuickAccessPreference | undefined;
  return data && (data.pinnedIds === null || Array.isArray(data.pinnedIds)) && Number.isInteger(data.version)
    ? data : null;
}

export function QuickAccessShelf(props: Props) {
  return <QuickAccessShelfState key={props.endpoint} {...props} />;
}

function QuickAccessShelfState({
  items, initialPreferences, defaultIds, endpoint, title = "Quick access",
  requestMode = "plain", loading = false, error = "", onRetry, onSaved,
  embedded = false, showAllMode = true,
  locale = "en", placement = "inline",
}: Props) {
  const t = copy[locale];
  const railBodyId = useId();
  const [localPreference, setPreference] = useState(initialPreferences);
  const preference = initialPreferences.version > localPreference.version ? initialPreferences : localPreference;
  const [expanded, setExpanded] = useState(false);
  const [railExpanded, setRailExpanded] = useState(false);
  const [allOpen, setAllOpen] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [draft, setDraft] = useState<string[] | null>(initialPreferences.pinnedIds);
  const [draftVersion, setDraftVersion] = useState(initialPreferences.version);
  const [saving, setSaving] = useState(false);
  const [editorError, setEditorError] = useState("");
  const [conflict, setConflict] = useState<QuickAccessPreference | null>(null);
  const [reviewed, setReviewed] = useState(false);
  const saveAttempt = useRef<{ body: string; key: string } | null>(null);
  const availableIds = useMemo(() => new Set(items.map((item) => item.id)), [items]);
  const availableById = useMemo(() => new Map(items.map((item) => [item.id, item])), [items]);
  const chosen = selectedIds(draft, defaultIds, availableIds, requestMode === "work" && draft !== null);
  const visible = visibleQuickAccessItems(items, preference, defaultIds).slice(0, 6);
  const selected = new Set(chosen);

  function openEditor() {
    setDraft(preference.pinnedIds);
    setDraftVersion(preference.version);
    setConflict(null);
    setReviewed(false);
    setEditorError("");
    setEditorOpen(true);
  }
  function updateDraft(ids: string[]) {
    setDraft(ids);
    setEditorError("");
  }
  function shift(id: string, offset: number) {
    const next = [...chosen];
    const old = next.indexOf(id), target = old + offset;
    if (old < 0 || target < 0 || target >= next.length) return;
    [next[old], next[target]] = [next[target], next[old]];
    updateDraft(next);
  }
  function request(input: RequestInfo | URL, init?: RequestInit) {
    return requestMode === "work" ? workFetch(input, init) : fetch(input, init);
  }
  async function save() {
    if (saving || (conflict && !reviewed)) return;
    setSaving(true);
    setEditorError("");
    try {
      const bodyText = JSON.stringify({ pinnedIds: draft, expectedVersion: draftVersion });
      if (saveAttempt.current?.body !== bodyText)
        saveAttempt.current = { body: bodyText, key: crypto.randomUUID() };
      const response = await request(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": saveAttempt.current.key },
        body: bodyText,
      });
      const body = await response.json().catch(() => null) as { data?: unknown; error?: { code?: string; message?: string } } | null;
      if (!response.ok) {
        if (response.status === 409 && body?.error?.code === "VERSION_CONFLICT") {
          const latestResponse = await request(endpoint, { cache: "no-store" });
          const latest = readPreference(await latestResponse.json().catch(() => null));
          if (latestResponse.ok && latest) {
            setPreference(latest);
            setDraftVersion(latest.version);
            setConflict(latest);
            setReviewed(false);
            setEditorError(t.moved);
            return;
          }
        }
        throw new Error(body?.error?.message ?? "Could not save your shortcuts.");
      }
      const saved = readPreference(body);
      if (!saved) throw new Error("The server returned an incomplete shortcut preference.");
      setPreference(saved);
      setDraftVersion(saved.version);
      onSaved?.(saved);
      saveAttempt.current = null;
      setEditorOpen(false);
    } catch (cause) {
      setEditorError(cause instanceof Error ? cause.message : "Could not save your shortcuts.");
    } finally {
      setSaving(false);
    }
  }

  return <section
    className={`${styles.shelf} ${embedded ? styles.embedded : ""} ${placement === "rail" ? styles.rail : ""} ${placement === "rail" && !railExpanded ? styles.railCollapsed : ""}`}
    aria-label={title}
    data-quick-access-shelf
    data-print-hide
  >
    <div className={styles.header}>
      <strong className={styles.heading}>{title}</strong>
      {placement === "rail" && <Button variant="ghost" size="compact" className={styles.railDisclosure}
        onClick={() => setRailExpanded((was) => !was)} aria-expanded={railExpanded} aria-controls={railBodyId}>
        <ChevronDown size={16} aria-hidden="true" /> {railExpanded ? t.hideShortcuts : t.showShortcuts}
      </Button>}
      <div className={styles.tools}>
        {items.length > visible.length && <Button variant="ghost" size="compact" onClick={() => setAllOpen(true)} disabled={loading}>
          <LayoutGrid size={16} aria-hidden="true" /> {t.all}
        </Button>}
        <Button variant="ghost" size="compact" onClick={openEditor} disabled={loading || !!error}>
          <Settings2 size={16} aria-hidden="true" /> {t.customize}
        </Button>
      </div>
    </div>
    <div id={railBodyId} className={styles.body}>
    {error && <div className={styles.notice} role="status">{error}{onRetry && <> <Button variant="ghost" size="compact" onClick={onRetry}>{t.retry}</Button></>}</div>}
    {loading ? <div className={styles.grid} aria-label={t.loading} aria-busy="true">
      {Array.from({ length: 6 }, (_, index) => <div key={index} className={styles.skeleton} />)}
    </div> : visible.length ? <>
      <div className={`${styles.grid} ${showAllMode && !expanded ? styles.collapsed : ""}`}>
        {visible.map((item) => {
          const Icon = iconFor(item.icon ?? item.id);
          return <article className={styles.tile} aria-label={item.title} data-tone={item.tone} data-quick-access-id={item.id} key={item.id}>
            <Link href={item.href} className={styles.tileLink}>
              <span className={styles.icon}><Icon size={19} strokeWidth={2.1} aria-hidden="true" /></span>
              <span className={styles.tileTitle}>{item.title}</span>
              {item.description && <span className={styles.description}>{item.description}</span>}
              {item.meta && <span className={styles.meta}>{item.meta}</span>}
            </Link>
            {item.secondary && <Link className={styles.secondary} href={item.secondary.href} aria-label={`${item.secondary.label} in ${item.title}`}>
              <Plus size={16} aria-hidden="true" />{item.secondary.label}
            </Link>}
          </article>;
        })}
      </div>
      {showAllMode && visible.length > 4 && <Button variant="ghost" size="compact" className={styles.mobileMore} onClick={() => setExpanded((was) => !was)} aria-expanded={expanded}>
        {expanded ? t.showFewer : t.showAll(visible.length)}
      </Button>}
    </> : <p className={styles.empty}>{t.empty}</p>}
    </div>

    <Dialog open={editorOpen} onClose={() => setEditorOpen(false)} title={t.editorTitle(title)} presentation="responsive-panel">
      <div className={styles.editor}>
        <p>{t.editorIntro}</p>
        {conflict && <div className={styles.conflict} role="alert">
          <strong>{t.currentOrder}</strong>
          <p>{selectedIds(conflict.pinnedIds, defaultIds, availableIds, requestMode === "work").map((id) => availableById.get(id)?.title ?? t.otherRole).join(" · ") || t.none}</p>
          <label><input type="checkbox" checked={reviewed} onChange={(event) => setReviewed(event.target.checked)} /> {t.reviewed}</label>
        </div>}
        <fieldset>
          <legend>{t.selected} ({chosen.length}/6)</legend>
          {chosen.length ? <ol className={styles.pins}>{chosen.map((id, index) => {
            const item = availableById.get(id);
            const label = item?.title ?? t.otherRole;
            return <li key={id}>
              <span><strong>{label}</strong>{item?.description && <small>{item.description}</small>}{!item && <small>{t.otherRoleNote}</small>}</span>
              <span className={styles.pinTools}>
                <Button variant="ghost" size="compact" onClick={() => shift(id, -1)} disabled={index === 0} aria-label={`${t.moveUp}: ${label}`}><ArrowUp size={16} aria-hidden="true" /></Button>
                <Button variant="ghost" size="compact" onClick={() => shift(id, 1)} disabled={index === chosen.length - 1} aria-label={`${t.moveDown}: ${label}`}><ArrowDown size={16} aria-hidden="true" /></Button>
                <Button variant="ghost" size="compact" onClick={() => updateDraft(chosen.filter((value) => value !== id))} aria-label={`${t.remove}: ${label}`}><Trash2 size={16} aria-hidden="true" /></Button>
              </span>
            </li>;
          })}</ol> : <p>{t.none}</p>}
        </fieldset>
        <fieldset>
          <legend>{t.available}</legend>
          <div className={styles.choices}>
            {items.filter((item) => !selected.has(item.id)).map((item) => <Button key={item.id} variant="secondary" size="compact" disabled={chosen.length >= 6} onClick={() => updateDraft([...chosen, item.id])}>
              <Plus size={16} aria-hidden="true" /> {item.title}
            </Button>)}
          </div>
        </fieldset>
        {editorError && <p role="alert">{editorError}</p>}
        <div className={styles.editorActions}>
          <Button variant="ghost" size="compact" onClick={() => { setDraft(null); setEditorError(""); }}><RotateCcw size={16} aria-hidden="true" /> {t.restore}</Button>
          <Button variant="secondary" size="compact" onClick={() => setEditorOpen(false)}>{t.cancel}</Button>
          <Button onClick={() => void save()} pending={saving} disabled={!!conflict && !reviewed}><Check size={16} aria-hidden="true" /> {t.save}</Button>
        </div>
      </div>
    </Dialog>
    <Dialog open={allOpen} onClose={() => setAllOpen(false)} title={t.all} presentation="responsive-panel">
      <nav className={styles.allActions} aria-label={t.allNav}>
        {items.map((item) => {
          const Icon = iconFor(item.icon ?? item.id);
          return <div className={styles.allActionRow} key={item.id} data-tone={item.tone}>
            <Link href={item.href} onClick={() => setAllOpen(false)}>
              <span className={styles.icon}><Icon size={19} aria-hidden="true" /></span>
              <span><strong>{item.title}</strong>{item.description && <small>{item.description}</small>}</span>
            </Link>
            {item.secondary && <Link href={item.secondary.href} onClick={() => setAllOpen(false)} aria-label={`${item.secondary.label} in ${item.title}`}>
              <Plus size={16} aria-hidden="true" />{item.secondary.label}
            </Link>}
          </div>;
        })}
      </nav>
    </Dialog>
  </section>;
}
