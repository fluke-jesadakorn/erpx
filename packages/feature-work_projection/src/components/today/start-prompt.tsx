"use client";
import { useState } from "react";
import { Sparkles } from "lucide-react";
import { Button, TextInput } from "@erpx/ui/components/foundation";
import styles from "@erpx/feature-work_projection/components/today/start-actions.module.css";
export function StartPrompt({ rail, onNavigate }: { rail: boolean; onNavigate?: () => void }) {
  const [prompt, setPrompt] = useState("");
  return <form className={styles.ask} onSubmit={event => {
    event.preventDefault();
    window.dispatchEvent(new CustomEvent("erpx:open-assistant", { detail: { question: prompt.trim(), mode: "intake" } }));
    onNavigate?.();
  }}>
    <Sparkles size={21} aria-hidden="true" />
    <TextInput appearance="standard" aria-label="Describe what you want to do"
      placeholder="Tell ERPX what you need to do…" value={prompt} onChange={event => setPrompt(event.target.value)} />
    <Button appearance="standard" clientManaged variant={rail ? "primary" : "ghost"} type="submit">Ask ERPX</Button>
  </form>;
}
