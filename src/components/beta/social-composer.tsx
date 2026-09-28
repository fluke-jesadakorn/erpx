"use client";

import { workFetch } from "@/lib/work-fetch";
import { PersonAvatar } from "@/components/ui/person-avatar";
import { SocialTextInput } from "./social-text-input";
import type { Mention } from "@/lib/social-text";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { ImagePlus, PenLine, ChevronDown, X } from "lucide-react";
import { betaFetch } from "@/lib/beta-client";
import { useSocialCommand } from "./social-command";
import type { Meta, Draft, PostPurpose } from "./social-types";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@/lib/ai/guided-intake";
import { preparedPostFields } from "@/lib/ai/prepared-work";
type Source = { kind: "staff_task" | "staff_project"; id: string };

const subscribeHydration = () => () => {};
const allowedPhotoTypes = new Set(["image/jpeg", "image/png", "image/webp"]);
const maxPhotoBytes = 10 * 1024 * 1024;
const maxPostLength = 3000;

type TextBlock = { type: "text"; id: string; text: string; mentions: Mention[] };
type ImageBlock = {
  type: "image"; slotId: string; attachmentId: string | null; name: string; alt: string;
  file?: File; uploadUrl?: string;
};
type EditorBlock = TextBlock | ImageBlock;
type WireBlock = { type: "text"; text: string } |
  { type: "image"; slotId: string; attachmentId: string | null; name: string; alt: string };
type Selection = { id: string; start: number; end: number };

const textBlock = (text = "", mentions: Mention[] = []): TextBlock =>
  ({ type: "text", id: crypto.randomUUID(), text, mentions });
const wireContent = (blocks: EditorBlock[]): WireBlock[] => blocks.map(block =>
  block.type === "text" ? { type: "text", text: block.text } :
    { type: "image", slotId: block.slotId, attachmentId: block.attachmentId, name: block.name, alt: block.alt });
const postBody = (blocks: EditorBlock[]) => {
  const texts = blocks.filter((block): block is TextBlock => block.type === "text");
  return texts.some(block => block.text.trim()) ? texts.map(block => block.text).join("\n") : "";
};
function postMentions(blocks: EditorBlock[]) {
  const mentions: Mention[] = [];
  let offset = 0;
  for (const block of blocks) {
    if (block.type !== "text") continue;
    for (const mention of block.mentions)
      mentions.push({ ...mention, start: mention.start + offset, end: mention.end + offset });
    offset += block.text.length + 1;
  }
  return mentions;
}
function restoredBlocks(draft: Draft): EditorBlock[] {
  const globalMentions = JSON.parse(draft.mentions_json ?? "[]") as Mention[];
  let content: WireBlock[] | null = null;
  try {
    const parsed: unknown = draft.content_json ? JSON.parse(draft.content_json) : null;
    if (Array.isArray(parsed)) content = parsed as WireBlock[];
  } catch { /* Legacy drafts use body and mentions. */ }
  if (!content?.length) return [textBlock(draft.body, globalMentions)];
  let offset = 0;
  const result: EditorBlock[] = [];
  for (const block of content) {
    if (block.type === "text") {
      const local = globalMentions.filter(m => m.start >= offset && m.end <= offset + block.text.length)
        .map(m => ({ ...m, start: m.start - offset, end: m.end - offset }));
      result.push(textBlock(block.text, local));
      offset += block.text.length + 1;
    } else if (block.type === "image") {
      result.push({ type: "image", slotId: block.slotId, attachmentId: block.attachmentId,
        name: block.name, alt: block.alt ?? block.name });
    }
  }
  if (!result.some(block => block.type === "text")) result.unshift(textBlock());
  if (result.at(-1)?.type === "image") result.push(textBlock());
  return result;
}
function removeImage(blocks: EditorBlock[], slotId: string): EditorBlock[] {
  const kept = blocks.filter(block => block.type !== "image" || block.slotId !== slotId);
  const result: EditorBlock[] = [];
  for (const block of kept) {
    const previous = result.at(-1);
    if (block.type === "text" && previous?.type === "text") {
      const offset = previous.text.length + 1;
      result[result.length - 1] = { ...previous, text: `${previous.text}\n${block.text}`,
        mentions: [...previous.mentions, ...block.mentions.map(m =>
          ({ ...m, start: m.start + offset, end: m.end + offset }))] };
    } else result.push(block);
  }
  return result.length ? result : [textBlock()];
}

