"use client";
import { useEffect, useState, type ReactNode } from "react";
import { Dialog } from "@/components/ui/dialog";
import { Button, InlineNotice, type UiAppearance } from "@/components/ui/foundation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { contextGeneration } from "@/lib/work_projection/context-switch";
import { createMentionRecordRead, type MentionRecordDisplay } from "@/lib/records/mention-read";

export function SocialRecordMention({ base, targetId, label, appearance, className, children }: {
  base: string; targetId: string; label: string; appearance: UiAppearance; className?: string; children: ReactNode;
}) {
  const [reader] = useState(() => createMentionRecordRead(workFetch, contextGeneration));
  const [open, setOpen] = useState(false);
  const [record, setRecord] = useState<MentionRecordDisplay | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    const close = () => { reader.cancel(); setOpen(false); setRecord(null); setError(""); };
    window.addEventListener("erpx:work-context-changed", close);
    window.addEventListener("erpx:context-switch-started", close);
    return () => { reader.cancel(); window.removeEventListener("erpx:work-context-changed", close); window.removeEventListener("erpx:context-switch-started", close); };
  }, [reader]);
  const close = () => { reader.cancel(); setOpen(false); setRecord(null); setError(""); };
  async function load() {
    setOpen(true); setRecord(null); setError("");
    try { const value = await reader.read(base, targetId); if (value) setRecord(value); }
    catch (failure) { setError(failure instanceof Error ? failure.message : "This record is unavailable."); }
  }
  return <><Button appearance={appearance} variant="plain" type="button" className={className} onClick={load}>{children}</Button>
  <Dialog appearance={appearance} presentation="form-drawer" workspaceRole="detail" open={open} onClose={close} title={record?.title ?? label}>
    {error ? <InlineNotice tone="danger">{error}</InlineNotice> : !record ? <p role="status">Loading record…</p> : <>
      <p>{record.description}</p>
      <p>{record.status} · Version {record.version}</p>
      <dl>{record.fields.map((field, index) => <div key={`${field.key}:${index}`}><dt>{field.label}</dt><dd>{String(field.value)}</dd></div>)}</dl>
    </>}
  </Dialog></>;
}
