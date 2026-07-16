'use client';

import React, { useState } from 'react';
import Link from 'next/link';
import { RequestAccessModal } from './RequestAccessModal';
import type { TileDef } from './tile-config';
import type { TileAccess } from './tileAccess';
import { T } from '@/components/i18n/T';

export type NoPermissionKind = 'locked' | 'stage_locked' | 'not_found';

interface NoPermissionViewProps {
  kind: NoPermissionKind;
  actor?: {
    id: number;
    role_name?: string;
    fullname?: string;
    department?: string | null;
  } | null;
  tile?: TileDef | null;
  access?: TileAccess | null;
  reason?: React.ReactNode;
  attemptedPath?: string;
}

const KIND_META: Record<NoPermissionKind, { id: string; icon: string; tone: string }> = {
  locked:        { id: 'access.lockedTitle',        icon: '🔒', tone: 'rose' },
  stage_locked:  { id: 'access.stageLockedTitle',   icon: '⏳', tone: 'amber' },
  not_found:     { id: 'access.notFoundTitle',      icon: '🧭', tone: 'slate' },
};

function isDev() {
  return process.env.NODE_ENV !== 'production';
}

function targetLabel(t?: TileDef | { requestAccessTarget?: 'hr_manager' | 'cfo' | 'admin' }): string {
  switch (t?.requestAccessTarget) {
    case 'cfo': return 'CFO';
    case 'admin': return 'Admin';
    case 'hr_manager':
    default: return 'HR Manager';
  }
}

export const NoPermissionView: React.FC<NoPermissionViewProps> = ({
  kind,
  actor,
  tile,
  access,
  reason,
  attemptedPath,
}) => {
  const meta = KIND_META[kind];
  const [modalOpen, setModalOpen] = useState(false);

  const currentRole = actor?.role_name;
  const isCeoOrAdmin = currentRole === 'ceo' || currentRole === 'admin';

  return (
    <div className="max-w-4xl mx-auto animate-fade-in">
      <div className="glass-panel p-8 sm:p-12 rounded-3xl border-slate-800 text-center relative overflow-hidden shadow-2xl">
        <div className={`absolute -top-24 -right-24 w-72 h-72 bg-${meta.tone}-500/10 rounded-full blur-3xl pointer-events-none`} />
        <div className={`absolute -bottom-24 -left-24 w-72 h-72 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none`} />

        <div className="relative z-10">
          <div className={`inline-flex items-center justify-center w-20 h-20 rounded-3xl bg-gradient-to-br from-${meta.tone}-600/20 to-${meta.tone}-900/30 border border-${meta.tone}-500/30 mb-6 shadow-xl`}>
            <span className="text-4xl">{meta.icon}</span>
          </div>

          <h1 className="text-2xl font-black text-white mb-2 tracking-tight">
            <T id={meta.id} hideSecondary />
          </h1>
          <p className="text-sm text-slate-300 max-w-xl mx-auto leading-relaxed">
            <T id={`access.${kind}Subtitle`} hideSecondary />
          </p>

          {attemptedPath && kind === 'not_found' && (
            <div className="mt-4 inline-block px-3 py-1.5 rounded-lg bg-slate-950/80 border border-slate-800 font-mono text-sm text-slate-400">
              <span className="text-slate-500"><T id="access.pathLabel" hideSecondary /></span>{' '}
              <span className="text-slate-200">{attemptedPath}</span>
            </div>
          )}

          {(reason || (tile && kind !== 'not_found')) && (
            <div className="mt-6 p-4 bg-slate-950/80 rounded-2xl border border-slate-800 text-left text-xs space-y-3">
              {tile && (
                <div className="flex items-start gap-3">
                  <span className="text-2xl shrink-0">{tile.icon}</span>
                  <div className="min-w-0">
                    <div className="text-white font-bold truncate">{tile.display_name}</div>
                    <div className="text-slate-500 truncate">{tile.subtitle}</div>
                  </div>
                </div>
              )}

              {reason && (
                <p className="text-slate-300 font-sans leading-relaxed">{reason}</p>
              )}

              {currentRole && (
                <div className="flex justify-between items-center pt-2 border-t border-slate-900">
                  <span className="text-slate-400 font-mono text-sm"><T id="access.yourRole" hideSecondary /></span>
                  <span className="px-2.5 py-0.5 rounded-full bg-amber-500/10 text-amber-300 border border-amber-500/30 font-bold font-mono text-xs uppercase">
                    {currentRole}
                  </span>
                </div>
              )}

              {access?.source && (
                <div className="flex justify-between items-center pt-2 border-t border-slate-900">
                  <span className="text-slate-400 font-mono text-sm"><T id="access.source" hideSecondary /></span>
                  <span className="px-2 py-0.5 rounded-full bg-slate-800/60 text-slate-300 border border-slate-700 font-mono text-xs">
                    {access.source}{access.inheritedFrom ? ` (${access.inheritedFrom})` : ''}
                  </span>
                </div>
              )}
            </div>
          )}

          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Link
              href="/"
              className="px-4 py-2.5 rounded-2xl bg-slate-900/80 border border-slate-800 hover:border-slate-700 text-slate-200 text-xs font-bold font-mono uppercase tracking-wider transition-colors"
            >
              ← <T id="access.backHome" hideSecondary />
            </Link>

            {kind === 'locked' && tile && actor?.id && (
              <button
                type="button"
                onClick={() => setModalOpen(true)}
                className="px-4 py-2.5 rounded-2xl bg-cyan-500/20 border border-cyan-500/40 hover:bg-cyan-500/30 text-cyan-100 text-xs font-bold font-mono uppercase tracking-wider transition-colors"
              >
                ✉ <T id="access.requestAccessFrom" hideSecondary values={{ target: targetLabel(tile) }} />
              </button>
            )}

            {kind === 'stage_locked' && isCeoOrAdmin && (
              <Link
                href="/cockpit"
                className="px-4 py-2.5 rounded-2xl bg-rose-500/20 border border-rose-500/40 hover:bg-rose-500/30 text-rose-100 text-xs font-bold font-mono uppercase tracking-wider transition-colors"
              >
                ⚡ <T id="access.openOverrideConsole" hideSecondary />
              </Link>
            )}

            {isDev() && (
              <Link
                href="/"
                className="px-4 py-2.5 rounded-2xl bg-indigo-500/20 border border-indigo-500/40 hover:bg-indigo-500/30 text-indigo-100 text-xs font-bold font-mono uppercase tracking-wider transition-colors"
              >
                🧪 <T id="access.switchPersonaDev" hideSecondary />
              </Link>
            )}
          </div>

          <p className="text-sm text-slate-500 mt-6 font-sans leading-relaxed">
            <T id={`access.help.${kind}`} hideSecondary />
          </p>
        </div>
      </div>

      {tile && actor?.id && (
        <RequestAccessModal
          open={modalOpen}
          onClose={() => setModalOpen(false)}
          tile={tile}
          actorId={actor.id}
          targetLabel={targetLabel(tile)}
        />
      )}
    </div>
  );
};

export default NoPermissionView;
