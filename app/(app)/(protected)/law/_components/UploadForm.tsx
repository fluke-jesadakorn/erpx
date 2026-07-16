'use client';

import { useRef, useState } from 'react';
import { useRouter } from 'next/navigation';

export function UploadForm() {
  const router = useRouter();
  const input = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<File | null>(null);
  const [drag, setDrag] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function upload() {
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      const form = new FormData();
      form.set('file', file);
      const res = await fetch('/api/law/upload', { method: 'POST', body: form });
      const body = await res.json();
      if (!res.ok) throw new Error(body.error || 'Upload failed');
      router.push(`/law/${body.contractId}`);
      router.refresh();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="rounded-2xl border border-slate-800 bg-slate-950/55 p-5">
      <input
        ref={input}
        type="file"
        accept="application/pdf,text/plain,.pdf,.txt,.docx"
        className="hidden"
        onChange={(event) => setFile(event.target.files?.[0] ?? null)}
      />
      <button
        type="button"
        onClick={() => input.current?.click()}
        onDragOver={(event) => {
          event.preventDefault();
          setDrag(true);
        }}
        onDragLeave={() => setDrag(false)}
        onDrop={(event) => {
          event.preventDefault();
          setDrag(false);
          setFile(event.dataTransfer.files?.[0] ?? null);
        }}
        className={`flex min-h-64 w-full flex-col items-center justify-center rounded-2xl border-2 border-dashed px-6 text-center transition-colors ${
          drag
            ? 'border-cyan-400 bg-cyan-500/10'
            : 'border-slate-700 bg-slate-900/35 hover:border-cyan-500/60 hover:bg-slate-900/60'
        }`}
      >
        <span className="text-lg font-semibold text-slate-100">
          {file ? file.name : 'Drop a contract here'}
        </span>
        <span className="mt-2 text-sm text-slate-500">
          {file
            ? `${(file.size / 1024 / 1024).toFixed(2)} MB · ${file.type || 'unknown type'}`
            : 'or click to browse PDF, DOCX, or text files'}
        </span>
      </button>

      {error && <p className="mt-3 text-sm text-rose-300">{error}</p>}

      <div className="mt-4 flex items-center justify-between gap-3">
        <p className="text-xs text-slate-500">Files are stored under law/contracts/ and indexed asynchronously.</p>
        <button
          type="button"
          onClick={upload}
          disabled={!file || busy}
          className="rounded-xl border border-cyan-500/50 bg-cyan-500/15 px-4 py-2 text-sm font-medium text-cyan-100 hover:bg-cyan-500/25 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {busy ? 'Uploading' : 'Upload contract'}
        </button>
      </div>
    </section>
  );
}
