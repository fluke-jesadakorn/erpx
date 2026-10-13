"use client";
import { lazy, Suspense, useEffect, useState, type ComponentProps, type CSSProperties } from "react";
import { Sparkles } from "lucide-react";
import { Button } from "@/components/ui/foundation";
import { useHydrated } from "@/components/ui/use-hydrated";
import styles from "./workspace-assistant.module.css";
const Content = lazy(() => import("./workspace-assistant-content").then(module => ({ default: module.WorkspaceAssistantContent })));
export function WorkspaceAssistant(props: Omit<ComponentProps<typeof Content>, "initialIntent">) {
  const ready = useHydrated();
  const [intent, setIntent] = useState<{ question?: string; mode?: string } | null>(null);
  useEffect(() => {
    const open = (event: Event) => {
      const detail = (event as CustomEvent<{ question?: unknown; mode?: unknown }>).detail;
      setIntent({ question: typeof detail?.question === "string" ? detail.question.slice(0, 4000) : "",
        mode: detail?.mode === "intake" ? "intake" : "chat" });
    };
    window.addEventListener("erpx:open-assistant", open);
    return () => window.removeEventListener("erpx:open-assistant", open);
  }, []);
  return intent ? <Suspense fallback={<p role="status">Opening work assistant…</p>}><Content {...props} initialIntent={intent} /></Suspense> :
    <div data-app-chrome><Button appearance="standard" type="button" variant="primary" disabled={!ready}
      style={{ "--ui-brand": "#3458db", "--ui-brand-strong": "#2947b8", "--ui-on-brand": "#fff" } as CSSProperties}
      className={styles.launcher} onClick={() => setIntent({})} aria-label="Open work assistant">
      <Sparkles size={19} /><span>Ask ERPX</span>
    </Button></div>;
}
