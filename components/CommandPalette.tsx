'use client';

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { type RoleName } from '@/org/display';
import { type TileDef, tileHref } from './tile-config';
import { roleGlyph } from './UserAvatar';
import { T } from '@/components/i18n/T';

interface AiIntent {
  id: string;
  label: string;
  group: string;
  glyph: string;
  perform: () => void;
}

export interface PaletteAction {
  id: string;
  label: string;
  hint?: string;
  group: string;
  glyph?: string;
  perform: () => void;
}

interface CommandPaletteProps {
  role: RoleName | undefined;
  onNavigate: (href: string) => void;
  users: any[];
  currentUser: any;
  openCommand: boolean;
  setOpenCommand: (v: boolean) => void;
  tiles?: TileDef[];
}

export const CommandPalette: React.FC<CommandPaletteProps> = ({
  role,
  onNavigate,
  users,
  currentUser,
  openCommand,
  setOpenCommand,
  tiles: tilesProp = [],
}) => {
  const router = useRouter();
  const [query, setQuery] = useState('');
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [aiIntent, setAiIntent] = useState<AiIntent | null>(null);
  const [aiBusy, setAiBusy] = useState(false);
  const aiReqId = useRef(0);

  const switchPersona = async (userId: number) => {
    try {
      await fetch('/api/actor', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ id: userId }),
      });
    } catch {}
    router.refresh();
  };

  const tiles = tilesProp;
  const openTiles = tiles;
  const lockedTiles: TileDef[] = [];

  const TILE_HREFS: Record<string, string> = useMemo(() => {
    const out: Record<string, string> = {};
    for (const t of tiles) out[t.id] = tileHref(t.id);
    return out;
  }, [tiles]);

  const actions = useMemo<PaletteAction[]>(() => {
    const tileActions: PaletteAction[] = tiles.map((t) => ({
      id: `tile:${t.id}`,
      label: t.display_name,
      hint: t.subtitle,
      group: 'Tiles',
      glyph: t.icon,
      perform: () => onNavigate(tileHref(t.id)),
    }));

    const personaPrefixes = ['as ', 'role ', 'user ', '@', 'login '];
    const q = query.trim().toLowerCase();
    const personaActive =
      personaPrefixes.some((p) => q.startsWith(p)) || q === 'as' || q === 'role' || q === 'user';

    const personas: PaletteAction[] = personaActive
      ? users.slice(0, 8).map((u) => ({
          id: `user:${u.id}`,
          label: `${u.fullname} · ${u.employee_code}`,
          hint: u.role_name,
          group: 'Switch Role',
          glyph: roleGlyph(u.role_name),
          perform: () => switchPersona(u.id),
        }))
      : q
        ? users.slice(0, 3).map((u) => ({
            id: `user:${u.id}`,
            label: `${u.fullname} · ${u.employee_code}`,
            hint: u.role_name,
            group: 'Switch Role',
            glyph: roleGlyph(u.role_name),
            perform: () => switchPersona(u.id),
          }))
        : [];

    return [...tileActions, ...personas];
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [role, currentUser, users, tiles, onNavigate, query]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return actions;
    return actions.filter((a) => a.label.toLowerCase().includes(q) || a.hint?.toLowerCase().includes(q));
  }, [actions, query]);

  useEffect(() => {
    const q = query.trim();
    if (q.length < 4) {
      setAiIntent(null);
      return;
    }
    const hasStrongStatic = filtered.some((a) => a.label.toLowerCase().startsWith(q.toLowerCase()));
    if (hasStrongStatic) {
      setAiIntent(null);
      return;
    }
    const reqId = ++aiReqId.current;
    setAiBusy(true);
    const timer = setTimeout(async () => {
      try {
        const tileList = Object.keys(TILE_HREFS).join(', ');
        const res = await fetch('/api/ai/invoke', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            sectionKey: 'command:intent',
            task: 'chat',
            systemPrompt: `You interpret a user's natural-language command in a finance ERP and pick the best destination. Available tiles: ${tileList || 'none'}. Reply with ONLY a JSON object on one line: {"action":"tile","target":"<tile_id>","label":"<short label>"} or {"action":"none","label":"<what user asked>"}. Do not include any other text.`,
            text: q,
          }),
        });
        const data = await res.json();
        if (reqId !== aiReqId.current) return;
        if (!data.ok || !data.text) { setAiIntent(null); return; }
        const m = data.text.match(/\{[\s\S]*\}/);
        if (!m) { setAiIntent(null); return; }
        const parsed = JSON.parse(m[0]);
        if (parsed.action === 'tile' && typeof parsed.target === 'string' && TILE_HREFS[parsed.target]) {
          const href = TILE_HREFS[parsed.target];
          setAiIntent({
            id: 'ai:intent',
            label: `✨ ${parsed.label || `Open ${parsed.target}`}`,
            group: 'AI Suggestion',
            glyph: '✨',
            perform: () => onNavigate(href),
          });
        } else {
          setAiIntent(null);
        }
      } catch {
        setAiIntent(null);
      } finally {
        if (reqId === aiReqId.current) setAiBusy(false);
      }
    }, 350);
    return () => clearTimeout(timer);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, filtered.length, role, currentUser]);

  void openTiles;
  void lockedTiles;

  useEffect(() => {
    const text = query.trim();
    if (text.length < 3) {
      setAiIntent(null);
      return;
    }
    if (highlight > 0) return;

    const reqId = ++aiReqId.current;
    setAiBusy(true);
    const handle = setTimeout(async () => {
      try {
        const res = await fetch('/api/command/intent', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ query: text }),
        });
        const json = await res.json();
        if (reqId !== aiReqId.current) return;
        if (res.ok && json.ok && json.intent?.top) {
          const top = json.intent.top;
          setAiIntent({
            id: top.tileId,
            label: `AI · ${top.tileId} (${Math.round(top.confidence * 100)}%)`,
            group: 'AI guess',
            glyph: '✨',
            perform: () => {
              onNavigate(`/${top.tileId}`);
              setOpenCommand(false);
            },
          });
        } else {
          setAiIntent(null);
        }
      } catch {
        setAiIntent(null);
      } finally {
        if (reqId === aiReqId.current) setAiBusy(false);
      }
    }, 350);

    return () => clearTimeout(handle);
  }, [query, highlight, onNavigate, setOpenCommand]);

  const finalList = useMemo(() => {
    if (!aiIntent) return filtered;
    return [aiIntent, ...filtered];
  }, [filtered, aiIntent]);

  const grouped = useMemo(() => {
    const out: Record<string, PaletteAction[]> = {};
    for (const a of finalList) {
      if (!out[a.group]) out[a.group] = [];
      out[a.group].push(a);
    }
    const order = ['Tiles', 'AI Suggestion', 'Switch Role'];
    return Object.entries(out).sort(
      ([a], [b]) => {
        const ai = order.indexOf(a);
        const bi = order.indexOf(b);
        if (ai === -1 && bi === -1) return a.localeCompare(b);
        if (ai === -1) return 1;
        if (bi === -1) return -1;
        return ai - bi;
      },
    );
  }, [finalList]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setOpenCommand(true);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [setOpenCommand]);

  useEffect(() => {
    if (!openCommand) {
      setQuery('');
      setHighlight(0);
      setAiIntent(null);
    } else {
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [openCommand]);

  useEffect(() => {
    if (!openCommand) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setOpenCommand(false);
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setHighlight((h) => Math.min(h + 1, finalList.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setHighlight((h) => Math.max(h - 1, 0));
      } else if (e.key === 'Enter') {
        const a = finalList[highlight];
        if (a) {
          a.perform();
          setOpenCommand(false);
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [openCommand, finalList, highlight, setOpenCommand]);

  useEffect(() => setHighlight(0), [query]);

  if (!openCommand) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[12vh] px-4 animate-fade-in">
      <div
        className="absolute inset-0 bg-slate-950/70 backdrop-blur-sm"
        onClick={() => setOpenCommand(false)}
        aria-hidden
      />
      <div
        role="dialog"
        aria-label="Command palette"
        className="relative w-full max-w-xl glass-panel-heavy rounded-2xl shadow-2xl shadow-black border-indigo-500/30 flex flex-col max-h-[70vh] overflow-hidden"
      >
        <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-800/80">
          <span className="text-indigo-400 text-lg">⌘</span>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search tiles, features, or pages — type “as …” to switch user"
            className="flex-1 bg-transparent outline-none text-sm text-white placeholder:text-slate-500"
          />
          <kbd className="hidden sm:inline-flex text-xs font-mono px-1.5 py-0.5 rounded border border-slate-700 text-slate-400 bg-slate-900">
            ESC
          </kbd>
        </div>

        <div className="flex-1 overflow-y-auto px-1.5 py-2">
          {finalList.length === 0 ? (
            <div className="px-3 py-12 text-center">
              <div className="text-2xl mb-2">🔍</div>
              <div className="text-xs text-slate-400">
                {aiBusy
                  ? <><span className="mr-1">✨</span><T id="command.askingAi" hideSecondary /></>
                  : <T id="command.noMatch" hideSecondary values={{ query }} />}
              </div>
            </div>
          ) : (
            grouped.map(([group, items]) => {
              return (
                <div key={group} className="px-1 mb-1">
                  <div className="px-2 py-1 text-xs uppercase tracking-widest font-mono text-slate-500">
                    {group === 'Tiles' ? <T id="command.groupTiles" hideSecondary />
                      : group === 'AI Suggestion' ? <T id="command.groupAiSuggestion" hideSecondary />
                      : group === 'AI guess' ? <T id="command.groupAiGuess" hideSecondary />
                      : group === 'Switch Role' ? <T id="command.groupSwitchRole" hideSecondary />
                      : group}
                  </div>
                  {items.map((a) => {
                    const idx = finalList.indexOf(a);
                    const focused = idx === highlight;
                    return (
                      <button
                        key={a.id}
                        type="button"
                        onMouseEnter={() => setHighlight(idx)}
                        onClick={() => {
                          a.perform();
                          setOpenCommand(false);
                        }}
                        className={[
                          'w-full flex items-center gap-3 px-3 py-2.5 rounded-xl text-left transition-all',
                          focused ? 'bg-indigo-500/15 border border-indigo-500/40 text-white' : 'border border-transparent text-slate-200 hover:bg-slate-900/60',
                        ].join(' ')}
                      >
                        <span className="w-7 h-7 inline-flex items-center justify-center rounded-lg bg-slate-900 border border-slate-800 text-base shrink-0">
                          {a.glyph || '⚡'}
                        </span>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium truncate">{a.label}</div>
                          {a.hint && <div className="text-xs text-slate-500 font-mono truncate">{a.hint}</div>}
                        </div>
                        {focused && <span className="text-xs font-mono text-indigo-300 px-1.5 py-0.5 rounded border border-indigo-500/40">↵</span>}
                      </button>
                    );
                  })}
                </div>
              );
            })
          )}
        </div>

        <div className="px-3 py-2 border-t border-slate-800/80 text-xs text-slate-500 font-mono flex items-center justify-between">
          <span>
            <T id="command.matches" hideSecondary values={{ n: finalList.length }} />
            {aiBusy && <span className="ml-2 text-indigo-300"><span className="mr-1">✨</span><T id="command.aiThinking" hideSecondary /></span>}
          </span>
          <span className="flex items-center gap-2">
            <span><T id="command.keysSelect" hideSecondary /></span>
            <span><T id="command.keysRun" hideSecondary /></span>
            <span><T id="command.keysClose" hideSecondary /></span>
          </span>
        </div>
      </div>
    </div>
  );
};
