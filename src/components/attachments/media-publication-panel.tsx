"use client";

import Image from "next/image";
import {
  ArrowDown,
  ArrowUp,
  Check,
  ExternalLink,
  Globe2,
  RotateCcw,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AttachmentApiError,
  approveMediaPublication,
  attachmentContentUrl,
  createMediaPublication,
  listAttachmentSets,
  listMediaPublications,
  publicMediaPublicationUrl,
  reorderMediaPublications,
  revokeMediaPublication,
  type AttachmentComponentKind,
  type AttachmentLinkDto,
  type MediaPublicationDto,
  type MediaPublicationListDto,
} from "@/lib/attachment-client";
import {
  canReorderAttachmentPublications,
  canRevokeAttachmentPublication,
} from "@/lib/attachment-state";
import { useAttachmentFeatureMode } from "./attachment-feature-context";
import styles from "./media-publication-panel.module.css";

type PublishableLink = AttachmentLinkDto & { setId: string };

type MediaPublicationPanelProps = {
  base: string;
  targetType: string;
  targetId: string;
  purposeKey: string;
  sourceComponentKind?: AttachmentComponentKind;
  title?: string;
  canCreate?: boolean;
  canReview?: boolean;
};

export function MediaPublicationPanel(props: MediaPublicationPanelProps) {
  const mode = useAttachmentFeatureMode();
  if (mode === "disabled") return null;
  return (
    <MediaPublicationPanelContent
      {...props}
      canCreate={props.canCreate === true && mode !== "read-only"}
      canReview={props.canReview === true && mode !== "read-only"}
    />
  );
}

