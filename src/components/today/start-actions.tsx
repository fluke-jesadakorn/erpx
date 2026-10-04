"use client";
import { Button, TextInput } from "@/components/ui/foundation";

import Link from "next/link";
import {
  CalendarDays,
  CheckSquare2,
  CirclePlus,
  MessageSquareText,
  ReceiptText,
  ShoppingBasket,
  Sparkles,
  Send,
  Scale,
  Clock3,
  LifeBuoy,
} from "lucide-react";
import { useState } from "react";
import styles from "./start-actions.module.css";
import { workplaceStartActions } from "@/lib/workplace-catalogue";

const actionIcons = {
  buy: ShoppingBasket,
  expense: ReceiptText,
  leave: CalendarDays,
  overtime: Clock3,
  task: CheckSquare2,
  post: MessageSquareText,
  handover: Send,
  help: LifeBuoy,
  plan: CalendarDays,
};
const actions = workplaceStartActions.map((action) => ({
  ...action,
  icon: actionIcons[action.id],
}));

export function StartActions({
  base,
  compact = false,
  rail = false,
  onNavigate,
  canRequestLegal = false,
}: {
  base: string;
  compact?: boolean;
  rail?: boolean;
  onNavigate?: () => void;
  canRequestLegal?: boolean;
}) {
  const [prompt, setPrompt] = useState("");
  function ask() {
    window.dispatchEvent(
      new CustomEvent("erpx:open-assistant", {
        detail: { question: prompt.trim(), mode: "intake" },
      }),
    );
    onNavigate?.();
  }
  return (
    <section
      className={`${styles.start} ${compact ? styles.compact : ""} ${rail ? styles.rail : ""}`}
      id={compact ? undefined : "start"}
      aria-label="Start work"
    >
      {!compact && (
        <div className={styles.heading}>
          <span className={styles.eyebrow}>
            <CirclePlus size={16} aria-hidden="true" /> START SOMETHING
          </span>
          <h2>What would you like to do?</h2>
          <p>Tell ERPX what you need, or choose a direct path.</p>
        </div>
      )}
      <form
        className={styles.ask}
        onSubmit={(event) => {
          event.preventDefault();
          ask();
        }}
      >
        <Sparkles size={21} aria-hidden="true" />
        <TextInput appearance="standard"
          aria-label="Describe what you want to do"
          placeholder="Tell ERPX what you need to do…"
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
        />
        <Button appearance="standard" clientManaged variant="ghost" type="submit">
          Ask ERPX
        </Button>
      </form>
      <div className={styles.actions}>
        {canRequestLegal && (
          <Link href={`${base}/legal?kind=request&new=request`} onClick={onNavigate}>
            <Scale size={18} aria-hidden="true" />
            Request legal advice
          </Link>
        )}
        {actions.map(({ id, label, icon: Icon, path }) => (
          <Link key={id} href={`${base}${path}`} onClick={onNavigate}>
            <Icon size={18} aria-hidden="true" />
            {label}
          </Link>
        ))}
      </div>
    </section>
  );
}