function ImagePreview({ block, api, remove, describe, reselect }: {
  block: ImageBlock; api: string; remove: () => void; describe: (alt: string) => void;
  reselect: (file: File) => void;
}) {
  const image = useRef<HTMLImageElement>(null);
  useEffect(() => {
    if (!block.file || !image.current) return;
    const url = URL.createObjectURL(block.file);
    image.current.src = url;
    return () => URL.revokeObjectURL(url);
  }, [block.file]);
  return <figure className="beta-inline-image">
    {block.file || block.attachmentId ? (
      // Blob previews and authenticated private attachments must bypass image optimization.
      // eslint-disable-next-line @next/next/no-img-element
      <img ref={image} src={block.file ? undefined : `${api}/attachments/${block.attachmentId}/content`}
        alt={block.alt || block.name} />
    ) : <div className="beta-image-missing" role="status">Select {block.name} again to finish this draft.</div>}
    <div className="beta-inline-image-controls">
      <input aria-label={`Image description for ${block.name}`} value={block.alt} maxLength={160}
        placeholder="Image description" onChange={event => describe(event.target.value)} />
      {!block.file && !block.attachmentId && <label className="button secondary">Select image
        <input type="file" accept="image/png,image/jpeg,image/webp" aria-label={`Reselect ${block.name}`}
          onChange={event => { const file = event.target.files?.[0]; if (file) reselect(file); event.target.value = ""; }} />
      </label>}
      <button type="button" onClick={remove} aria-label={`Remove ${block.name}`}>
        <X size={15} aria-hidden="true" />
      </button>
    </div>
  </figure>;
}

