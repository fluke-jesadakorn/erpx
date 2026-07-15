'use client';

import React, { useState } from 'react';
import { Modal } from '@/components/ui/Modal';
import { useToast } from '@/components/ui/Toast';
import type { TileDef } from './tile-config';

interface Props {
  open: boolean;
  onClose: () => void;
  tile: TileDef | null;
  actorId: number;
  targetLabel: string;
}

export const RequestAccessModal: React.FC<Props> = ({ open, onClose, tile, targetLabel }) => {
  const toast = useToast();
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);

  if (!tile) return null;

  const submit = async () => {
    setBusy(true);
    try {
      const res = await fetch('/api/access-requests', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          tileId: tile.id,
          tileTitle: tile.display_name,
          target: tile.requestAccessTarget ?? 'hr_manager',
          note,
        }),
      }).then((r) => r.json());
      if (res.error) {
        toast.error(res.error || 'Request failed');
        return;
      }
      toast.success(`Request sent to ${targetLabel}.`, 'Request Access');
      setNote('');
      onClose();
    } catch (e: any) {
      toast.error(e?.message || 'Request failed');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Request Access"
      subtitle={`Ask ${targetLabel} to enable this feature for you.`}
      tone="cyan"
      width="md"
      footer={
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="px-3 py-1.5 rounded-lg text-sm font-mono font-bold uppercase tracking-wider text-slate-300 hover:text-white border border-slate-700 hover:border-slate-600"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={submit}
            disabled={busy}
            className="px-3 py-1.5 rounded-lg text-sm font-mono font-bold uppercase tracking-wider text-cyan-100 bg-cyan-500/20 border border-cyan-500/50 hover:bg-cyan-500/30 disabled:opacity-50"
          >
            {busy ? 'Sending…' : 'Send Request'}
          </button>
        </div>
      }
    >
      <div className="space-y-3">
        <div className="rounded-xl border border-slate-800 bg-slate-950/60 px-3 py-2 text-sm font-mono text-slate-300 flex items-center gap-2">
          <span className="text-base">{tile.icon}</span>
          <div className="min-w-0">
            <div className="text-white truncate">{tile.display_name}</div>
            <div className="text-slate-500 truncate">{tile.subtitle}</div>
          </div>
        </div>
        <label className="block">
          <span className="text-xs font-mono uppercase tracking-wider text-slate-400">Note (optional)</span>
          <textarea
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="Why do you need access?"
            rows={4}
            className="mt-1 w-full rounded-xl bg-slate-950/70 border border-slate-800 px-3 py-2 text-xs text-slate-100 placeholder:text-slate-600 focus:outline-none focus:border-cyan-500/60"
          />
        </label>
        <p className="text-xs text-slate-500 leading-relaxed">
          Approval is recorded in the Access Requests inbox; your actual permissions will not change
          until your role is updated by HR. This only notifies {targetLabel}.
        </p>
      </div>
    </Modal>
  );
};