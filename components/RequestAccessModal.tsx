'use client';

import React, { useState } from 'react';
import { Modal } from '@/components/ui/Modal';
import { useToast } from '@/components/ui/Toast';
import { T } from '@/components/i18n/T';
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
      title={<T id="access.requestAccess" hideSecondary />}
      subtitle={<T id="access.askTarget" hideSecondary values={{ target: targetLabel }} />}
      tone="cyan"
      width="md"
      footer={
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="px-3 py-1.5 rounded-lg text-sm font-mono font-bold uppercase tracking-wider text-ink-2 hover:text-ink border border-rule hover:border-rule"
          >
            <T id="common.cancel" hideSecondary />
          </button>
          <button
            type="button"
            onClick={submit}
            disabled={busy}
            className="px-3 py-1.5 rounded-lg text-sm font-mono font-bold uppercase tracking-wider text-info-soft bg-info border border-info hover:bg-info disabled:opacity-50"
          >
            {busy ? <T id="access.requestSending" hideSecondary /> : <T id="access.requestSubmit" hideSecondary />}
          </button>
        </div>
      }
    >
      <div className="space-y-3">
        <div className="rounded-md border border-rule bg-paper-2/60 px-3 py-2 text-sm font-mono text-ink-2 flex items-center gap-2">
          <span className="text-base">{tile.icon}</span>
          <div className="min-w-0">
            <div className="text-ink truncate">{tile.display_name}</div>
            <div className="text-mute truncate">{tile.subtitle}</div>
          </div>
        </div>
        <label className="block">
          <span className="text-xs font-mono uppercase tracking-wider text-ink-2"><T id="access.requestReason" hideSecondary /></span>
          <textarea
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="Why do you need access?"
            rows={4}
            className="mt-1 w-full rounded-md bg-paper-2/70 border border-rule px-3 py-2 text-xs text-ink placeholder:text-mute focus:outline-none focus:border-info"
          />
        </label>
        <p className="text-xs text-mute leading-relaxed">
          <T id="access.requestNote" hideSecondary values={{ target: targetLabel }} />
        </p>
      </div>
    </Modal>
  );
};
