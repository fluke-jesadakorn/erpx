'use client';


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
      <span className="inline-flex items-center gap-1 rounded-md border border-rule bg-paper-2/60 px-2 py-0.5 text-xs font-mono text-mute">
        ⏳ Awaiting: —
      </span>
    );
  }
  const roleLabel = summary.role_label_th ?? summary.role_label;
  const head = view === 'can-act' ? `You act as · ${roleLabel}` : `⏳ Awaiting · ${roleLabel}`;
  const names = summary.privacy === 'team'
    ? `${roleLabel} team (${summary.count})`
    : summary.names.slice(0, 2).join(', ') + (summary.names.length > 2 ? ` +${summary.names.length - 2}` : '');
  return (
    <span className="inline-flex items-center gap-1 rounded-md border border-caution bg-caution px-2 py-0.5 text-xs font-mono text-caution-soft">
      <span>{head}</span>
      <span className="text-ink-2">·</span>
      <span className="text-caution-soft">{names}</span>
    </span>
  );
}