function MediaPublicationPanelContent({
  base,
  targetType,
  targetId,
  purposeKey,
  sourceComponentKind = "record",
  title = "Public media",
  canCreate = false,
  canReview = false,
}: MediaPublicationPanelProps) {
  const [manifest, setManifest] = useState<MediaPublicationListDto | null>(
    null,
  );
  const [sources, setSources] = useState<PublishableLink[]>([]);
  const [sourceLinkId, setSourceLinkId] = useState("");
  const [altText, setAltText] = useState("");
  const [busy, setBusy] = useState(false);
  const [available, setAvailable] = useState(true);
  const [error, setError] = useState("");
  const [revokeId, setRevokeId] = useState<string | null>(null);
  const [revokeReason, setRevokeReason] = useState("");

  const load = useCallback(async () => {
    try {
      const [sets, publications] = await Promise.all([
        listAttachmentSets(base, {
          targetType,
          targetId,
          purposeKey,
          componentKind: sourceComponentKind,
        }),
        listMediaPublications(base, targetType, targetId),
      ]);
      const nextSources = sets.flatMap((set) =>
        set.state === "bound"
          ? set.links
              .filter(
                (link) =>
                  link.linkState === "bound" &&
                  link.sourceState === "available" &&
                  link.previewAvailable &&
                  link.mediaType.startsWith("image/"),
              )
              .map((link) => ({ ...link, setId: set.id }))
          : [],
      );
      setSources(nextSources);
      setSourceLinkId((current) =>
        nextSources.some((source) => source.id === current)
          ? current
          : (nextSources[0]?.id ?? ""),
      );
      setManifest(publications);
      setAvailable(true);
    } catch (cause) {
      if (
        cause instanceof AttachmentApiError &&
        cause.status === 404 &&
        cause.code === "ATTACHMENTS_DISABLED"
      ) {
        setAvailable(false);
        return;
      }
      setError(
        cause instanceof Error ? cause.message : "Unable to load public media.",
      );
    }
  }, [base, purposeKey, sourceComponentKind, targetId, targetType]);

  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 0);
    return () => window.clearTimeout(timer);
  }, [load]);

  const sourceById = useMemo(
    () => new Map(sources.map((source) => [source.id, source])),
    [sources],
  );
  const visibleItems = useMemo(
    () =>
      (manifest?.items ?? []).filter((item) =>
        sourceById.has(item.source_link_id),
      ),
    [manifest, sourceById],
  );
  const activeItems = useMemo(
    () =>
      visibleItems
        .filter((item) => item.state !== "revoked")
        .sort(
          (left, right) =>
            left.ordinal - right.ordinal || left.revision - right.revision,
        ),
    [visibleItems],
  );

  async function perform(action: () => Promise<unknown>) {
    setBusy(true);
    setError("");
    try {
      await action();
      await load();
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Unable to update public media.",
      );
    } finally {
      setBusy(false);
    }
  }

  function create() {
    if (!canCreate || !sourceLinkId || altText.trim().length < 1) return;
    void perform(async () => {
      await createMediaPublication(
        base,
        {
          sourceLinkId,
          targetType,
          targetId,
          altText: altText.trim(),
          ordinal: activeItems.length,
        },
        crypto.randomUUID(),
      );
      setAltText("");
    });
  }

  function approve(item: MediaPublicationDto) {
    if (!canReview) return;
    void perform(() =>
      approveMediaPublication(base, item.id, crypto.randomUUID()),
    );
  }

  function revoke(item: MediaPublicationDto) {
    if (
      !canRevokeAttachmentPublication({ canReview, itemState: item.state }) ||
      revokeReason.trim().length < 3
    )
      return;
    void perform(async () => {
      await revokeMediaPublication(
        base,
        item.id,
        revokeReason.trim(),
        crypto.randomUUID(),
      );
      setRevokeId(null);
      setRevokeReason("");
    });
  }

  function move(item: MediaPublicationDto, direction: -1 | 1) {
    if (
      !manifest ||
      !canReorderAttachmentPublications({
        canCreate,
        canReview,
        sourceComponentKind,
        itemState: item.state,
        activeCount: activeItems.length,
      })
    )
      return;
    const ordered = [...activeItems].sort(
      (left, right) =>
        left.ordinal - right.ordinal || left.revision - right.revision,
    );
    const index = ordered.findIndex((candidate) => candidate.id === item.id);
    const nextIndex = index + direction;
    if (index < 0 || nextIndex < 0 || nextIndex >= ordered.length) return;
    [ordered[index], ordered[nextIndex]] = [ordered[nextIndex], ordered[index]];
    void perform(() =>
      reorderMediaPublications(
        base,
        {
          targetType,
          targetId,
          expectedVersion: manifest.orderVersion,
          items: ordered.map((candidate, ordinal) => ({
            id: candidate.id,
            ordinal,
          })),
        },
        crypto.randomUUID(),
      ),
    );
  }

  if (!available) return null;

  return (
    <section className={`card ${styles.panel}`}>
      <div className={styles.heading}>
        <div>
          <h2>{title}</h2>
          <p>
            Only a sanitized derivative can become public. Each revision needs
            alt text and approval from another authorized reviewer.
          </p>
        </div>
        <span>
          <Globe2 aria-hidden size={16} /> {activeItems.length} active
        </span>
      </div>

      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}

      {canCreate && (
        <div className={styles.create}>
          <label>
            Bound private source
            <select
              value={sourceLinkId}
              disabled={busy || sources.length === 0}
              onChange={(event) => setSourceLinkId(event.target.value)}
            >
              {sources.length === 0 && (
                <option value="">Bind a clean image first</option>
              )}
              {sources.map((source) => (
                <option key={source.id} value={source.id}>
                  {source.filename}
                  {source.caption ? ` · ${source.caption}` : ""}
                </option>
              ))}
            </select>
          </label>
          <label>
            Public alt text
            <input
              value={altText}
              maxLength={500}
              disabled={busy || !sourceLinkId}
              onChange={(event) => setAltText(event.target.value)}
              placeholder="Describe the image for customers using screen readers"
            />
          </label>
          <button
            type="button"
            className="button"
            disabled={busy || !sourceLinkId || !altText.trim()}
            onClick={create}
          >
            Create publication revision
          </button>
        </div>
      )}

      {manifest && visibleItems.length > 0 ? (
        <ol className={styles.items}>
          {[...visibleItems]
            .sort(
              (left, right) =>
                left.ordinal - right.ordinal || left.revision - right.revision,
            )
            .map((item) => {
              const source = sourceById.get(item.source_link_id);
              const activeIndex = activeItems.findIndex(
                (candidate) => candidate.id === item.id,
              );
              const imageUrl =
                item.state === "approved"
                  ? publicMediaPublicationUrl(base, item.id)
                  : source
                    ? attachmentContentUrl(base, source.id, "preview")
                    : null;
              return (
                <li
                  key={item.id}
                  className={styles.item}
                  data-state={item.state}
                >
                  {imageUrl ? (
                    <Image
                      className={styles.thumbnail}
                      src={imageUrl}
                      alt={item.alt_text}
                      width={112}
                      height={84}
                      unoptimized
                    />
                  ) : (
                    <div className={styles.thumbnailPlaceholder}>
                      Preview unavailable
                    </div>
                  )}
                  <div className={styles.details}>
                    <strong>
                      Revision {item.revision} · {item.state}
                    </strong>
                    <span>{item.alt_text}</span>
                    <small>
                      Order {item.ordinal + 1} ·{" "}
                      {source?.filename ?? "retained source"}
                    </small>
                    {item.revocation_reason && (
                      <small>Revoked: {item.revocation_reason}</small>
                    )}
                  </div>
                  <div className={styles.actions}>
                    {canReorderAttachmentPublications({
                      canCreate,
                      canReview,
                      sourceComponentKind,
                      itemState: item.state,
                      activeCount: activeItems.length,
                    }) && (
                      <>
                        <button
                          type="button"
                          aria-label={`Move revision ${item.revision} earlier`}
                          disabled={busy || activeIndex === 0}
                          onClick={() => move(item, -1)}
                        >
                          <ArrowUp aria-hidden size={15} />
                        </button>
                        <button
                          type="button"
                          aria-label={`Move revision ${item.revision} later`}
                          disabled={
                            busy || activeIndex === activeItems.length - 1
                          }
                          onClick={() => move(item, 1)}
                        >
                          <ArrowDown aria-hidden size={15} />
                        </button>
                      </>
                    )}
                    {item.state === "draft" && canReview && (
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => approve(item)}
                      >
                        <Check aria-hidden size={15} /> Approve
                      </button>
                    )}
                    {item.state === "approved" && (
                      <a
                        href={publicMediaPublicationUrl(base, item.id)}
                        target="_blank"
                        rel="noreferrer"
                      >
                        <ExternalLink aria-hidden size={15} /> Public derivative
                      </a>
                    )}
                    {canRevokeAttachmentPublication({
                      canReview,
                      itemState: item.state,
                    }) && (
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => {
                          setRevokeId(item.id);
                          setRevokeReason("");
                        }}
                      >
                        <RotateCcw aria-hidden size={15} /> Revoke
                      </button>
                    )}
                  </div>
                  {canRevokeAttachmentPublication({
                    canReview,
                    itemState: item.state,
                  }) &&
                    revokeId === item.id && (
                      <div className={styles.revoke}>
                        <label>
                          Revocation reason
                          <textarea
                            value={revokeReason}
                            minLength={3}
                            maxLength={1000}
                            onChange={(event) =>
                              setRevokeReason(event.target.value)
                            }
                          />
                        </label>
                        <button
                          type="button"
                          className="button secondary"
                          disabled={busy || revokeReason.trim().length < 3}
                          onClick={() => revoke(item)}
                        >
                          Confirm revocation
                        </button>
                        <button
                          type="button"
                          className="button secondary"
                          disabled={busy}
                          onClick={() => setRevokeId(null)}
                        >
                          Cancel
                        </button>
                      </div>
                    )}
                </li>
              );
            })}
        </ol>
      ) : (
        <p>No public media revisions yet.</p>
      )}
    </section>
  );
}
