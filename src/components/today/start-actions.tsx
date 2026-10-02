"use client";

import Link from "next/link";
import { CalendarDays, CheckSquare2, CirclePlus, MessageSquareText, ReceiptText, ShoppingBasket, Sparkles, Send, Scale } from "lucide-react";
import { useState } from "react";
import styles from "./start-actions.module.css";

const actions = [
  { id: "buy", label: "Buy something", icon: ShoppingBasket, path: "/requests/new?kind=purchase" },
  { id: "expense", label: "Claim an expense", icon: ReceiptText, path: "/expenses/new" },
  { id: "leave", label: "Request leave", icon: CalendarDays, path: "/leave" },
  { id: "task", label: "Create a task", icon: CheckSquare2, path: "/staff-tasks/new" },
  { id: "post", label: "Post an update", icon: MessageSquareText, path: "/home#compose" },
  { id: "handover", label: "Pass on work", icon: Send, path: "/home#handover" },
] as const;

export function StartActions({ base, compact = false, rail = false, onNavigate, canRequestLegal = false }: {
  base: string;
  compact?: boolean;
  rail?: boolean;
  onNavigate?: () => void;
  canRequestLegal?: boolean;
}) {
  const [prompt, setPrompt] = useState("");
  function ask() {
    window.dispatchEvent(new CustomEvent("erpx:open-assistant", { detail: { question: prompt.trim(), mode: "intake" } }));
    onNavigate?.();
  }
  return <section className={`${styles.start} ${compact ? styles.compact : ""} ${rail ? styles.rail : ""}`} id={compact ? undefined : "start"} aria-label="Start work">
    {!compact && <div className={styles.heading}>
      <span className={styles.eyebrow}><CirclePlus size={16} aria-hidden="true" /> START SOMETHING</span>
      <h2>What would you like to do?</h2>
      <p>Tell ERPX what you need, or choose a direct path.</p>
    </div>}
    <form className={styles.ask} onSubmit={(event) => { event.preventDefault(); ask(); }}>
      <Sparkles size={21} aria-hidden="true" />
      <input aria-label="Describe what you want to do" placeholder="Tell ERPX what you need to do…" value={prompt} onChange={(event) => setPrompt(event.target.value)} />
      <button type="submit">Ask ERPX</button>
    </form>
    <div className={styles.actions}>
      {canRequestLegal && <Link href={`${base}/legal?kind=request&new=request`} onClick={onNavigate}><Scale size={18} aria-hidden="true" />Request legal advice</Link>}
      {actions.map(({ id, label, icon: Icon, path }) => <Link key={id} href={`${base}${path}`} onClick={onNavigate}>
        <Icon size={18} aria-hidden="true" />{label}
      </Link>)}
    </div>
  </section>;
}
