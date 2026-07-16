'use client';

import { useEffect, useRef, useState } from 'react';
import type { VisionModel } from '@folio-lib/ai/loadVisionModels';
import { discardSlip, getSlipLockState } from '@/app/actions/slips';
import type {
  ExtractionState,
  ParsedFields,
  Phase,
  SlipOcrOpts,
  UploadOk,
  UploadValidation,
} from './types';

const MODEL_STORAGE_KEY = 'slip.ocrModel';

export function useSlipOcr(opts: SlipOcrOpts) {
  const {
    kind,
    initialModels = [],
    currentUserId,
    onSlipReady,
    onSlipDiscarded,
  } = opts;

  const xhrRef = useRef<XMLHttpRequest | null>(null);
  const extractStartRef = useRef<number | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const [visionModels] = useState<VisionModel[]>(initialModels);
  const [selectedModel, setSelectedModel] = useState<string>('');
  const [phase, setPhase] = useState<Phase>('idle');
  const [extractionState, setExtractionState] = useState<ExtractionState>('pending');
  const [elapsed, setElapsed] = useState(0);
  const [slipId, setSlipId] = useState<number | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const [pendingFile, setPendingFile] = useState<File | null>(null);
  const [confidence, setConfidence] = useState<number>(0);
  const [mode, setMode] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const [validation, setValidation] = useState<UploadValidation | undefined>(undefined);
  const [parsed, setParsed] = useState<ParsedFields | null>(null);
  const [locked, setLocked] = useState(false);
  const [lockReason, setLockReason] = useState<string | null>(null);
  const [zoomOpen, setZoomOpen] = useState(false);

  useEffect(() => {
    if (!selectedModel && visionModels.length > 0) {
      const saved = typeof window !== 'undefined' ? window.localStorage.getItem(MODEL_STORAGE_KEY) : null;
      if (saved && visionModels.some((m) => m.name === saved)) {
        setSelectedModel(saved);
      } else {
        setSelectedModel(visionModels[0].name);
      }
    }
  }, [visionModels, selectedModel]);

  useEffect(() => {
    return () => {
      if (preview) URL.revokeObjectURL(preview);
    };
  }, [preview]);

  useEffect(() => {
    if (extractionState !== 'running') {
      extractStartRef.current = null;
      setElapsed(0);
      return;
    }
    extractStartRef.current = Date.now();
    setElapsed(0);
    const id = setInterval(() => {
      if (extractStartRef.current != null) {
        setElapsed(Math.floor((Date.now() - extractStartRef.current) / 1000));
      }
    }, 500);
    return () => clearInterval(id);
  }, [extractionState]);

  function revokePreview() {
    if (preview) {
      URL.revokeObjectURL(preview);
      setPreview(null);
    }
  }

  function resetOcrState() {
    setSlipId(null);
    setConfidence(0);
    setMode('');
    setParsed(null);
    setLocked(false);
    setLockReason(null);
    setError(null);
    setValidation(undefined);
  }

  async function uploadFile(file: File) {
    setError(null);
    setFileName(file.name);
    setPhase('extracting');
    setExtractionState('running');
    try {
      const fd = new FormData();
      fd.append('file', file, file.name);
      if (selectedModel) fd.append('model_name', selectedModel);
      fd.append('kind', kind);
      const xhr = new XMLHttpRequest();
      xhrRef.current = xhr;
      const result: UploadOk = await new Promise((resolve, reject) => {
        xhr.open('POST', '/api/upload');
        xhr.onload = () => {
          if (xhr.status >= 200 && xhr.status < 300) {
            try {
              resolve(JSON.parse(xhr.responseText));
            } catch {
              reject(new Error('Invalid JSON response'));
            }
          } else {
            try {
              const j = JSON.parse(xhr.responseText);
              const detail = typeof j?.detail === 'string' ? j.detail : null;
              const upstream = typeof j?.upstreamMessage === 'string' ? j.upstreamMessage : null;
              const msg =
                (detail && upstream ? `${detail} — ${upstream}` : detail) ||
                j?.error ||
                `HTTP ${xhr.status}`;
              reject(new Error(msg));
            } catch {
              reject(new Error(`HTTP ${xhr.status}`));
            }
          }
        };
        xhr.onerror = () => reject(new Error('Network error'));
        xhr.onabort = () => reject(new Error('Aborted'));
        xhr.send(fd);
      });

      setSlipId(result.slipId);
      setConfidence(result.confidence ?? 0);
      setMode(result.mode ?? '');
      setValidation(result.validation ?? undefined);
      setParsed(result.parsed ?? null);

      if (result.status === 'confirmed') {
        const lock = await getSlipLockState({
          slipId: result.slipId,
          actorId: currentUserId ?? 0,
        });
        setLocked(lock.locked);
        setLockReason(lock.reason);
        setPhase('confirmed');
      } else {
        setExtractionState('done');
        onSlipReady?.(result.slipId, kind);
      }
      xhrRef.current = null;
    } catch (err: unknown) {
      xhrRef.current = null;
      if (err instanceof Error && err.message === 'Aborted') {
        revokePreview();
        setPendingFile(null);
        setFileName(null);
        setError(null);
        setExtractionState('pending');
        setPhase('idle');
        return;
      }
      setError(err instanceof Error ? err.message : String(err));
      setExtractionState('done');
    }
  }

  function pickModel(name: string) {
    if (name === selectedModel) return;
    setSelectedModel(name);
    try {
      window.localStorage.setItem(MODEL_STORAGE_KEY, name);
    } catch {
      /* ignore */
    }
    setValidation(undefined);
    if (extractionState === 'done' && pendingFile && visionModels.length > 0 && phase !== 'confirming') {
      setError(null);
      setExtractionState('running');
      void uploadFile(pendingFile);
    }
  }

  function extract() {
    if (!pendingFile) return;
    if (extractionState === 'running') return;
    if (phase === 'confirming' || phase === 'confirmed') return;
    setError(null);
    setPhase('extracting');
    setExtractionState('running');
    void uploadFile(pendingFile);
  }

  function selectFile(file: File) {
    setError(null);
    resetOcrState();
    revokePreview();
    if (file.type.startsWith('image/')) {
      setPreview(URL.createObjectURL(file));
    }
    setFileName(file.name);
    setPendingFile(file);
    setPhase('extracting');
    setExtractionState('pending');
  }

  function onPick(e: React.ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0];
    if (f) selectFile(f);
    e.target.value = '';
  }

  function onDrop(e: React.DragEvent<HTMLDivElement>) {
    e.preventDefault();
    const f = e.dataTransfer.files?.[0];
    if (f) selectFile(f);
  }

  async function removeFile() {
    if (slipId == null) return;
    const removedId = slipId;
    setError(null);
    const r = await discardSlip({ slipId, actorId: currentUserId ?? 0 });
    if (!r.success) {
      setError(r.error ?? 'Remove failed');
      if (r.error?.toLowerCase().includes('locked')) {
        setLocked(true);
        setLockReason(r.error);
      }
      return;
    }
    onSlipDiscarded?.(removedId, kind);
    resetOcrState();
    revokePreview();
    setPendingFile(null);
    setFileName(null);
    setExtractionState('pending');
    setPhase('idle');
  }

  function pickAnother() {
    resetOcrState();
    revokePreview();
    setPendingFile(null);
    setFileName(null);
    setExtractionState('pending');
    setPhase('idle');
  }

  const selectedModelDesc =
    visionModels.find((m) => m.name === selectedModel)?.description ?? null;

  return {
    inputRef,
    visionModels,
    selectedModel,
    pickModel,
    selectedModelDesc,
    pendingFile,
    fileName,
    preview,
    zoomOpen,
    setZoomOpen,
    phase,
    extractionState,
    elapsed,
    slipId,
    confidence,
    mode,
    error,
    validation,
    parsed,
    locked,
    lockReason,
    selectFile,
    extract,
    removeFile,
    pickAnother,
    onPick,
    onDrop,
    setError,
    setPhase,
    setExtractionState,
    setLocked,
    setLockReason,
  };
}

export type SlipOcrApi = ReturnType<typeof useSlipOcr>;