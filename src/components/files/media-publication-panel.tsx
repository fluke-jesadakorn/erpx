"use client";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import Image from "next/image";
import { ArrowDown, ArrowUp, Check, ExternalLink, Globe2, RotateCcw } from "lucide-react";
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
} from "@/lib/files/attachment-client";
import {
  canReorderAttachmentPublications,
  canRevokeAttachmentPublication,
} from "@/lib/files/attachment-state";
import { useAttachmentFeatureMode } from "@/components/files/attachment-feature-context";
import styles from "@/components/files/media-publication-panel.module.css";
import { type UiAppearance } from "@/components/ui/foundation";

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

export function MediaPublicationPanel(props: (MediaPublicationPanelProps) & { appearance?: UiAppearance }) {
  const mode = useAttachmentFeatureMode();
  if (mode === "disabled") return null;
  return (
    <MediaPublicationPanelContent appearance={(props.appearance ?? "legacy")}
      {...props}
      canCreate={props.canCreate === true && mode !== "read-only"}
      canReview={props.canReview === true && mode !== "read-only"}
    />
  );
}

function MediaPublicationPanelContent({ appearance = "legacy",
  base,
  targetType,
  targetId,
  purposeKey,
  sourceComponentKind = "record",
  title = "Public media",
  canCreate = false,
  canReview = false,
}: (MediaPublicationPanelProps) & { appearance?: UiAppearance }) {
  const [manifest, setManifest] = useState<MediaPublicationListDto | null>(null);
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
        nextSources.some((source) => source.id === current) ? current : (nextSources[0]?.id ?? ""),
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
      setError(cause instanceof Error ? cause.message : "Unable to load public media.");
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
    () => (manifest?.items ?? []).filter((item) => sourceById.has(item.source_link_id)),
    [manifest, sourceById],
  );
  const activeItems = useMemo(
    () =>
      visibleItems
        .filter((item) => item.state !== "revoked")
        .sort((left, right) => left.ordinal - right.ordinal || left.revision - right.revision),
    [visibleItems],
  );

  async function perform(action: () => Promise<unknown>) {
    setBusy(true);
    setError("");
    try {
      await action();
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to update public media.");
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
    void perform(() => approveMediaPublication(base, item.id, crypto.randomUUID()));
  }

  function revoke(item: MediaPublicationDto) {
    if (
      !canRevokeAttachmentPublication({ canReview, itemState: item.state }) ||
      revokeReason.trim().length < 3
    )
      return;
    void perform(async () => {
      await revokeMediaPublication(base, item.id, revokeReason.trim(), crypto.randomUUID());
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
      (left, right) => left.ordinal - right.ordinal || left.revision - right.revision,
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
    <section data-ui-appearance={appearance} className={`card ${styles.panel}`}>
      <div data-ui-appearance={appearance} className={styles.heading}>
        <div data-ui-appearance={appearance}>
          <h2 data-ui-appearance={appearance}>{title}</h2>
          <p data-ui-appearance={appearance}>
            Only a sanitized derivative can become public. Each revision needs alt text and approval
            from another authorized reviewer.
          </p>
        </div>
        <span data-ui-appearance={appearance}>
          <Globe2 aria-hidden size={16} /> {activeItems.length} active
        </span>
      </div>

      {error && (
        <p data-ui-appearance={appearance} className={styles.error} role="alert">
          {error}
        </p>
      )}

      {canCreate && (
        <div data-ui-appearance={appearance} className={styles.create}>
          <Field appearance={appearance}>
            Bound private source
            <Select appearance={appearance}
              value={sourceLinkId}
              disabled={busy || sources.length === 0}
              onChange={(event) => setSourceLinkId(event.target.value)}
            >
              {sources.length === 0 && <option data-ui-appearance={appearance} value="">Bind a clean image first</option>}
              {sources.map((source) => (
                <option data-ui-appearance={appearance} key={source.id} value={source.id}>
                  {source.filename}
                  {source.caption ? ` · ${source.caption}` : ""}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Public alt text
            <TextInput appearance={appearance}
              value={altText}
              maxLength={500}
              disabled={busy || !sourceLinkId}
              onChange={(event) => setAltText(event.target.value)}
              placeholder="Describe the image for customers using screen readers"
            />
          </Field>
          <Button appearance={appearance}
            variant="primary"
            type="button"

            disabled={busy || !sourceLinkId || !altText.trim()}
            onClick={create}
          >
            Create publication revision
          </Button>
        </div>
      )}

      {manifest && visibleItems.length > 0 ? (
        <ol data-ui-appearance={appearance} className={styles.items}>
          {[...visibleItems]
            .sort((left, right) => left.ordinal - right.ordinal || left.revision - right.revision)
            .map((item) => {
              const source = sourceById.get(item.source_link_id);
              const activeIndex = activeItems.findIndex((candidate) => candidate.id === item.id);
              const imageUrl =
                item.state === "approved"
                  ? publicMediaPublicationUrl(base, item.id)
                  : source
                    ? attachmentContentUrl(base, source.id, "preview")
                    : null;
              return (
                <li data-ui-appearance={appearance} key={item.id} className={styles.item} data-state={item.state}>
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
                    <div data-ui-appearance={appearance} className={styles.thumbnailPlaceholder}>Preview unavailable</div>
                  )}
                  <div data-ui-appearance={appearance} className={styles.details}>
                    <strong data-ui-appearance={appearance}>
                      Revision {item.revision} · {item.state}
                    </strong>
                    <span data-ui-appearance={appearance}>{item.alt_text}</span>
                    <small data-ui-appearance={appearance}>
                      Order {item.ordinal + 1} · {source?.filename ?? "retained source"}
                    </small>
                    {item.revocation_reason && <small data-ui-appearance={appearance}>Revoked: {item.revocation_reason}</small>}
                  </div>
                  <div data-ui-appearance={appearance} className={styles.actions}>
                    {canReorderAttachmentPublications({
                      canCreate,
                      canReview,
                      sourceComponentKind,
                      itemState: item.state,
                      activeCount: activeItems.length,
                    }) && (
                      <>
                        <Button appearance={appearance}
                          variant="plain"
                          type="button"
                          aria-label={`Move revision ${item.revision} earlier`}
                          disabled={busy || activeIndex === 0}
                          onClick={() => move(item, -1)}
                        >
                          <ArrowUp aria-hidden size={15} />
                        </Button>
                        <Button appearance={appearance}
                          variant="plain"
                          type="button"
                          aria-label={`Move revision ${item.revision} later`}
                          disabled={busy || activeIndex === activeItems.length - 1}
                          onClick={() => move(item, 1)}
                        >
                          <ArrowDown aria-hidden size={15} />
                        </Button>
                      </>
                    )}
                    {item.state === "draft" && canReview && (
                      <Button appearance={appearance}
                        variant="plain"
                        type="button"
                        disabled={busy}
                        onClick={() => approve(item)}
                      >
                        <Check aria-hidden size={15} /> Approve
                      </Button>
                    )}
                    {item.state === "approved" && (
                      <a data-ui-appearance={appearance}
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
                      <Button appearance={appearance}
                        variant="plain"
                        type="button"
                        disabled={busy}
                        onClick={() => {
                          setRevokeId(item.id);
                          setRevokeReason("");
                        }}
                      >
                        <RotateCcw aria-hidden size={15} /> Revoke
                      </Button>
                    )}
                  </div>
                  {canRevokeAttachmentPublication({
                    canReview,
                    itemState: item.state,
                  }) &&
                    revokeId === item.id && (
                      <div data-ui-appearance={appearance} className={styles.revoke}>
                        <Field appearance={appearance}>
                          Revocation reason
                          <TextArea appearance={appearance}
                            value={revokeReason}
                            minLength={3}
                            maxLength={1000}
                            onChange={(event) => setRevokeReason(event.target.value)}
                          />
                        </Field>
                        <Button appearance={appearance}
                          variant="secondary"
                          type="button"

                          disabled={busy || revokeReason.trim().length < 3}
                          onClick={() => revoke(item)}
                        >
                          Confirm revocation
                        </Button>
                        <Button appearance={appearance}
                          variant="secondary"
                          type="button"

                          disabled={busy}
                          onClick={() => setRevokeId(null)}
                        >
                          Cancel
                        </Button>
                      </div>
                    )}
                </li>
              );
            })}
        </ol>
      ) : (
        <p data-ui-appearance={appearance}>No public media revisions yet.</p>
      )}
    </section>
  );
}
