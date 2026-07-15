interface Props {
  summary: {
    role: string;
    role_label: string;
    role_label_th?: string | null;
    names: string[];
    count: number;
    privacy: 'named' | 'team';
  } | null;
  view: 'awaiting' | 'can-act';
}

export function ApproverChip({ summary, view }: Props) {
  if (!summary) {
    return (
      <span className="inline-flex items-center gap-1 rounded-md border border-slate-700 bg-slate-900/60 px-2 py-0.5 text-xs font-mono text-slate-500">
        ⏳ Awaiting: —
      </span>
    );
  }
  const roleLabel = summary.role_label_th ?? summary.role_label;
  const head = view === 'can-act'
    ? `You act as · ${roleLabel}`
    : `⏳ Awaiting · ${roleLabel}`;
  const names = summary.privacy === 'team'
    ? `${roleLabel} team (${summary.count})`
    : summary.names.slice(0, 2).join(', ') + (summary.names.length > 2 ? ` +${summary.names.length - 2}` : '');
  return (
    <span className="inline-flex items-center gap-1 rounded-md border border-amber-500/40 bg-amber-500/10 px-2 py-0.5 text-xs font-mono text-amber-200">
      <span>{head}</span>
      <span className="text-slate-300">·</span>
      <span className="text-amber-100">{names}</span>
    </span>
  );
}
