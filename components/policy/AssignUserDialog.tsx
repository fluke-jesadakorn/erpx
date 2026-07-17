'use client';

import React, { useEffect, useState } from 'react';
import { T } from '@/components/i18n/T';

interface User {
  id: number;
  fullname: string;
  employee_code: string;
  department: string | null;
  role_id: string | null;
}

interface Props {
  onClose: () => void;
  targets: { id: string; kind: 'department' | 'role'; label: string }[];
}

export function AssignUserDialog({ onClose, targets }: Props) {
  const [users, setUsers] = useState<User[] | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [selectedDept, setSelectedDept] = useState<string>('');
  const [selectedRoles, setSelectedRoles] = useState<string[]>([]);
  const [search, setSearch] = useState('');
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    setUsers(null);
    setSelectedId(null);
    setSelectedDept('');
    setSelectedRoles([]);
    setMsg(null);
    fetch('/api/perm/users')
      .then((r) => r.json())
      .then((j) => setUsers(j.users ?? []))
      .catch(() => setMsg('Failed to load users'));
  }, []);

  const departments = targets.filter((t) => t.kind === 'department');
  const roles = targets.filter((t) => t.kind === 'role');

  const filtered = (users ?? []).filter((u) => {
    if (!search.trim()) return true;
    const q = search.trim().toLowerCase();
    return (
      u.fullname.toLowerCase().includes(q) ||
      u.employee_code.toLowerCase().includes(q) ||
      (u.department ?? '').toLowerCase().includes(q)
    );
  });

  const toggleRole = (rid: string) => {
    setSelectedRoles((prev) =>
      prev.includes(rid) ? prev.filter((r) => r !== rid) : [...prev, rid],
    );
  };

  const save = async () => {
    if (!selectedId) return;
    setSaving(true);
    setMsg(null);
    try {
      const desired: string[] = [];
      if (selectedDept) desired.push(`user:dept:${selectedDept}::allow`);
      for (const rid of selectedRoles) desired.push(`user:dept:${rid}::allow`);
      const r = await fetch(`/api/perm/users/${selectedId}/grants`, {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ mode: 'permanent', desired_perm_ids: desired }),
      });
      const r2 = await fetch(`/api/perm/users/${selectedId}/roles`, {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ roles: selectedRoles }),
      });
      if (!r.ok || !r2.ok) throw new Error('Save failed');
      setMsg('Saved');
      setTimeout(() => onClose(), 500);
    } catch (e: unknown) {
      setMsg(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-slate-950/60 flex items-center justify-center p-4">
      <div className="w-full max-w-2xl bg-white border border-slate-300 shadow-xl">
        <h3 className="text-base font-black uppercase tracking-widest text-slate-100">
          <T id="policy.matrix2.assignUser" hideSecondary />
        </h3>

        <div>
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search user…"
            className="w-full px-3 py-2 rounded-md text-xs font-mono bg-slate-950 border border-slate-700 text-slate-100"
          />
          <div className="max-h-40 overflow-auto mt-2 border border-slate-800 rounded">
            {filtered.map((u) => (
              <button
                key={u.id}
                type="button"
                onClick={() => {
                  setSelectedId(u.id);
                  setSelectedDept(u.department ?? '');
                }}
                className={[
                  'w-full text-left px-2 py-1 text-xs font-mono flex items-center justify-between border-b border-slate-800 last:border-b-0',
                  selectedId === u.id ? 'bg-indigo-500/15 text-indigo-100' : 'hover:bg-slate-900/50 text-slate-300',
                ].join(' ')}
              >
                <span>{u.fullname}</span>
                <span className="text-[10px] text-slate-500">{u.employee_code} · {u.department ?? '—'}</span>
              </button>
            ))}
            {users === null ? (
              <div className="px-2 py-2 text-xs text-slate-500">Loading…</div>
            ) : null}
            {users !== null && filtered.length === 0 ? (
              <div className="px-2 py-2 text-xs text-slate-500">No matches</div>
            ) : null}
          </div>
        </div>

        <div>
          <div className="text-[10px] font-mono uppercase tracking-widest text-slate-400 mb-1">
            <T id="policy.matrix2.assignDept" hideSecondary /> (one)
          </div>
          <div className="flex flex-wrap gap-1.5">
            {departments.map((d) => (
              <button
                key={d.id}
                type="button"
                onClick={() => setSelectedDept(d.id)}
                className={[
                  'px-2 py-1 rounded text-[10px] font-mono uppercase border',
                  selectedDept === d.id
                    ? 'bg-sky-500/25 border-sky-400/60 text-sky-100'
                    : 'border-slate-700 text-slate-300 hover:bg-slate-900/60',
                ].join(' ')}
              >
                {d.label}
              </button>
            ))}
          </div>
        </div>

        <div>
          <div className="text-[10px] font-mono uppercase tracking-widest text-slate-400 mb-1">
            <T id="policy.matrix2.assignRoles" hideSecondary /> (multiple)
          </div>
          <div className="flex flex-wrap gap-1.5 max-h-32 overflow-auto p-2 border border-slate-800 rounded">
            {roles.map((r) => (
              <button
                key={r.id}
                type="button"
                onClick={() => toggleRole(r.id)}
                className={[
                  'px-2 py-1 rounded text-[10px] font-mono uppercase border',
                  selectedRoles.includes(r.id)
                    ? 'bg-violet-500/25 border-violet-400/60 text-violet-100'
                    : 'border-slate-700 text-slate-400 hover:bg-slate-900/60',
                ].join(' ')}
              >
                {r.label}
              </button>
            ))}
          </div>
        </div>

        {msg ? <div className="text-xs text-slate-300">{msg}</div> : null}

        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onClose}
            className="px-3 py-1.5 rounded text-xs font-mono border border-slate-700 text-slate-300 hover:bg-slate-800"
          >
            <T id="policy.matrix2.cancel" hideSecondary />
          </button>
          <button
            type="button"
            disabled={!selectedId || saving}
            onClick={save}
            className="px-3 py-1.5 rounded text-xs font-mono border border-indigo-500/60 bg-indigo-500/30 text-indigo-100 hover:bg-indigo-500/40 disabled:opacity-40"
          >
            {saving ? 'Saving…' : <T id="policy.matrix2.save" hideSecondary />}
          </button>
        </div>
      </div>
    </div>
  );
}
