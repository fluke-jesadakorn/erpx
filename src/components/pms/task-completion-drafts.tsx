"use client";

import { Button, TextArea, TextInput } from "@/components/ui/foundation";
import { useState } from "react";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import { attachmentContentUrl } from "@/lib/files/attachment-client";
import type { PmsTaskCompletionDraft } from "./workspace-types";
import styles from "./workspace.module.css";

export function TaskCompletionDrafts({
  drafts,
  onUpdate,
  onDiscard,
}: {
  drafts: PmsTaskCompletionDraft[];
  onUpdate: (draft: PmsTaskCompletionDraft) => void;
  onDiscard: (key: string) => void;
}) {
  const [copiedKey,setCopiedKey]=useState<string | null>(null);
  const language = useFrontLanguage();
  const tr = (value: string) => frontText(value, language);
  if (!drafts.length) return null;

  return (
    <FrontLocalizedTree>
      {/* This editor has its own PMS owner guard; avoid the boundary's sticky form-less dirty marker. */}
      <section aria-label={tr("Saved completion drafts")} className={styles.taskCreation} data-work-dirty="false">
        <h3>{tr("Saved completion drafts")}</h3>
        {drafts.map((draft) => (
          <article key={draft.key} className={styles.record} data-pms-task-completion-draft={draft.taskId}>
            <div className={styles.line}>
              <h4>{tr(draft.taskLabel)} · {draft.taskId}</h4>
              <span role="status">{tr("Local reference · Later changes were not submitted and cannot update this completed task")}</span>
            </div>
            <div>
              {draft.checklist.map((item, index) => (
                <label key={`${item.label}:${index}`}>
                  <TextInput
                    appearance="standard"
                    type="checkbox"
                    checked={item.checked}
                    disabled={!draft.editing}
                    onChange={(event) => onUpdate({
                      ...draft,
                      checklist: draft.checklist.map((current, currentIndex) =>
                        currentIndex === index ? { ...current, checked: event.target.checked } : current,
                      ),
                    })}
                  />
                  {tr(item.label)}
                </label>
              ))}
            </div>
            <label>
              {tr("Notes")}
              <TextArea
                appearance="standard"
                rows={3}
                value={draft.notes}
                disabled={!draft.editing}
                onChange={(event) => onUpdate({ ...draft, notes: event.target.value })}
              />
            </label>
            {draft.attachments.length > 0 && (
              <div>
                <h5>{tr("Attachment references at acknowledgement")}</h5>
                <p>{tr("Files are shown for reference only. This draft does not upload or submit them again.")}</p>
                {draft.attachments.map((attachment) => (
                  <section key={attachment.purposeKey} aria-label={tr(attachment.purposeKey === "damage_photo" ? "Damage evidence" : "Room completion photos")}>
                    <strong>{tr(attachment.purposeKey === "damage_photo" ? "Damage evidence" : "Room completion photos")}</strong>
                    {attachment.setId && <p>{tr("Attachment set")}: <code>{attachment.setId}</code></p>}
                    {attachment.links.length ? (
                      <ul>{attachment.links.map((link) => {
                        const active = !["withdrawn", "expired"].includes(link.linkState);
                        return (
                        <li key={link.id}>
                          {link.filename} · {link.linkState} · {link.sourceState ?? tr("unknown")} · <code>{link.id}</code>
                          {active && link.previewAvailable && <a href={attachmentContentUrl(draft.attachmentBase, link.id, "preview")} target="_blank" rel="noreferrer">{tr("Open sanitized preview")}</a>}
                          {active && link.sourceState === "available" && <a href={attachmentContentUrl(draft.attachmentBase, link.id, "original")}>{tr("Download private original")}</a>}
                        </li>
                        );
                      })}</ul>
                    ) : <p>{tr("No files in this attachment set")}</p>}
                  </section>
                ))}
              </div>
            )}
            <div className={styles.formFoot}>
              {draft.editing ? (
                <Button appearance="standard" type="button" variant="secondary" onClick={() => onUpdate({ ...draft, editing: false })}>
                  {tr("Done editing")}
                </Button>
              ) : (
                <Button appearance="standard" type="button" variant="secondary" onClick={() => onUpdate({ ...draft, editing: true })}>
                  {tr("Keep editing")}
                </Button>
              )}
              <Button appearance="standard" type="button" variant="secondary" onClick={async()=>{
                const text=[draft.taskLabel,`${tr("Task")}: ${draft.taskId}`,...draft.checklist.map(item=>`${item.checked?"[x]":"[ ]"} ${tr(item.label)}`),`${tr("Notes")}: ${draft.notes}`,...draft.attachments.flatMap(attachment=>[
                  `${tr(attachment.purposeKey==="damage_photo"?"Damage evidence":"Room completion photos")} · ${tr("reference only")}`,
                  `${tr("Attachment set")}: ${attachment.setId ?? tr("not created")}`,
                  ...attachment.links.map(link=>{
                    const state=`${link.linkState}, ${link.sourceState??tr("unknown")}`;
                    const urls=[...(link.previewAvailable?[attachmentContentUrl(draft.attachmentBase,link.id,"preview")]:[]),...(link.sourceState==="available"?[attachmentContentUrl(draft.attachmentBase,link.id,"original")]:[])].map(url=>new URL(url,window.location.origin).href);
                    return `- ${link.filename} (${state}; id ${link.id})${urls.length?`\n  ${urls.join("\n  ")}`:""}`;
                  }),
                ])].join("\n");
                try{await navigator.clipboard.writeText(text);setCopiedKey(draft.key);}catch{setCopiedKey(null);}
              }}>
                {tr(copiedKey===draft.key?"Copied":"Copy local reference")}
              </Button>
              <Button appearance="standard" type="button" variant="secondary" onClick={() => onDiscard(draft.key)}>
                {tr("Discard draft")}
              </Button>
            </div>
            {copiedKey===draft.key&&<p role="status">{tr("Reference copied for your notes.")}</p>}
          </article>
        ))}
      </section>
    </FrontLocalizedTree>
  );
}
