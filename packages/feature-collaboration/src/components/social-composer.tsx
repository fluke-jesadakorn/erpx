"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import socialStyles from "@erpx/feature-collaboration/components/social.module.css";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { PersonAvatar } from "@erpx/ui/components/person-avatar";
import dynamic from "next/dynamic";
import {
  newPostTextBlock,
  postContentBody,
  postContentMentions,
  postWireContent,
  postWireRichContent,
  postEditorBlocksFromStored,
  type PostEditorBlock,
} from "@erpx/feature-collaboration/lib/social-post-editor-state";
import type { RichContent } from "@erpx/feature-shared/lib/home-rich-content";
import type { Mention } from "@erpx/server-collaboration/public/rules/social-text";
import type { PostContent } from "@erpx/server-collaboration/public/social-home";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { PenLine, ChevronDown } from "lucide-react";
import { betaFetch } from "@erpx/feature-shared/lib/beta-client";
import {SocialCommandRecovery} from "@erpx/feature-collaboration/components/social-command-recovery";
import { useSocialCommand } from "@erpx/feature-collaboration/components/social-command";
import type { Meta, Draft, PostPurpose } from "@erpx/feature-collaboration/components/social-types";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@erpx/feature-shared/lib/ai/guided-intake";
import { preparedPostFields } from "@erpx/feature-shared/lib/ai/prepared-work";
const SocialPostContentEditor = dynamic(
  () => import("@erpx/feature-collaboration/components/social-post-content-editor").then((module) => module.SocialPostContentEditor),
  { ssr: false, loading: () => <InlineNotice tone="info" role="status">Loading editor…</InlineNotice> },
);

type Source = { kind: "staff_task" | "staff_project"; id: string };

const subscribeHydration = () => () => {};
const maxPostLength = 30000;
function restoredBlocks(draft: Draft): PostEditorBlock[] {
  const globalMentions = JSON.parse(draft.mentions_json ?? "[]") as Mention[];
  let content: PostContent | null = null;
  try {
    const parsed: unknown = draft.content_json ? JSON.parse(draft.content_json) : null;
    if (Array.isArray(parsed)) content = parsed as PostContent;
  } catch {
    /* Legacy drafts use body and mentions. */
  }
  let rich: RichContent | null = null;
  try {
    rich = draft.rich_content_json ? (JSON.parse(draft.rich_content_json) as RichContent) : null;
  } catch {}
  return postEditorBlocksFromStored(content, rich, draft.body, globalMentions);
}

