'use client';

export function ChatTrigger({
  open,
  onToggle,
  pillLabel,
  tooltipLabel,
}: {
  open: boolean;
  onToggle: () => void;
  pillLabel: string;
  tooltipLabel: string;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      title={tooltipLabel}
      aria-label={tooltipLabel}
      className={`fixed bottom-4 right-4 z-40 rounded-full border px-4 py-2 text-sm font-mono shadow-xl transition-colors ${
        open
          ? 'bg-indigo-600 border-indigo-400 text-white'
          : 'bg-slate-900 border-indigo-500/40 text-indigo-200 hover:bg-slate-800'
      }`}
    >
      {pillLabel}
    </button>
  );
}
