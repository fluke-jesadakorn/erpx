"use client";

import { workFetch } from "@/lib/work-fetch";
import { PersonAvatar } from "@/components/ui/person-avatar";
import { SocialTextInput } from "./social-text-input";
import type { Mention } from "@/lib/social-text";
import { useState, useSyncExternalStore } from "react";
import { ImagePlus, PenLine, ChevronDown } from "lucide-react";
import { betaFetch } from "@/lib/beta-client";
import { useSocialCommand } from "./social-command";
import type { Meta, Draft } from "./social-types";
const subscribeHydration = () => () => {};
export function SocialComposer({
  api,
  name,
  branchId,
  meta,
  setMeta,
  loadMeta,
  onPublished,
}: {
  api: string;
  name: string;
  branchId: string | null;
  meta: Meta;
  setMeta: (meta: Meta) => void;
  loadMeta: () => void;
  onPublished: () => Promise<void>;
}) {
  const ready = useSyncExternalStore(
    subscribeHydration,
    () => true,
    () => false,
  );
  const { run, send, busy, message, setMessage } = useSocialCommand();
  const [mentions, setMentions] = useState<Mention[]>([]);
  const [body, setBody] = useState(""),
    [audience, setAudience] = useState(branchId ?? ""),
    [draft, setDraft] = useState<Draft | null>(null),
    [photos, setPhotos] = useState<File[]>([]);
  async function save(publish = false) {
    await run(async () => {
      const saved = await send<{ id: string; version: number }>(
        `${api}/beta/drafts${draft ? `/${draft.id}` : ""}`,
        {
          body,
          mentions,
          branchId: audience || null,
          expectedVersion: draft?.version ?? 1,
        },
      );
      let version = saved.version;
      setDraft({ id: saved.id, version, body, branch_id: audience || null, mentions_json: JSON.stringify(mentions) });
      for (const file of photos) {
        const intent = await send<{ uploadUrl: string }>(`${api}/uploads`, {
          ownerType: "social_draft",
          ownerId: saved.id,
          purpose: "post-image",
          filename: file.name,
          mediaType: file.type,
          bytes: file.size,
        });
        const r = await workFetch(intent.uploadUrl, { method: "PUT", body: file });
        const result = (await r.json()) as { error?: { message: string } };
        if (!r.ok)
          throw new Error(result.error?.message ?? "Image upload failed.");
      }
      setPhotos([]);
      const refreshed = await betaFetch<Meta>(`${api}/beta/meta`);
      setMeta(refreshed);
      version =
        refreshed.drafts.find((d) => d.id === saved.id)?.version ?? version;
      setDraft({ id: saved.id, version, body, branch_id: audience || null, mentions_json: JSON.stringify(mentions) });
      if (publish) {
        await send(`${api}/beta/drafts/${saved.id}/publish`, {
          expectedVersion: version,
        });
        setBody("");
        setMentions([]);
        setDraft(null);
        setMessage("Shared with your team.");
        await onPublished();
      } else setMessage("Draft saved. Only you can see it.");
    });
  }
  return (
    <>
      <details className="card beta-composer" onToggle={(event) => {
        if (event.currentTarget.open) loadMeta();
      }}>
        <summary>
          <PersonAvatar name={name} />
          <span className="beta-composer-prompt"><strong>Share an update</strong><small>Ask a question or share progress with your team</small></span>
          <PenLine className="beta-compose-icon" size={18} aria-hidden="true" />
          <ChevronDown className="beta-compose-chevron" size={16} aria-hidden="true" />
        </summary>
        <div className="beta-person">
          <PersonAvatar name={name} />
          <strong>What’s happening with your team?</strong>
        </div>
        <SocialTextInput api={api} label="Post message" disabled={!ready || busy} value={body} mentions={mentions} onChange={(body,mentions)=>{setBody(body);setMentions(mentions);}} placeholder="Share a win, ask for help, or mention @ERPX assistant…" maxLength={3000}/>
        <div className="beta-composer-actions">
          <label>
            Share with
            <select
              aria-label="Post audience"
              value={audience}
              onChange={(e) => setAudience(e.target.value)}
            >
              {!branchId && <option value="">Everyone in the company</option>}
              {meta.branches.map((b) => (
                <option key={b.id} value={b.id}>
                  {b.display_name}
                </option>
              ))}
            </select>
          </label>
          <label className="button secondary">
            <ImagePlus size={15} /> Photos
            <input
              aria-label="Add photos"
              type="file"
              accept="image/png,image/jpeg,image/webp"
              multiple
              onChange={(e) =>
                setPhotos(Array.from(e.target.files ?? []).slice(0, 10))
              }
            />
          </label>
          <button
            className="button secondary"
            disabled={busy}
            onClick={() => save()}
          >
            Save draft
          </button>
          <button
            className="button"
            disabled={busy || !body.trim()}
            onClick={() => save(true)}
          >
            Share update
          </button>
        </div>
        {photos.length > 0 && <p>{photos.map((f) => f.name).join(", ")}</p>}
        {meta.drafts.length > 0 && (
          <details>
            <summary>Your private drafts ({meta.drafts.length})</summary>
            {meta.drafts.map((d) => (
              <button
                className="beta-draft"
                key={d.id}
                onClick={() => {
                  setDraft(d);
                  setBody(d.body);
                  setMentions(JSON.parse(d.mentions_json ?? "[]"));
                  setAudience(d.branch_id ?? "");
                }}
              >
                {d.body || "Untitled draft"}
              </button>
            ))}
          </details>
        )}
      </details>
      <p className="beta-status" role="status">
        {busy ? "Saving…" : message}
      </p>
    </>
  );
}