export function SocialComposer({
  api,
  name,
  meta,
  setMeta,
  loadMeta,
  onPublished,
  initialOpen = false,
}: {
  initialOpen?: boolean;
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
  const command=useSocialCommand();
  const {run,send,uploadImage,busy,message,setMessage}=command;
  const [blocks, setBlocks] = useState<PostEditorBlock[]>([
    { type: "text", id: "initial", text: "", mentions: [] },
  ]);
  const [audience, setAudience] = useState("");
  const [participants, setParticipants] = useState<string[]>([]);
  const [sources, setSources] = useState<Source[]>([]);
  const [sourceChoice, setSourceChoice] = useState("");
  const [purpose, setPurpose] = useState<PostPurpose>("update");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [guided, setGuided] = useState<Record<string, string> | null>(null);
  const composer = useRef<HTMLDetailsElement>(null);
  const [composerRevision, setComposerRevision] = useState(0);
  const [editorLoaded, setEditorLoaded] = useState(initialOpen);
  const [editorResetKey, setEditorResetKey] = useState(0);
  const [focusSignal, setFocusSignal] = useState(0);
  const handoff = meta.handoffContext;
  useEffect(() => {
    const openTarget = () => {
      if (window.location.hash === "#compose" && composer.current) composer.current.open = true;
    };
    const openFromLink = (event: MouseEvent) => {
      const target = event.target;
      const link =
        target instanceof Element ? target.closest<HTMLAnchorElement>('a[href$="#compose"]') : null;
      if (link && new URL(link.href).pathname === window.location.pathname && composer.current)
        composer.current.open = true;
    };
    const fromEvent = () => { if (composer.current) composer.current.open = true; };
    window.addEventListener("erpx:open-compose", fromEvent);
    if (initialOpen) fromEvent();
    openTarget();
    window.addEventListener("hashchange", openTarget);
    document.addEventListener("click", openFromLink);
    return () => {
      window.removeEventListener("erpx:open-compose", fromEvent);
      window.removeEventListener("hashchange", openTarget);
      document.removeEventListener("click", openFromLink);
    };
  }, [initialOpen]);
  useEffect(() => {
    if (!handoff) return;
    const restore = () => {
      const prepared = readGuidedIntakeHandoff(
        handoff.companyId,
        handoff.userId,
        handoff.assignmentId,
        handoff.base,
        "post",
      );
      setGuided(prepared);
      if (prepared && composer.current) composer.current.open = true;
    };
    restore();
    window.addEventListener("erpx:guided-intake-handoff", restore);
    return () => window.removeEventListener("erpx:guided-intake-handoff", restore);
  }, [handoff]);
  function clearPrepared() {
    if (handoff)
      clearGuidedIntakeHandoff(
        handoff.companyId,
        handoff.userId,
        handoff.assignmentId,
        handoff.base,
        "post",
      );
    setGuided(null);
  }
  function usePrepared() {
    if (!guided) return;
    const prepared = preparedPostFields(guided);
    setDraft(null);
    setBlocks([newPostTextBlock(prepared.body)]);
    setEditorResetKey((value) => value + 1);
    setPurpose(prepared.purpose);
    setAudience("");
    setParticipants([]);
    setSources([]);
    clearPrepared();
    setMessage("Prepared details added. Review the message and choose an audience before sharing.");
  }

  async function save(publish = false) {
    const submittedForm=document.activeElement?.closest("form"),editVersion=submittedForm?.getAttribute("data-erpx-edit-version");
    const choice = meta.audiences?.find((option) => option.value === audience);
    if (publish && !choice) {
      setMessage("Choose who can see this post before sharing.");
      return;
    }
    const audienceInput = choice
      ? choice.kind === "company"
        ? { kind: "company" as const }
        : choice.kind === "participants"
          ? { kind: "participants" as const, userIds: participants }
          : { kind: choice.kind, id: choice.value.slice(choice.kind.length + 1) }
      : null;
    if (publish && choice?.kind === "participants" && !participants.length) {
      setMessage("Select at least one person before sharing.");
      return;
    }
    await run(async () => {
      let working = blocks;
      const body = postContentBody(working);
      const mentions = postContentMentions(working);
      const saved = await send<{ id: string; version: number }>(
        `${api}/beta/drafts${draft ? `/${draft.id}` : ""}`,
        {
          body,
          mentions,
          content: postWireContent(working),
          richContent: postWireRichContent(working),
          audience: audienceInput,
          sources,
          purpose,
          expectedVersion: draft?.version ?? 1,
        },
      );
      let version = saved.version;
      const updateDraft = (content: PostEditorBlock[], currentVersion: number) =>
        setDraft({
          id: saved.id,
          version: currentVersion,
          body: postContentBody(content),
          branch_id: choice?.kind === "branch" ? choice.value.slice("branch:".length) : null,
          audience_kind: choice?.kind ?? null,
          purpose,
          audience_id:
            choice && (choice.kind === "team" || choice.kind === "project")
              ? choice.value.slice(choice.kind.length + 1)
              : null,
          participants_json: JSON.stringify(participants),
          sources_json: JSON.stringify(sources),
          mentions_json: JSON.stringify(postContentMentions(content)),
          content_json: JSON.stringify(postWireContent(content)),
          rich_content_json: JSON.stringify(postWireRichContent(content)),
        });
      updateDraft(working, version);
      for (const image of working.filter(
        (block): block is Extract<PostEditorBlock, { type: "image" }> =>
          block.type === "image" && !!block.file && !block.attachmentId,
      )) {
        let uploadUrl = image.uploadUrl;
        if (!uploadUrl) {
          const intent = await send<{ uploadUrl: string }>(`${api}/uploads`, {
            ownerType: "social_draft",
            ownerId: saved.id,
            purpose: "post-image",
            filename: image.name,
            mediaType: image.file!.type,
            bytes: image.file!.size,
          });
          uploadUrl = intent.uploadUrl;
          working = working.map((block) =>
            block.type === "image" && block.slotId === image.slotId
              ? { ...block, uploadUrl }
              : block,
          );
          setBlocks(working);
        }
        const result={data:await uploadImage(uploadUrl!,image.file!)};
        version = result.data.ownerVersion;
        working = working.map((block) =>
          block.type === "image" && block.slotId === image.slotId
            ? {
                ...block,
                attachmentId: result.data!.id,
                file: undefined,
                uploadUrl: undefined,
                previewUrl: undefined,
              }
            : block,
        );
        setBlocks(working);
        updateDraft(working, version);
      }
      if (
        working.some(
          (block) => block.type === "image" && block.attachmentId === null && !block.file,
        )
      ) {
        setMessage("Draft saved. Reselect missing images before sharing.");
        return;
      }
      if (version !== saved.version) {
        const finalized = await send<{ id: string; version: number }>(
          `${api}/beta/drafts/${saved.id}`,
          {
            body: postContentBody(working),
            mentions: postContentMentions(working),
            content: postWireContent(working),
            richContent: postWireRichContent(working),
            audience: audienceInput,
            purpose,
            expectedVersion: version,
          },
        );
        version = finalized.version;
        updateDraft(working, version);
      }
      const refreshed = await betaFetch<Meta>(`${api}/beta/meta`);
      setMeta(refreshed);
      version = refreshed.drafts.find((row) => row.id === saved.id)?.version ?? version;
      updateDraft(working, version);
      if (publish) {
        await send(`${api}/beta/drafts/${saved.id}/publish`, { expectedVersion: version });
        setBlocks([newPostTextBlock()]);
        setEditorResetKey((value) => value + 1);
        setDraft(null);
        setAudience("");
        setParticipants([]);
        setSources([]);
        setSourceChoice("");
        setPurpose("update");
        setMessage(`Shared with ${choice?.label ?? "your selected audience"}.`);
        await onPublished();
        // A published draft is committed; reopen with fresh drawer dirty state.
        setComposerRevision((value) => value + 1);
      } else {if(submittedForm&&submittedForm.getAttribute("data-erpx-edit-version")===editVersion)window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:{form:submittedForm,formEditVersion:editVersion,socialAcknowledged:true}}));setMessage("Draft saved. Only you can see it.");}
    });
  }
  const hasImage = blocks.some((block) => block.type === "image");
  const sourceOptions = [
    ...meta.tasks.map((task) => ({
      kind: "staff_task" as const,
      id: task.id,
      label: `Task · ${task.title}`,
    })),
    ...(meta.projects ?? []).map((project) => ({
      kind: "staff_project" as const,
      id: project.id,
      label: `Project · ${project.title}`,
    })),
  ];
  return (
    <>
      <FormDisclosure finalFocus={() => composer.current?.querySelector<HTMLElement>("summary") ?? null} key={composerRevision} appearance="standard" drawerTitle="Share an update" contentClassName={socialStyles.home} dismissDisabled={busy||(!!command.retryAttempt||command.imagePending)} onDiscard={()=>{setBlocks([newPostTextBlock()]);setEditorResetKey(value=>value+1);setDraft(null);setAudience("");setParticipants([]);setSources([]);setSourceChoice("");setPurpose("update");}}
        ref={composer}
        className="ui-panel beta-composer"
        onToggle={(event) => {
          if (event.currentTarget.open) {
            setEditorLoaded(true);
            loadMeta();
            setFocusSignal((value) => value + 1);
          }
        }}
      >
        <summary>
          <PersonAvatar name={name} />
          <span className="beta-composer-prompt">
            <strong>Share an update</strong>
            <small>Ask a question or share progress with your team</small>
          </span>
          <PenLine className="beta-compose-icon" size={18} aria-hidden="true" />
          <ChevronDown className="beta-compose-chevron" size={16} aria-hidden="true" />
        </summary>
        <form data-social-draft onSubmit={event=>event.preventDefault()} onChangeCapture={event=>event.currentTarget.setAttribute("data-erpx-edit-version",crypto.randomUUID())}><fieldset disabled={busy||(!!command.retryAttempt||command.imagePending)} style={{ display: "contents" }}>
        {guided && (
          <InlineNotice tone="info" className="notice" role="status">
            <p>
              AI prepared an update. Review and edit it, then choose who can see it before sharing.
              Using it starts a new post.
            </p>
            <Button appearance="standard" variant="secondary" type="button" onClick={usePrepared}>
              Use prepared details
            </Button>{" "}
            <Button appearance="standard" variant="ghost" type="button" onClick={clearPrepared}>
              Discard suggestion
            </Button>
          </InlineNotice>
        )}
        <div className="beta-post-editor" aria-label="Post content">
          {editorLoaded && (
            <SocialPostContentEditor
              api={api}
              blocks={blocks}
              onChange={setBlocks}
              resetKey={editorResetKey}
              focusSignal={focusSignal}
              maxLength={maxPostLength}
              label="Post message"
              placeholder="Share a win, ask for help, paste an image, or mention @AI…"
              disabled={!ready || busy}
            />
          )}
        </div>
        <div className="beta-composer-actions" data-drawer-form>
          <Field appearance="standard">
            Post type
            <Select appearance="standard"
              aria-label="Post type"
              value={purpose}
              onChange={(event) => setPurpose(event.target.value as PostPurpose)}
            >
              <option value="update">Update</option>
              <option value="plan">Plan</option>
              <option value="blocker">Blocker</option>
              <option value="handover">Handover</option>
              <option value="announcement">Announcement</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Share with
            <Select appearance="standard"
              aria-label="Post audience"
              value={audience}
              onChange={(event) => setAudience(event.target.value)}
            >
              <option value="">Choose audience…</option>
              {(meta.audiences ?? []).map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </Select>
          </Field>
          <span className="small" aria-live="polite">
            {meta.audiences?.find((option) => option.value === audience)?.label ??
              "Audience required before sharing"}
          </span>
          {audience === "participants" && (
            <fieldset className="beta-audience-people">
              <legend>People who can see this post</legend>
              <p className="small">
                The author can always see this post. Your access scope still applies; mentions do
                not add viewers.
              </p>
              <div className="beta-audience-people-list">
                {meta.people.map((person) => (
                  <Field appearance="standard" orientation="inline" key={person.id}>
                    <TextInput appearance="standard"
                      type="checkbox"
                      checked={participants.includes(person.id)}
                      disabled={!participants.includes(person.id) && participants.length >= 20}
                      onChange={(event) =>
                        setParticipants((current) =>
                          event.target.checked
                            ? [...current, person.id]
                            : current.filter((id) => id !== person.id),
                        )
                      }
                    />{" "}
                    {person.name}
                  </Field>
                ))}
              </div>
            </fieldset>
          )}
          <div className="beta-post-sources">
            <Field appearance="standard">
              Link existing work
              <Select appearance="standard"
                aria-label="Link existing work"
                value={sourceChoice}
                onChange={(event) => setSourceChoice(event.target.value)}
              >
                <option value="">Choose a task or project…</option>
                {sourceOptions
                  .filter(
                    (option) => !sources.some((s) => s.kind === option.kind && s.id === option.id),
                  )
                  .map((option) => (
                    <option
                      key={`${option.kind}:${option.id}`}
                      value={`${option.kind}:${option.id}`}
                    >
                      {option.label}
                    </option>
                  ))}
              </Select>
            </Field>
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={!sourceChoice || sources.length >= 5}
              onClick={() => {
                const option = sourceOptions.find(
                  (item) => `${item.kind}:${item.id}` === sourceChoice,
                );
                if (option && !sources.some((s) => s.kind === option.kind && s.id === option.id))
                  setSources((current) => [...current, { kind: option.kind, id: option.id }]);
                setSourceChoice("");
              }}
            >
              Link
            </Button>
            {sources.map((source) => (
              <span key={`${source.kind}:${source.id}`} className="beta-source-chip">
                {sourceOptions.find(
                  (option) => option.kind === source.kind && option.id === source.id,
                )?.label ?? "Linked work"}
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  aria-label="Remove linked work"
                  onClick={() =>
                    setSources((current) =>
                      current.filter((item) => item.kind !== source.kind || item.id !== source.id),
                    )
                  }
                >
                  ×
                </Button>
              </span>
            ))}
          </div>
          <div className="beta-composer-submit">
          <Button appearance="standard" type="submit" variant="secondary" disabled={busy} onClick={() => void save()}>
            Save draft
          </Button>
          <Button appearance="standard"
            type="submit"
            variant="primary"
            disabled={busy || (!postContentBody(blocks).trim() && !hasImage)}
            onClick={() => void save(true)}
          >
            Share {purpose}
          </Button>
          </div>
        </div>
        {meta.drafts.length > 0 && (
          <details>
            <summary>Your private drafts ({meta.drafts.length})</summary>
            {meta.drafts.map((item) => (
              <Button appearance="standard"
                type="submit"
                variant="ghost"
                className="beta-draft"
                key={item.id}
                onClick={() => {
                  const restored = restoredBlocks(item);
                  setDraft(item);
                  setBlocks(restored);
                  setEditorResetKey((value) => value + 1);
                  // A restored draft needs a fresh audience choice before it is shared.
                  setAudience("");
                  setParticipants(JSON.parse(item.participants_json ?? "[]") as string[]);
                  setSources(JSON.parse(item.sources_json ?? "[]") as Source[]);
                  setPurpose(item.purpose ?? "update");
                  setFocusSignal((value) => value + 1);
                }}
              >
                {item.body || "Photo update draft"}
              </Button>
            ))}
          </details>
        )}
      <InlineNotice tone="info" className="beta-status" role="status">{busy ? "Saving…" : message}</InlineNotice>
        </fieldset><SocialCommandRecovery command={command}/></form>
      </FormDisclosure>
    </>
  );
}