export function SocialComposer({ api, name, meta, setMeta, loadMeta, onPublished }: {
  api: string; name: string; branchId: string | null; meta: Meta;
  setMeta: (meta: Meta) => void; loadMeta: () => void; onPublished: () => Promise<void>;
}) {
  const ready = useSyncExternalStore(subscribeHydration, () => true, () => false);
  const { run, send, busy, message, setMessage } = useSocialCommand();
  const [blocks, setBlocks] = useState<EditorBlock[]>([{ type: "text", id: "initial", text: "", mentions: [] }]);
  const [audience, setAudience] = useState("");
  const [participants, setParticipants] = useState<string[]>([]);
  const [sources, setSources] = useState<Source[]>([]);
  const [sourceChoice, setSourceChoice] = useState("");
  const [purpose, setPurpose] = useState<PostPurpose>("update");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [guided, setGuided] = useState<Record<string,string> | null>(null);
  const composer = useRef<HTMLDetailsElement>(null);
  const [focus, setFocus] = useState<{ id: string; caret: number; signal: number } | null>(null);
  const selection = useRef<Selection | null>(null);
  const handoff = meta.handoffContext;
  useEffect(() => {
    const openTarget = () => {
      if (window.location.hash === "#compose" && composer.current) composer.current.open = true;
    };
    const openFromLink = (event: MouseEvent) => {
      const target = event.target;
      const link = target instanceof Element ? target.closest<HTMLAnchorElement>('a[href$="#compose"]') : null;
      if (link && new URL(link.href).pathname === window.location.pathname && composer.current)
        composer.current.open = true;
    };
    openTarget();
    window.addEventListener("hashchange", openTarget);
    document.addEventListener("click", openFromLink);
    return () => {
      window.removeEventListener("hashchange", openTarget);
      document.removeEventListener("click", openFromLink);
    };
  }, []);
  useEffect(() => {
    if (!handoff) return;
    const restore = () => {
      const prepared = readGuidedIntakeHandoff(handoff.companyId,handoff.userId,handoff.assignmentId,handoff.base,"post");
      setGuided(prepared);
      if (prepared && composer.current) composer.current.open = true;
    };
    restore();
    window.addEventListener("erpx:guided-intake-handoff", restore);
    return () => window.removeEventListener("erpx:guided-intake-handoff", restore);
  }, [handoff]);
  function clearPrepared() {
    if (handoff) clearGuidedIntakeHandoff(handoff.companyId,handoff.userId,handoff.assignmentId,handoff.base,"post");
    setGuided(null);
  }
  function usePrepared() {
    if (!guided) return;
    const prepared = preparedPostFields(guided);
    setDraft(null);
    setBlocks([textBlock(prepared.body)]);
    setPurpose(prepared.purpose);
    setAudience("");
    setParticipants([]);
    setSources([]);
    clearPrepared();
    setMessage("Prepared details added. Review the message and choose an audience before sharing.");
  }

  function focusText(id: string, caret: number) {
    setFocus(current => ({ id, caret, signal: (current?.signal ?? 0) + 1 }));
  }
  function addPhotos(files: File[], target?: Selection) {
    if (!files.length) return;
    const supported = files.filter(file => allowedPhotoTypes.has(file.type) && file.size > 0 && file.size <= maxPhotoBytes);
    const available = Math.max(0, 10 - blocks.filter(block => block.type === "image").length);
    const added = supported.slice(0, available);
    if (added.length) {
      const spot = target ?? selection.current;
      const currentIndex = spot ? blocks.findIndex(block => block.type === "text" && block.id === spot.id) : -1;
      const index = currentIndex >= 0 ? currentIndex : blocks.findLastIndex(block => block.type === "text");
      const current = blocks[index] as TextBlock;
      const start = currentIndex >= 0 ? Math.min(spot!.start, current.text.length) : current.text.length;
      const end = currentIndex >= 0 ? Math.min(spot!.end, current.text.length) : start;
      const before = { ...current, text: current.text.slice(0, start),
        mentions: current.mentions.filter(m => m.end <= start) };
      const after = textBlock(current.text.slice(end), current.mentions.filter(m => m.start >= end)
        .map(m => ({ ...m, start: m.start - end, end: m.end - end })));
      const images: ImageBlock[] = added.map(file => ({ type: "image", slotId: crypto.randomUUID(),
        attachmentId: null, name: file.name || "pasted-image.png", alt: file.name || "Pasted image", file }));
      const next = [...blocks.slice(0, index), before, ...images, after, ...blocks.slice(index + 1)];
      if (postBody(next).length > maxPostLength) {
        setMessage("This message is too long to add images here.");
        return;
      }
      setBlocks(next);
      selection.current = { id: after.id, start: 0, end: 0 };
      focusText(after.id, 0);
    }
    if (files.some(file => !allowedPhotoTypes.has(file.type)))
      setMessage("Posts accept JPEG, PNG, and WebP images.");
    else if (files.some(file => file.size < 1 || file.size > maxPhotoBytes))
      setMessage("Each image must be between 1 byte and 10 MB.");
    else if (files.length > available)
      setMessage("You can attach up to 10 images to an update.");
    else if (added.length)
      setMessage(`${added.length === 1 ? "Image" : `${added.length} images`} attached.`);
  }
  function updateText(id: string, text: string, mentions: Mention[]) {
    const next = blocks.map(block => block.type === "text" && block.id === id ? { ...block, text, mentions } : block);
    if (postBody(next).length > maxPostLength) {
      setMessage("Posts can contain up to 3,000 characters of text.");
      return;
    }
    setBlocks(next);
  }
  function reselectImage(slotId: string, file: File) {
    if (!allowedPhotoTypes.has(file.type) || file.size < 1 || file.size > maxPhotoBytes) {
      setMessage("Select a JPEG, PNG, or WebP image no larger than 10 MB.");
      return;
    }
    setBlocks(current => current.map(block => block.type === "image" && block.slotId === slotId
      ? { ...block, file, name: file.name, alt: block.alt || file.name, uploadUrl: undefined } : block));
    setMessage("Image ready to upload.");
  }
  async function save(publish = false) {
    const choice = meta.audiences?.find(option => option.value === audience);
    if (publish && !choice) {
      setMessage("Choose who can see this post before sharing.");
      return;
    }
    const audienceInput = choice
      ? choice.kind === "company" ? { kind: "company" as const }
        : choice.kind === "participants" ? { kind: "participants" as const, userIds: participants }
        : { kind: choice.kind, id: choice.value.slice(choice.kind.length + 1) }
      : null;
    if (publish && choice?.kind === "participants" && !participants.length) {
      setMessage("Select at least one person before sharing.");
      return;
    }
    await run(async () => {
      let working = blocks;
      const body = postBody(working);
      const mentions = postMentions(working);
      const saved = await send<{ id: string; version: number }>(`${api}/beta/drafts${draft ? `/${draft.id}` : ""}`,
        { body, mentions, content: wireContent(working), audience: audienceInput, sources, purpose,
          expectedVersion: draft?.version ?? 1 });
      let version = saved.version;
      const updateDraft = (content: EditorBlock[], currentVersion: number) => setDraft({
        id: saved.id, version: currentVersion, body: postBody(content),
        branch_id: choice?.kind === "branch" ? choice.value.slice("branch:".length) : null,
        audience_kind: choice?.kind ?? null, purpose,
        audience_id: choice && (choice.kind === "team" || choice.kind === "project")
          ? choice.value.slice(choice.kind.length + 1) : null,
        participants_json: JSON.stringify(participants), sources_json: JSON.stringify(sources),
        mentions_json: JSON.stringify(postMentions(content)), content_json: JSON.stringify(wireContent(content)),
      });
      updateDraft(working, version);
      for (const image of working.filter((block): block is ImageBlock =>
        block.type === "image" && !!block.file && !block.attachmentId)) {
        let uploadUrl = image.uploadUrl;
        if (!uploadUrl) {
          const intent = await send<{ uploadUrl: string }>(`${api}/uploads`, {
            ownerType: "social_draft", ownerId: saved.id, purpose: "post-image",
            filename: image.name, mediaType: image.file!.type, bytes: image.file!.size,
          });
          uploadUrl = intent.uploadUrl;
          working = working.map(block => block.type === "image" && block.slotId === image.slotId
            ? { ...block, uploadUrl } : block);
          setBlocks(working);
        }
        const response = await workFetch(uploadUrl, { method: "PUT", body: image.file });
        const result = (await response.json()) as {
          data?: { id: string; ownerVersion: number }; error?: { message: string };
        };
        if (!response.ok || !result.data) {
          if (response.status === 404) {
            working = working.map(block => block.type === "image" && block.slotId === image.slotId
              ? { ...block, uploadUrl: undefined } : block);
            setBlocks(working);
          }
          throw new Error(result.error?.message ?? "Image upload failed. Your draft is still here; try again.");
        }
        version = result.data.ownerVersion;
        working = working.map(block => block.type === "image" && block.slotId === image.slotId
          ? { ...block, attachmentId: result.data!.id, file: undefined, uploadUrl: undefined } : block);
        setBlocks(working);
        updateDraft(working, version);
      }
      if (working.some(block => block.type === "image" && block.attachmentId === null && !block.file)) {
        setMessage("Draft saved. Reselect missing images before sharing.");
        return;
      }
      if (version !== saved.version) {
        const finalized = await send<{ id: string; version: number }>(`${api}/beta/drafts/${saved.id}`, {
          body: postBody(working), mentions: postMentions(working), content: wireContent(working),
          audience: audienceInput, purpose, expectedVersion: version,
        });
        version = finalized.version;
        updateDraft(working, version);
      }
      const refreshed = await betaFetch<Meta>(`${api}/beta/meta`);
      setMeta(refreshed);
      version = refreshed.drafts.find(row => row.id === saved.id)?.version ?? version;
      updateDraft(working, version);
      if (publish) {
        await send(`${api}/beta/drafts/${saved.id}/publish`, { expectedVersion: version });
        setBlocks([textBlock()]);
        setDraft(null);
        setAudience("");
        setParticipants([]);
        setSources([]);
        setPurpose("update");
        selection.current = null;
        setMessage(`Shared with ${choice?.label ?? "your selected audience"}.`);
        await onPublished();
      } else setMessage("Draft saved. Only you can see it.");
    });
  }
  const hasImage = blocks.some(block => block.type === "image");
  const sourceOptions = [
    ...meta.tasks.map(task => ({ kind: "staff_task" as const, id: task.id, label: `Task · ${task.title}` })),
    ...(meta.projects ?? []).map(project => ({ kind: "staff_project" as const, id: project.id,
      label: `Project · ${project.title}` })),
  ];
  return <>
    <details ref={composer} className="card beta-composer" onToggle={event => {
      if (event.currentTarget.open) {
        loadMeta();
        const first = blocks.find((block): block is TextBlock => block.type === "text");
        if (first) focusText(first.id, first.text.length);
      }
    }}>
      <summary>
        <PersonAvatar name={name} />
        <span className="beta-composer-prompt"><strong>Share an update</strong>
          <small>Ask a question or share progress with your team</small></span>
        <PenLine className="beta-compose-icon" size={18} aria-hidden="true" />
        <ChevronDown className="beta-compose-chevron" size={16} aria-hidden="true" />
      </summary>
      {guided && <div className="notice" role="status">
        <p>AI prepared an update. Review and edit it, then choose who can see it before sharing. Using it starts a new post.</p>
        <button type="button" className="button secondary" onClick={usePrepared}>Use prepared details</button>{" "}
        <button type="button" className="button ghost" onClick={clearPrepared}>Discard suggestion</button>
      </div>}
      <div className="beta-post-editor" aria-label="Post content">
        {blocks.map((block, index) => block.type === "text" ? (
          <SocialTextInput key={block.id} api={api}
            label={index === 0 ? "Post message" : `Post message continuation ${index}`}
            disabled={!ready || busy} value={block.text} mentions={block.mentions}
            onChange={(text, mentions) => updateText(block.id, text, mentions)}
            onPasteImages={(files, range) => addPhotos(files, { id: block.id, ...range })}
            onSelectionChange={range => { selection.current = { id: block.id, ...range }; }}
            focusSignal={focus?.id === block.id ? focus.signal : undefined} focusCaret={focus?.caret}
            showHint={index === 0} maxLength={maxPostLength}
            placeholder={index === 0 ? "Share a win, ask for help, paste an image, or mention @ERPX assistant…" : "Continue writing…"} />
        ) : (
          <ImagePreview key={block.slotId} block={block} api={api}
            remove={() => setBlocks(current => removeImage(current, block.slotId))}
            describe={alt => setBlocks(current => current.map(row =>
              row.type === "image" && row.slotId === block.slotId ? { ...row, alt } : row))}
            reselect={file => reselectImage(block.slotId, file)} />
        ))}
      </div>
      <div className="beta-composer-actions">
        <label>Post type
          <select aria-label="Post type" value={purpose}
            onChange={event => setPurpose(event.target.value as PostPurpose)}>
            <option value="update">Update</option>
            <option value="plan">Plan</option>
            <option value="blocker">Blocker</option>
            <option value="handover">Handover</option>
            <option value="announcement">Announcement</option>
          </select>
        </label>
        <label>Share with
          <select aria-label="Post audience" value={audience} onChange={event => setAudience(event.target.value)}>
            <option value="">Choose audience…</option>
            {(meta.audiences ?? []).map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </label>
        <span className="small" aria-live="polite">
          {meta.audiences?.find(option => option.value === audience)?.label ?? "Audience required before sharing"}
        </span>
        {audience === "participants" && <fieldset className="beta-audience-people">
          <legend>People who can see this post</legend>
          <p className="small">The author can always see this post. Your access scope still applies; mentions do not add viewers.</p>
          <div className="beta-audience-people-list">
            {meta.people.map(person =>
              <label key={person.id}><input type="checkbox" checked={participants.includes(person.id)}
                disabled={!participants.includes(person.id) && participants.length >= 20}
                onChange={event => setParticipants(current => event.target.checked
                  ? [...current, person.id] : current.filter(id => id !== person.id))} /> {person.name}</label>)}
          </div>
        </fieldset>}
        <div className="beta-post-sources">
          <label>Link existing work
            <select aria-label="Link existing work" value={sourceChoice}
              onChange={event => setSourceChoice(event.target.value)}>
              <option value="">Choose a task or project…</option>
              {sourceOptions.filter(option => !sources.some(s => s.kind === option.kind && s.id === option.id))
                .map(option => <option key={`${option.kind}:${option.id}`} value={`${option.kind}:${option.id}`}>{option.label}</option>)}
            </select>
          </label>
          <button type="button" className="button secondary" disabled={!sourceChoice || sources.length >= 5}
            onClick={() => { const option = sourceOptions.find(item => `${item.kind}:${item.id}` === sourceChoice);
              if (option && !sources.some(s => s.kind === option.kind && s.id === option.id))
                setSources(current => [...current, { kind: option.kind, id: option.id }]);
              setSourceChoice(""); }}>Link</button>
          {sources.map(source => <span key={`${source.kind}:${source.id}`} className="beta-source-chip">
            {sourceOptions.find(option => option.kind === source.kind && option.id === source.id)?.label ?? "Linked work"}
            <button type="button" aria-label="Remove linked work" onClick={() => setSources(current => current.filter(item =>
              item.kind !== source.kind || item.id !== source.id))}>×</button>
          </span>)}
        </div>
        <label className="button secondary beta-photo-input"><ImagePlus size={15} /> Photos
          <input aria-label="Add photos" type="file" accept="image/png,image/jpeg,image/webp" multiple
            onChange={event => { addPhotos(Array.from(event.target.files ?? [])); event.target.value = ""; }} />
        </label>
        <button className="button secondary" disabled={busy} onClick={() => void save()}>Save draft</button>
        <button className="button" disabled={busy || (!postBody(blocks).trim() && !hasImage)}
          onClick={() => void save(true)}>Share {purpose}</button>
      </div>
      {meta.drafts.length > 0 && <details>
        <summary>Your private drafts ({meta.drafts.length})</summary>
        {meta.drafts.map(item => <button className="beta-draft" key={item.id} onClick={() => {
          const restored = restoredBlocks(item);
          setDraft(item);
          setBlocks(restored);
          // A restored draft needs a fresh audience choice before it is shared.
          setAudience("");
          setParticipants(JSON.parse(item.participants_json ?? "[]") as string[]);
          setSources(JSON.parse(item.sources_json ?? "[]") as Source[]);
          setPurpose(item.purpose ?? "update");
          selection.current = null;
          const first = restored.find((row): row is TextBlock => row.type === "text");
          if (first) focusText(first.id, first.text.length);
        }}>{item.body || "Photo update draft"}</button>)}
      </details>}
    </details>
    <p className="beta-status" role="status">{busy ? "Saving…" : message}</p>
  </>;
}
