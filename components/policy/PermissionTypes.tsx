export interface AdminColumn {
  perm: string;
  domain: string;
  subject: string;
  verb: string;
  description: string | null;
}

export interface AdminTarget {
  id: string;
  kind: 'department' | 'role';
  label: string;
  significance: boolean;
  member_count: number;
}

export function AdminBadge({ target }: { target: AdminTarget }) {
  const isDept = target.kind === 'department';
  return (
    <div className="flex flex-col min-w-0">
      <span className="font-bold text-slate-100 truncate">
        {isDept ? `🏢 ${target.label}` : `👤 ${target.label}`}
      </span>
      <span className="text-[10px] font-mono text-slate-500 truncate">
        {target.id}
      </span>
      <div className="flex flex-wrap items-center gap-1 mt-0.5">
        <span
          className={[
            'px-1 py-0.5 rounded text-[9px] font-mono uppercase border tracking-wider',
            isDept
              ? 'border-sky-500/40 bg-sky-500/10 text-sky-200'
              : 'border-violet-500/40 bg-violet-500/10 text-violet-200',
          ].join(' ')}
        >
          {isDept ? 'dept' : 'role'}
        </span>
        {target.significance ? (
          <span className="px-1 py-0.5 rounded text-[9px] font-mono uppercase border tracking-wider border-amber-500/40 bg-amber-500/10 text-amber-200">
            significant
          </span>
        ) : null}
        <span className="text-[9px] font-mono text-slate-500">
          {target.member_count} {target.member_count === 1 ? 'member' : 'members'}
        </span>
      </div>
    </div>
  );
}
