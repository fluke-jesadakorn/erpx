"use client";
import { ActionAnchor as UiActionAnchor } from "@erpx/ui/components/foundation";

import { Button, Field, InlineNotice, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";

import Image from "next/image";
import {
  Camera,
  Download,
  Eye,
  FileText,
  Image as ImageIcon,
  LoaderCircle,
  LockKeyhole,
  Paperclip,
  RefreshCw,
  ShieldCheck,
  Trash2,
  TriangleAlert,
  Upload,
} from "lucide-react";
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import {
  AttachmentApiError,
  type AttachmentComponentKind,
  type AttachmentLinkDto,
  type AttachmentSetDto,
  attachmentContentUrl,
  attachmentMediaType,
  attachmentStatus,
  attestAttachmentSetSanitized,
  bindRecordAttachmentSet,
  createAttachmentSet,
  formatAttachmentBytes,
  listAttachmentSets,
  reserveAttachmentFile,
  uploadAttachmentBytes,
  validateGenericAttachment,
  withdrawAttachmentLink,
} from "@erpx/feature-files/lib/attachment-client";
import {fieldWithdrawalScope,pendingFieldWithdrawal,registerFieldWithdrawalOwner,retryFieldWithdrawal,submitFieldWithdrawal,subscribeFieldWithdrawal} from "@erpx/feature-files/lib/native-field-withdraw";
import {pendingFieldBind,registerFieldBindOwner,retryFieldBind,submitFieldBind,subscribeFieldBind} from "@erpx/feature-files/lib/native-field-bind";
import { attachmentStateLabel, canMutateAttachmentDraft } from "@erpx/feature-files/lib/attachment-state";
import { canMutateAttachments, useAttachmentFeatureMode } from "@erpx/feature-files/components/attachment-feature-context";
import styles from "@erpx/feature-files/components/attachment-panel.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";

type LocalLink = AttachmentLinkDto & {
  localState?: "reserving" | "uploading" | "waiting";
  localError?: string;
};

export type AttachmentPanelChange = {
  setId: string | null;
  links: AttachmentLinkDto[];
  readyCount: number;
  requiredCount: number;
  bindable: boolean;
};

export type AttachmentPanelProps = {
  base: string;
  targetType: string;
  targetId: string;
  expectedTargetVersion: number;
  componentKind: AttachmentComponentKind;
  componentRef?: string;
  purposeKey: string;
  title?: string;
  description?: string;
  visibility?: string;
  compact?: boolean;
  disabled?: boolean;
  interactionDisabled?: boolean;
  frozen?: boolean;
  requiredCount?: number;
  maxFiles?: number;
  initialSet?: AttachmentSetDto | null;
  fieldName?: string;
  onChange?: (change: AttachmentPanelChange) => void;
  onActivityChange?: (active: boolean) => void;
  onAvailabilityChange?: (available: boolean) => void;
  bindMode?: "action" | "record";
  onBound?: (result: { setId: string; version: number }) => void;
  readOnly?: boolean;
  showHistory?: boolean;
  allowOriginalDownload?: boolean;
  nativeWithdrawIds?: readonly string[];
  nativeBindIds?: readonly string[];
};

const active = (link: LocalLink) => !["withdrawn", "expired"].includes(link.linkState);
const ready = (link: LocalLink) => active(link) && link.sourceState === "available";
const terminal = (link: LocalLink) =>
  ["available", "scan_error", "malware_rejected", "destroyed"].includes(link.sourceState ?? "") ||
  ["withdrawn", "expired", "bound"].includes(link.linkState);

const sanitizationAttestationTargets = new Set([
  "iam_review",
  "iam_migration",
  "company_release_acceptance",
  "people_provision_operation",
  "company_provisioning",
]);

function statusIcon(link: LocalLink) {
  if (link.localState || ["quarantined", "scanning"].includes(link.sourceState ?? ""))
    return <LoaderCircle aria-hidden size={14} className={styles.spin} />;
  if (link.linkState === "bound") return <LockKeyhole aria-hidden size={14} />;
  if (ready(link)) return <ShieldCheck aria-hidden size={14} />;
  if (["scan_error", "malware_rejected", "destroyed"].includes(link.sourceState ?? ""))
    return <TriangleAlert aria-hidden size={14} />;
  return <Paperclip aria-hidden size={14} />;
}

function newerSet(
  sets: AttachmentSetDto[],
  expectedTargetVersion: number,
  allowHistoricalDraft = false,
) {
  return (
    sets.find(
      (item) => item.expectedTargetVersion === expectedTargetVersion && item.state === "draft",
    ) ??
    sets.find((item) => item.expectedTargetVersion === expectedTargetVersion) ??
    (allowHistoricalDraft ? sets.find((item) => item.state === "draft") : null) ??
    null
  );
}

export function AttachmentPanel(props: (AttachmentPanelProps) & { appearance?: UiAppearance }) {
  const mode = useAttachmentFeatureMode();
  const { onAvailabilityChange, onChange, requiredCount } = props;
  const mutationsEnabled = canMutateAttachments(
    mode,
    props.readOnly !== true && props.disabled !== true && props.frozen !== true,
  );
  const previousMutationsEnabled = useRef(mutationsEnabled);
  useEffect(() => {
    if (mode === "disabled") onAvailabilityChange?.(false);
  }, [mode, onAvailabilityChange]);
  useEffect(() => {
    const wasEnabled = previousMutationsEnabled.current;
    previousMutationsEnabled.current = mutationsEnabled;
    if (!wasEnabled || mutationsEnabled) return;
    onChange?.({
      setId: null,
      links: [],
      readyCount: 0,
      requiredCount: requiredCount ?? 0,
      bindable: false,
    });
  }, [mutationsEnabled, onChange, requiredCount]);
  if (mode === "disabled") return null;
  return (
    <AttachmentPanelContent appearance={(props.appearance ?? "legacy")}
      {...props}
      onChange={mutationsEnabled ? onChange : undefined}
      readOnly={props.readOnly === true || mode === "read-only"}
    />
  );
}

function AttachmentPanelContent({ appearance = "legacy",
  base,
  targetType,
  targetId,
  expectedTargetVersion,
  componentKind,
  componentRef,
  purposeKey,
  title = "Files and evidence",
  description = "Attach images or PDF documents. Files stay private and must pass a security scan before they can be used.",
  visibility = "Access follows this record and its current authorized reviewers.",
  compact = false,
  disabled = false,
  interactionDisabled = false,
  frozen = false,
  requiredCount: requiredOverride,
  maxFiles: maxOverride,
  initialSet = null,
  fieldName,
  onChange,
  onActivityChange,
  onAvailabilityChange,
  bindMode = "action",
  onBound,
  readOnly = false,
  showHistory = false,
  allowOriginalDownload = true,
  nativeWithdrawIds = [],
  nativeBindIds = [],
}: (AttachmentPanelProps) & { appearance?: UiAppearance }) {
  const router = useRouter();
  const inputId = useId();
  const cameraId = useId();
  const panelRef = useRef<HTMLElement | null>(null);
  const mounted = useRef(true);
  const createKey = useRef(crypto.randomUUID());
  const bindKey = useRef<{ setId: string; key: string } | null>(null);
  const retryUploads = useRef(
    new Map<string, { uploadUrl: string; file: File; mediaType: string }>(),
  );
  const setRef = useRef<AttachmentSetDto | null>(initialSet);
  const [set, setSet] = useState<AttachmentSetDto | null>(initialSet);
  const [links, setLinks] = useState<LocalLink[]>(initialSet?.links ?? []);
  const [history, setHistory] = useState<AttachmentSetDto[]>([]);
  const [checking, setChecking] = useState(!initialSet);
  const [gateUnavailable, setGateUnavailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [dragging, setDragging] = useState(false);
  const [caption, setCaption] = useState("");
  const [error, setError] = useState("");
  const [withdrawId, setWithdrawId] = useState<string | null>(null);
  const [withdrawReason, setWithdrawReason] = useState("");
  const withdrawReasonRef=useRef("");
  const withdrawalScope=fieldWithdrawalScope(base,targetType,targetId,componentKind,componentRef??null,purposeKey);
  const [,setWithdrawalRevision]=useState(0);
  useEffect(()=>{const unregister=registerFieldWithdrawalOwner(withdrawalScope,()=>({setId:setRef.current?.id??null}));const unsubscribe=subscribeFieldWithdrawal(withdrawalScope,()=>setWithdrawalRevision(v=>v+1));return()=>{unsubscribe();unregister();};},[withdrawalScope,expectedTargetVersion]);
  const pendingWithdrawal=pendingFieldWithdrawal(withdrawalScope);
  useEffect(()=>{const owner={};const unregister=registerFieldBindOwner(withdrawalScope,owner);const unsubscribe=subscribeFieldBind(withdrawalScope,()=>setWithdrawalRevision(v=>v+1));return()=>{unsubscribe();unregister();};},[withdrawalScope,expectedTargetVersion]);
  const pendingBind=pendingFieldBind(withdrawalScope);
  const [boundLocally, setBoundLocally] = useState(initialSet?.state === "bound");
  const [retryableLinkIds, setRetryableLinkIds] = useState<Set<string>>(() => new Set());
  const [attestationNote, setAttestationNote] = useState("");

  const setActivity = useCallback(
    (active: boolean) => {
      busyRef.current = active;
      if (mounted.current) setBusy(active);
      // Keep the owner guard active while an upload still has an in-memory
      // File available for explicit retry, even after the active request ends.
      onActivityChange?.(active || retryUploads.current.size > 0 || !!pendingFieldWithdrawal(withdrawalScope) || !!pendingFieldBind(withdrawalScope));
    },
    [onActivityChange,withdrawalScope],
  );

  const requiredCount = Math.max(set?.requiredCount ?? 0, requiredOverride ?? 0);
  const maxFiles = Math.min(100, Math.max(1, set?.maxFiles ?? maxOverride ?? 10));
  const activeLinks = useMemo(() => links.filter(active), [links]);
  const readyCount = activeLinks.filter(ready).length;
  const sanitizationRequired = sanitizationAttestationTargets.has(targetType);
  const sanitizationAttested = !sanitizationRequired || set?.sanitizationAttested === true;
  const filesBindable = readyCount >= requiredCount && activeLinks.every(ready);
  const bindable = filesBindable && sanitizationAttested;
  const locked = frozen || boundLocally || set?.state === "bound";
  const draftMutable = !pendingWithdrawal && !pendingBind && canMutateAttachmentDraft({
    disabled,
    readOnly,
    frozen,
    locked,
  });
  const submissionBlocked =
    !!pendingWithdrawal || !!pendingBind || (draftMutable &&
    ((requiredCount > 0 && readyCount < requiredCount) || (activeLinks.length > 0 && !bindable)));

  useEffect(() => {
    const form = panelRef.current?.closest("form");
    if (!form || !submissionBlocked) return;
    const stopPendingSubmission = (event: SubmitEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      setError(
        pendingWithdrawal ? "Confirm the original file withdrawal before submitting." : pendingBind ? "Confirm the original file binding before submitting." : requiredCount > readyCount
          ? `Wait for ${requiredCount} required clean file${requiredCount === 1 ? "" : "s"} before submitting.`
          : filesBindable && !sanitizationAttested
            ? "Review the final file list and record the sanitization attestation before submitting."
            : "Wait for every selected file to finish security scanning, or withdraw it before submitting.",
      );
      panelRef.current?.focus();
    };
    form.addEventListener("submit", stopPendingSubmission, true);
    return () => form.removeEventListener("submit", stopPendingSubmission, true);
  }, [filesBindable, readyCount, requiredCount, sanitizationAttested, submissionBlocked,pendingWithdrawal,pendingBind]);

  const notify = useCallback(
    (nextSet: AttachmentSetDto | null, nextLinks: LocalLink[]) => {
      if (!onChange) return;
      const usable = nextLinks.filter(active);
      const readyLinks = usable.filter(ready);
      const minimum = Math.max(nextSet?.requiredCount ?? 0, requiredOverride ?? 0);
      onChange({
        setId: nextSet?.id ?? null,
        links: nextLinks,
        readyCount: readyLinks.length,
        requiredCount: minimum,
        bindable:
          !pendingFieldWithdrawal(withdrawalScope) && !pendingFieldBind(withdrawalScope) && readyLinks.length >= minimum &&
          usable.every(ready) &&
          (!sanitizationAttestationTargets.has(targetType) ||
            nextSet?.sanitizationAttested === true),
      });
    },
    [onChange, requiredOverride, targetType,withdrawalScope],
  );

  const notifiedPending=useRef(false);
  useEffect(()=>{const pending=!!pendingWithdrawal||!!pendingBind;if(notifiedPending.current!==pending){notifiedPending.current=pending;notify(setRef.current,links);onActivityChange?.(pending||busyRef.current||retryUploads.current.size>0);}},[pendingWithdrawal,pendingBind,notify,links,onActivityChange]);

  const replace = useCallback(
    (nextSet: AttachmentSetDto | null, nextLinks = nextSet?.links ?? []) => {
      if (!mounted.current) return;
      setRef.current = nextSet;
      setSet(nextSet);
      setLinks(nextLinks);
      setBoundLocally(nextSet?.state === "bound");
      notify(nextSet, nextLinks);
    },
    [notify],
  );

  const load = useCallback(async () => {
    try {
      const sets = await listAttachmentSets(base, {
        targetType,
        targetId,
        purposeKey,
        componentKind,
        ...(!showHistory ? { expectedTargetVersion } : {}),
      });
      const current = newerSet(sets, expectedTargetVersion, targetType === "leave_request");
      replace(current);
      setHistory(
        showHistory
          ? sets.filter(
              (item) =>
                item.id !== current?.id &&
                (item.state === "bound" ||
                  (targetType === "leave_request" && item.state === "draft")),
            )
          : [],
      );
      setGateUnavailable(false);
      onAvailabilityChange?.(true);
    } catch (cause) {
      if (
        cause instanceof AttachmentApiError &&
        cause.status === 404 &&
        cause.code === "ATTACHMENTS_DISABLED"
      ) {
        setGateUnavailable(true);
        onAvailabilityChange?.(false);
        return;
      }
      setError(cause instanceof Error ? cause.message : "Unable to load attached files.");
    } finally {
      if (mounted.current) setChecking(false);
    }
  }, [
    base,
    componentKind,
    expectedTargetVersion,
    onAvailabilityChange,
    purposeKey,
    replace,
    showHistory,
    targetId,
    targetType,
  ]);

  useEffect(() => {
    mounted.current = true;
    const timer = !initialSet ? window.setTimeout(() => void load(), 0) : null;
    return () => {
      if (timer !== null) window.clearTimeout(timer);
      mounted.current = false;
    };
  }, [initialSet, load]);

  useEffect(() => {
    createKey.current = crypto.randomUUID();
    bindKey.current = null;
  }, [componentKind, componentRef, expectedTargetVersion, purposeKey, targetId, targetType]);

  useEffect(() => {
    if (initialSet) replace(initialSet);
  }, [initialSet, replace]);

  const ensureSet = useCallback(async () => {
    if (set?.expectedTargetVersion === expectedTargetVersion) return set;
    const created = await createAttachmentSet(
      base,
      {
        targetType,
        targetId,
        componentKind,
        ...(componentRef ? { componentRef } : {}),
        purposeKey,
        expectedTargetVersion,
      },
      createKey.current,
    );
    replace(created);
    return created;
  }, [
    base,
    componentKind,
    componentRef,
    expectedTargetVersion,
    purposeKey,
    replace,
    set,
    targetId,
    targetType,
  ]);

  const patchLink = useCallback(
    (linkId: string, patch: Partial<LocalLink>) => {
      setLinks((current) => {
        const next = current.map((link) => (link.id === linkId ? { ...link, ...patch } : link));
        notify(setRef.current, next);
        return next;
      });
    },
    [notify],
  );

  const invalidateSanitizationAttestation = useCallback(() => {
    if (!sanitizationRequired || !setRef.current) return;
    const next = { ...setRef.current, sanitizationAttested: false };
    setRef.current = next;
    setSet(next);
    notify(next, links);
  }, [links, notify, sanitizationRequired]);

  const poll = useCallback(
    async (linkId: string) => {
      for (let attempt = 0; attempt < 120 && mounted.current; attempt += 1) {
        const status = await attachmentStatus(base, linkId);
        patchLink(linkId, {
          sourceId: status.sourceId,
          sourceState: status.sourceState,
          ...(pendingFieldWithdrawal(withdrawalScope)?.linkId===linkId?{}:{linkState:status.linkState}),
          previewAvailable: status.previewAvailable,
          boundAt: status.boundAt,
          localState: undefined,
          localError: status.scan.lastError ?? undefined,
        });
        if (
          terminal({
            id: linkId,
            filename: "",
            mediaType: "",
            bytes: 0,
            createdAt: 0,
            previewAvailable: status.previewAvailable,
            sourceState: status.sourceState,
            linkState: status.linkState,
          })
        )
          return;
        await new Promise((resolve) => window.setTimeout(resolve, 1000));
      }
      if (mounted.current)
        patchLink(linkId, {
          localError: "Scanning is taking longer than expected. Reload status before submitting.",
        });
    },
    [base, patchLink,withdrawalScope],
  );

  const addFiles = useCallback(
    async (selected: File[]) => {
      if (!draftMutable || interactionDisabled || busyRef.current || !selected.length) return;
      setError("");
      const remaining = maxFiles - activeLinks.length;
      if (selected.length > remaining) {
        setError(`You can attach ${remaining} more file${remaining === 1 ? "" : "s"} to this set.`);
        return;
      }
      const invalid = selected
        .map((file) => ({ file, error: validateGenericAttachment(file) }))
        .find((item) => item.error);
      if (invalid) {
        setError(`${invalid.file.name}: ${invalid.error}`);
        return;
      }
      setActivity(true);
      try {
        const destination = await ensureSet();
        for (const file of selected) {
          const mediaType = attachmentMediaType(file);
          const optimisticId = `reserving:${crypto.randomUUID()}`;
          const optimistic: LocalLink = {
            id: optimisticId,
            filename: file.name,
            mediaType,
            bytes: file.size,
            sourceState: "quarantined",
            linkState: "draft",
            caption: caption.trim() || null,
            previewAvailable: false,
            createdAt: Date.now(),
            localState: "reserving",
          };
          setLinks((current) => [...current, optimistic]);
          let currentLinkId = optimisticId;
          try {
            const reservation = await reserveAttachmentFile(
              base,
              destination.id,
              {
                filename: file.name,
                mediaType,
                bytes: file.size,
                ...(caption.trim() ? { caption: caption.trim() } : {}),
              },
              crypto.randomUUID(),
            );
            invalidateSanitizationAttestation();
            const pending: LocalLink = {
              ...optimistic,
              id: reservation.attachmentLinkId,
              localState: "uploading",
            };
            currentLinkId = reservation.attachmentLinkId;
            retryUploads.current.set(reservation.attachmentLinkId, {
              uploadUrl: reservation.uploadUrl,
              file,
              mediaType,
            });
            setRetryableLinkIds((current) => new Set(current).add(reservation.attachmentLinkId));
            setLinks((current) =>
              current.map((item) => (item.id === optimisticId ? pending : item)),
            );
            await uploadAttachmentBytes(reservation.uploadUrl, file, mediaType);
            retryUploads.current.delete(reservation.attachmentLinkId);
            setRetryableLinkIds((current) => {
              const next = new Set(current);
              next.delete(reservation.attachmentLinkId);
              return next;
            });
            patchLink(reservation.attachmentLinkId, { localState: "waiting" });
            void poll(reservation.attachmentLinkId).catch((cause) =>
              patchLink(reservation.attachmentLinkId, {
                localState: undefined,
                localError:
                  cause instanceof Error ? cause.message : "Unable to refresh scan status.",
              }),
            );
          } catch (cause) {
            const message =
              cause instanceof Error
                ? cause.message
                : "Upload failed. Select this file again to retry.";
            setLinks((current) =>
              current.map((item) =>
                item.id === currentLinkId
                  ? { ...item, localState: undefined, localError: message }
                  : item,
              ),
            );
            setError(`${file.name}: ${message}`);
          }
        }
        setCaption("");
      } catch (cause) {
        if (
          cause instanceof AttachmentApiError &&
          cause.status === 404 &&
          cause.code === "ATTACHMENTS_DISABLED"
        )
          setGateUnavailable(true);
        else setError(cause instanceof Error ? cause.message : "Unable to start secure upload.");
      } finally {
        setActivity(false);
      }
    },
    [
      activeLinks.length,
      base,
      busy,
      caption,
      draftMutable,
      ensureSet,
      invalidateSanitizationAttestation,
      interactionDisabled,
      maxFiles,
      patchLink,
      poll,
      setActivity,
    ],
  );

  async function retryUpload(linkId: string) {
    const pending = retryUploads.current.get(linkId);
    if (!pending || !draftMutable || interactionDisabled || busyRef.current) return;
    setActivity(true);
    setError("");
    patchLink(linkId, { localState: "uploading", localError: undefined });
    try {
      await uploadAttachmentBytes(pending.uploadUrl, pending.file, pending.mediaType);
      retryUploads.current.delete(linkId);
      setRetryableLinkIds((current) => {
        const next = new Set(current);
        next.delete(linkId);
        return next;
      });
      patchLink(linkId, { localState: "waiting", localError: undefined });
      void poll(linkId).catch((cause) =>
        patchLink(linkId, {
          localState: undefined,
          localError: cause instanceof Error ? cause.message : "Unable to refresh scan status.",
        }),
      );
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : "Upload retry failed.";
      patchLink(linkId, { localState: undefined, localError: message });
      setError(message);
    } finally {
      setActivity(false);
    }
  }

  async function applyWithdrawal(status: Awaited<ReturnType<typeof withdrawAttachmentLink>>,originalReason:string) {
    if(!mounted.current||setRef.current?.id!==status.setId)return;
    patchLink(status.linkId,{sourceState:status.sourceState,linkState:status.linkState,localState:undefined});
    invalidateSanitizationAttestation();retryUploads.current.delete(status.linkId);
    setRetryableLinkIds(current=>{const next=new Set(current);next.delete(status.linkId);return next;});
    if(withdrawReasonRef.current===originalReason){withdrawReasonRef.current="";setWithdrawReason("");setWithdrawId(current=>current===status.linkId?null:current);}
  }
  async function withdraw() {
    if (!draftMutable || interactionDisabled || busyRef.current || !withdrawId || withdrawReason.trim().length < 3) return;
    const id=withdrawId,draftReason=withdrawReason,reason=draftReason.trim(),selected=nativeWithdrawIds.includes(id);
    setActivity(true);setError("");
    try {
      const status=selected?await submitFieldWithdrawal({scope:withdrawalScope,base,linkId:id,setId:setRef.current?.id??'',reason,draftReason}):await withdrawAttachmentLink(base,id,reason,crypto.randomUUID());
      await applyWithdrawal(status,draftReason);
    } catch(cause){if(mounted.current)setError(cause instanceof Error?cause.message:"Unable to withdraw this file.");}
    finally{setActivity(false);}
  }
  async function retryWithdrawal() {
    const pending=pendingFieldWithdrawal(withdrawalScope);if(!pending||interactionDisabled||busyRef.current||!nativeWithdrawIds.includes(pending.linkId))return;
    setActivity(true);setError("");try{const status=await retryFieldWithdrawal(withdrawalScope);await applyWithdrawal(status,pending.draftReason);}catch(cause){if(mounted.current)setError(cause instanceof Error?cause.message:"Unable to confirm the original withdrawal.");}finally{setActivity(false);}
  }

  async function attestSanitization() {
    if (
      !draftMutable ||
      interactionDisabled ||
      !set ||
      !sanitizationRequired ||
      !filesBindable ||
      attestationNote.trim().length < 10 ||
      busyRef.current
    )
      return;
    setActivity(true);
    setError("");
    try {
      const reviewed = await attestAttachmentSetSanitized(
        base,
        set.id,
        { confirmed: true, note: attestationNote.trim() },
        crypto.randomUUID(),
      );
      replace(reviewed);
      setAttestationNote("");
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Unable to record the sanitization review.",
      );
    } finally {
      setActivity(false);
    }
  }

  async function applyBind(result: Awaited<ReturnType<typeof bindRecordAttachmentSet>>) {
    if (!mounted.current || setRef.current?.id !== result.setId) return;
    const boundIds=new Set(result.manifest.map(item=>item.linkId));
    setLinks(current=>{const next=current.map(link=>boundIds.has(link.id)?{...link,linkState:"bound" as const,boundAt:Date.now()}:link);const currentSet=setRef.current;if(currentSet?.id===result.setId){const updated={...currentSet,state:"bound",links:next};setRef.current=updated;setSet(updated);notify(updated,next);}return next;});
    setBoundLocally(true);onBound?.({setId:result.setId,version:result.version});router.refresh();
  }
  async function bindRecord() {
    if (!draftMutable || interactionDisabled || busyRef.current || bindMode !== "record" || !set || readyCount < 1 || !bindable) return;
    setActivity(true);setError("");
    try {
      if (bindKey.current?.setId !== set.id) bindKey.current={setId:set.id,key:crypto.randomUUID()};
      const result=nativeBindIds.includes(set.id)?await submitFieldBind({scope:withdrawalScope,base,setId:set.id,expectedVersion:expectedTargetVersion}):await bindRecordAttachmentSet(base,set.id,expectedTargetVersion,bindKey.current.key);
      await applyBind(result);
    } catch(cause){setError(cause instanceof Error?cause.message:"Unable to lock these files to the record.");}finally{setActivity(false);}
  }
  async function retryBind(){const pending=pendingFieldBind(withdrawalScope);if(!pending||interactionDisabled||busyRef.current||!nativeBindIds.includes(pending.setId))return;setActivity(true);setError("");try{const result=await retryFieldBind(withdrawalScope);await applyBind(result);}catch(cause){if(mounted.current)setError(cause instanceof Error?cause.message:"Unable to confirm the original file binding.");}finally{setActivity(false);}}

  if (checking || gateUnavailable) return null;

  return (
    <section data-ui-appearance={appearance}
      ref={panelRef}
      tabIndex={-1}
      className={`${styles.panel} ${compact ? styles.compact : ""}`}
      aria-labelledby={`${inputId}-title`}
    >
      <div data-ui-appearance={appearance} className={styles.heading}>
        <div data-ui-appearance={appearance}>
          <h3 data-ui-appearance={appearance} id={`${inputId}-title`}>{title}</h3>
          <p data-ui-appearance={appearance}>{description}</p>
          <p data-ui-appearance={appearance} className={styles.visibility}>
            <LockKeyhole aria-hidden size={13} />
            {visibility}
          </p>
        </div>
        <span data-ui-appearance={appearance} className={styles.counter}>
          {!set && history.length > 0
            ? `${history.reduce((count, item) => count + item.links.filter(active).length, 0)} attached`
            : `${readyCount}/${maxFiles} ready${requiredCount ? ` · ${requiredCount} required` : ""}`}
        </span>
      </div>

      {draftMutable && fieldName && set?.id && (
        <TextInput appearance={appearance} type="hidden" name={fieldName} value={set.id} />
      )}

      {draftMutable && (
        <>
          <Field appearance={appearance} className={styles.caption}>
            Caption for the next files (optional)
            <TextInput appearance={appearance}
              value={caption}
              maxLength={500}
              disabled={disabled || interactionDisabled || busy}
              onChange={(event) => setCaption(event.target.value)}
              placeholder="What this file proves or contains"
            />
          </Field>
          <div data-ui-appearance={appearance}
            className={styles.dropzone}
            data-dragging={dragging}
            onDragEnter={(event) => {
              event.preventDefault();
              if (!disabled && !interactionDisabled && !readOnly && !busy) setDragging(true);
            }}
            onDragOver={(event) => event.preventDefault()}
            onDragLeave={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node | null))
                setDragging(false);
            }}
            onDrop={(event) => {
              event.preventDefault();
              setDragging(false);
              if (!interactionDisabled) void addFiles(Array.from(event.dataTransfer.files));
            }}
          >
            <Upload aria-hidden size={24} />
            <strong data-ui-appearance={appearance}>Drop files here or choose from this device</strong>
            <span data-ui-appearance={appearance}>JPEG, PNG, WebP, HEIC, HEIF, or PDF · up to 10 MB each</span>
            <div data-ui-appearance={appearance} className={styles.pickerActions}>
              <Field appearance={appearance} className={styles.picker} htmlFor={inputId}>
                <Paperclip aria-hidden size={14} /> Choose files
                <TextInput appearance={appearance}
                  id={inputId}
                  type="file"
                  multiple
                  accept="image/jpeg,image/png,image/webp,image/heic,image/heif,.heic,.heif,application/pdf"
                  disabled={disabled || interactionDisabled || busy || activeLinks.length >= maxFiles}
                  onChange={(event) => {
                    void addFiles(Array.from(event.target.files ?? []));
                    event.currentTarget.value = "";
                  }}
                />
              </Field>
              <Field appearance={appearance} className={styles.picker} htmlFor={cameraId}>
                <Camera aria-hidden size={14} /> Take photo
                <TextInput appearance={appearance}
                  id={cameraId}
                  type="file"
                  accept="image/*"
                  capture="environment"
                  disabled={disabled || interactionDisabled || busy || activeLinks.length >= maxFiles}
                  onChange={(event) => {
                    void addFiles(Array.from(event.target.files ?? []));
                    event.currentTarget.value = "";
                  }}
                />
              </Field>
            </div>
          </div>
        </>
      )}

      {locked && !pendingBind && (
        <p data-ui-appearance={appearance} className={styles.notice}>
          <LockKeyhole aria-hidden size={15} />
          This evidence is bound to a recorded action. Add a correction or a new revision instead of
          replacing it.
        </p>
      )}

      {sanitizationRequired && draftMutable && (
        <div data-ui-appearance={appearance} className={styles.notice}>
          <ShieldCheck aria-hidden size={15} />
          <div data-ui-appearance={appearance}>
            <strong data-ui-appearance={appearance}>Sanitized review evidence only</strong>
            <p data-ui-appearance={appearance}>
              Never upload passwords, private keys, secrets, configuration exports, or database
              dumps. Review the final clean file list and record what was sanitized before binding.
            </p>
            {!sanitizationAttested && (
              <>
                <Field appearance={appearance} className={styles.caption}>
                  Sanitization review note
                  <TextArea appearance={appearance}
                    value={attestationNote}
                    minLength={10}
                    maxLength={1000}
                    disabled={!filesBindable || interactionDisabled || busy}
                    onChange={(event) => setAttestationNote(event.target.value)}
                    placeholder="Reviewed the artifact; credentials and sensitive system material were removed."
                  />
                </Field>
                <Button appearance={appearance}
                  variant="secondary"
                  type="button"

                  disabled={!filesBindable || attestationNote.trim().length < 10 || interactionDisabled || busy}
                  onClick={() => void attestSanitization()}
                >
                  Record sanitization review
                </Button>
              </>
            )}
            {sanitizationAttested && (
              <p data-ui-appearance={appearance}>
                <LockKeyhole aria-hidden size={13} /> Sanitization review recorded. Adding or
                withdrawing a file requires a new review.
              </p>
            )}
          </div>
        </div>
      )}

      {pendingBind && <InlineNotice tone="warning">The original file binding awaits confirmation. Your newer entries are retained. <Button appearance={appearance} type="button" disabled={interactionDisabled||busy||!nativeBindIds.includes(pendingBind.setId)} onClick={()=>void retryBind()}>Retry original file binding</Button></InlineNotice>}
      {pendingWithdrawal && <InlineNotice tone="warning">The original file withdrawal awaits confirmation. Your newer entries are retained. <Button appearance={appearance} type="button" disabled={interactionDisabled||busy||!nativeWithdrawIds.includes(pendingWithdrawal.linkId)} onClick={()=>void retryWithdrawal()}>Retry original withdrawal</Button></InlineNotice>}
      {error && (
        <p data-ui-appearance={appearance} className={styles.error} role="alert">
          {error}
        </p>
      )}

      {links.length > 0 && (
        <ul data-ui-appearance={appearance} className={styles.files} aria-label={`${title} file list`}>
          {links.map((link) => {
            const isError =
              Boolean(link.localError) ||
              ["scan_error", "malware_rejected", "destroyed"].includes(link.sourceState ?? "");
            const canWithdraw =
              draftMutable && link.linkState === "draft" && !link.id.startsWith("reserving:");
            return (
              <li data-ui-appearance={appearance} className={styles.file} key={link.id}>
                {link.previewAvailable && active(link) ? (
                  <a data-ui-appearance={appearance}
                    className={styles.filePreview}
                    href={attachmentContentUrl(base, link.id, "preview")}
                    target="_blank"
                    rel="noreferrer"
                    aria-label={`Open sanitized preview of ${link.filename}`}
                  >
                    <Image
                      src={attachmentContentUrl(base, link.id, "preview")}
                      alt={`Sanitized preview of ${link.filename}`}
                      width={52}
                      height={52}
                      unoptimized
                    />
                  </a>
                ) : (
                  <span data-ui-appearance={appearance} className={styles.fileIcon}>
                    {link.mediaType.startsWith("image/") ? (
                      <ImageIcon aria-hidden size={16} />
                    ) : (
                      <FileText aria-hidden size={16} />
                    )}
                  </span>
                )}
                <span data-ui-appearance={appearance} className={styles.fileInfo}>
                  <strong data-ui-appearance={appearance} title={link.filename}>{link.filename}</strong>
                  <span data-ui-appearance={appearance}>
                    {formatAttachmentBytes(link.bytes ?? 0)}
                    {link.caption ? ` · ${link.caption}` : ""}
                  </span>
                  <span data-ui-appearance={appearance}
                    className={styles.state}
                    data-ready={ready(link)}
                    data-error={isError}
                    role="status"
                  >
                    {pendingBind && set?.id===pendingBind.setId ? "Binding confirmation pending" : pendingWithdrawal?.linkId===link.id ? "Withdrawal confirmation pending" : <>{statusIcon(link)} {attachmentStateLabel(link)}</>}
                  </span>
                  {link.localError && <span data-ui-appearance={appearance} role="alert">{link.localError}</span>}
                </span>
                <span data-ui-appearance={appearance} className={styles.fileActions}>
                  {link.previewAvailable && active(link) && (
                    <UiActionAnchor appearance={appearance}
                      variant="ghost"
                      className={styles.iconAction}
                      href={attachmentContentUrl(base, link.id, "preview")}
                      target="_blank"
                      rel="noreferrer"
                      aria-label={`Preview ${link.filename}`}
                      title="Open sanitized preview"
                    >
                      <Eye aria-hidden size={15} />
                    </UiActionAnchor>
                  )}
                  {allowOriginalDownload && ready(link) && (
                    <UiActionAnchor appearance={appearance}
                      variant="ghost"
                      className={styles.iconAction}
                      href={attachmentContentUrl(base, link.id, "original")}
                      aria-label={`Download ${link.filename}`}
                      title="Download private original"
                    >
                      <Download aria-hidden size={15} />
                    </UiActionAnchor>
                  )}
                  {!terminal(link) && !link.localState && (
                    <Button appearance={appearance}
                      variant="ghost"
                      type="button"
                      className={styles.iconAction}
                      aria-label={`Reload status for ${link.filename}`}
                      title="Reload security status"
                      onClick={() => void poll(link.id)}
                    >
                      <RefreshCw aria-hidden size={15} />
                    </Button>
                  )}
                  {draftMutable && link.localError && retryableLinkIds.has(link.id) && (
                    <Button appearance={appearance}
                      variant="ghost"
                      type="button"
                      className={styles.iconAction}
                      aria-label={`Retry upload for ${link.filename}`}
                      title="Retry interrupted upload"
                      disabled={interactionDisabled || busy}
                      onClick={() => void retryUpload(link.id)}
                    >
                      <Upload aria-hidden size={15} />
                    </Button>
                  )}
                  {canWithdraw && (
                    <Button appearance={appearance}
                      variant="ghost"
                      type="button"
                      className={styles.iconAction}
                      aria-label={`Withdraw ${link.filename}`}
                      title="Withdraw before this evidence is locked"
                      disabled={interactionDisabled || busy}
                      onClick={() => {
                        setWithdrawId(link.id);
                        withdrawReasonRef.current="";setWithdrawReason("");
                      }}
                    >
                      <Trash2 aria-hidden size={15} />
                    </Button>
                  )}
                </span>
                {canWithdraw && withdrawId === link.id && (
                  <div data-ui-appearance={appearance} className={styles.withdrawForm}>
                    <Field appearance={appearance}>
                      Reason for withdrawing {link.filename}
                      <TextArea appearance={appearance}
                        value={withdrawReason}
                        minLength={3}
                        maxLength={1000}
                        required
                        autoFocus
                        disabled={interactionDisabled}
                        onChange={(event) => {withdrawReasonRef.current=event.target.value;setWithdrawReason(event.target.value);}}
                      />
                    </Field>
                    <div data-ui-appearance={appearance} className={styles.withdrawActions}>
                      <Button appearance={appearance}
                        variant="secondary"
                        type="button"

                        onClick={() => setWithdrawId(null)}
                      >
                        Keep file
                      </Button>
                      <Button appearance={appearance}
                        variant="primary"
                        type="button"

                        disabled={interactionDisabled || busy || withdrawReason.trim().length < 3}
                        onClick={() => void withdraw()}
                      >
                        Withdraw file
                      </Button>
                    </div>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      {history.length > 0 && (
        <div data-ui-appearance={appearance} className={styles.history}>
          <h4 data-ui-appearance={appearance}>Attached history</h4>
          {history.map((historicalSet) => (
            <section data-ui-appearance={appearance} className={styles.historySet} key={historicalSet.id}>
              <p data-ui-appearance={appearance}>
                Record version {historicalSet.expectedTargetVersion ?? "—"}
                {historicalSet.links[0]?.boundAt ? (
                  <>
                    {" "}
                    · attached <TimeText appearance={appearance} value={historicalSet.links[0].boundAt} />
                  </>
                ) : historicalSet.state === "draft" ? (
                  " · staged for approval"
                ) : (
                  ""
                )}
              </p>
              <ul data-ui-appearance={appearance}
                className={styles.files}
                aria-label={`${title} record version ${historicalSet.expectedTargetVersion ?? "history"}`}
              >
                {historicalSet.links.filter(active).map((link) => (
                  <li data-ui-appearance={appearance} className={styles.file} key={link.id}>
                    {link.previewAvailable ? (
                      <a data-ui-appearance={appearance}
                        className={styles.filePreview}
                        href={attachmentContentUrl(base, link.id, "preview")}
                        target="_blank"
                        rel="noreferrer"
                        aria-label={`Open sanitized preview of ${link.filename}`}
                      >
                        <Image
                          src={attachmentContentUrl(base, link.id, "preview")}
                          alt={`Sanitized preview of ${link.filename}`}
                          width={52}
                          height={52}
                          unoptimized
                        />
                      </a>
                    ) : (
                      <span data-ui-appearance={appearance} className={styles.fileIcon}>
                        {link.mediaType.startsWith("image/") ? (
                          <ImageIcon aria-hidden size={16} />
                        ) : (
                          <FileText aria-hidden size={16} />
                        )}
                      </span>
                    )}
                    <span data-ui-appearance={appearance} className={styles.fileInfo}>
                      <strong data-ui-appearance={appearance} title={link.filename}>{link.filename}</strong>
                      <span data-ui-appearance={appearance}>
                        {formatAttachmentBytes(link.bytes ?? 0)}
                        {link.caption ? ` · ${link.caption}` : ""}
                      </span>
                      <span data-ui-appearance={appearance} className={styles.state} data-ready={ready(link)}>
                        {historicalSet.state === "bound" ? (
                          <LockKeyhole aria-hidden size={13} />
                        ) : (
                          <ShieldCheck aria-hidden size={13} />
                        )}
                        {historicalSet.state === "bound"
                          ? " Attached and locked"
                          : " Clean and staged for approval"}
                      </span>
                    </span>
                    <span data-ui-appearance={appearance} className={styles.fileActions}>
                      {link.previewAvailable && (
                        <UiActionAnchor appearance={appearance}
                          variant="ghost"
                          className={styles.iconAction}
                          href={attachmentContentUrl(base, link.id, "preview")}
                          target="_blank"
                          rel="noreferrer"
                          aria-label={`Preview ${link.filename}`}
                          title="Open sanitized preview"
                        >
                          <Eye aria-hidden size={15} />
                        </UiActionAnchor>
                      )}
                      {allowOriginalDownload && link.sourceState === "available" && (
                        <UiActionAnchor appearance={appearance}
                          variant="ghost"
                          className={styles.iconAction}
                          href={attachmentContentUrl(base, link.id, "original")}
                          aria-label={`Download ${link.filename}`}
                          title="Download private original"
                        >
                          <Download aria-hidden size={15} />
                        </UiActionAnchor>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}

      {draftMutable && requiredCount > 0 && !bindable && (
        <p data-ui-appearance={appearance} className={styles.notice} role="status">
          <TriangleAlert aria-hidden size={15} />
          This action needs {requiredCount} clean file
          {requiredCount === 1 ? "" : "s"}. Wait for every selected file to finish scanning before
          submitting.
        </p>
      )}
      {draftMutable &&
        activeLinks.length > 0 &&
        activeLinks.some((link) => !ready(link)) &&
        requiredCount === 0 && (
          <p data-ui-appearance={appearance} className={styles.hint}>
            Wait for every selected file to finish scanning, or withdraw it, before submitting this
            action.
          </p>
        )}
      {draftMutable && bindMode === "record" && set && readyCount > 0 && (
        <div data-ui-appearance={appearance} className={styles.bindActions}>
          <Button appearance={appearance}
            variant="primary"
            type="button"
            className="primary"
            disabled={interactionDisabled || busy || !bindable}
            onClick={() => void bindRecord()}
          >
            <LockKeyhole aria-hidden size={15} />
            Attach clean files to this record
          </Button>
          {!bindable && <span data-ui-appearance={appearance}>Wait for every selected file to pass its security scan.</span>}
        </div>
      )}
    </section>
  );
}
