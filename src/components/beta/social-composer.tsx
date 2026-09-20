"use client";

import { workFetch } from "@/lib/work-fetch";
import { PersonAvatar } from "@/components/ui/person-avatar";
import { SocialTextInput } from "./social-text-input";
import type { Mention } from "@/lib/social-text";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { ImagePlus, PenLine, ChevronDown, X } from "lucide-react";
import { betaFetch } from "@/lib/beta-client";
import { useSocialCommand } from "./social-command";
import type { Meta, Draft } from "./social-types";
const subscribeHydration = () => () => {};
const allowedPhotoTypes = new Set(["image/jpeg", "image/png", "image/webp"]);
const maxPhotoBytes = 10 * 1024 * 1024;

function PhotoPreview({
  file,
  remove,
}: {
  file: File;
  remove: () => void;
}) {
  const image = useRef<HTMLImageElement>(null);
  useEffect(() => {
    const url = URL.createObjectURL(file);
    if (image.current) image.current.src = url;
    return () => URL.revokeObjectURL(url);
  }, [file]);
  return (
    <figure>
      {/* blob previews are local until the draft is saved */}
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img ref={image} alt={file.name} />
      <figcaption title={file.name}>{file.name}</figcaption>
      <button type="button" onClick={remove} aria-label={`Remove ${file.name}`}>
        <X size={14} aria-hidden="true" />
      </button>
    </figure>
  );
}

function PhotoPreviews({ photos, remove }: { photos: File[]; remove: (index: number) => void }) {
  return (
    <div className="beta-photo-previews" aria-label="Photos attached to this update">
      {photos.map((file, index) => (
        <PhotoPreview
          key={`${file.name}:${file.size}:${file.lastModified}:${index}`}
          file={file}
          remove={() => remove(index)}
        />
      ))}
    </div>
  );
}

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
  function addPhotos(files: File[]) {
    const supported = files.filter((file) => allowedPhotoTypes.has(file.type) && file.size > 0 && file.size <= maxPhotoBytes);
    const available = Math.max(0, 10 - photos.length);
    const added = supported.slice(0, available);
    if (added.length) setPhotos((current) => [...current, ...added].slice(0, 10));
    if (files.some((file) => !allowedPhotoTypes.has(file.type)))
      setMessage("Posts accept JPEG, PNG, and WebP images.");
    else if (files.some((file) => file.size < 1 || file.size > maxPhotoBytes))
      setMessage("Each image must be between 1 byte and 10 MB.");
    else if (files.length > available)
      setMessage("You can attach up to 10 images to an update.");
    else if (added.length)
      setMessage(`${added.length === 1 ? "Image" : `${added.length} images`} attached.`);
  }
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
        <SocialTextInput api={api} label="Post message" disabled={!ready || busy} value={body} mentions={mentions} onPasteImages={addPhotos} onChange={(body,mentions)=>{setBody(body);setMentions(mentions);}} placeholder="Share a win, ask for help, paste an image, or mention @ERPX assistant…" maxLength={3000}/>
        {photos.length > 0 && <PhotoPreviews photos={photos} remove={(index) => setPhotos((current) => current.filter((_, photoIndex) => photoIndex !== index))} />}
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
              onChange={(e) => {
                addPhotos(Array.from(e.target.files ?? []));
                e.target.value = "";
              }}
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
