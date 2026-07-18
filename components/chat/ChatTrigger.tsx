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
      className={`fixed bottom-4 right-4 z-sticky rounded-full border px-4 py-2 text-sm font-mono shadow-xl transition-colors ${
        open
          ? 'bg-accent-strong border-accent text-ink'
          : 'bg-paper border-accent text-accent-soft hover:bg-paper-2'
      }`}
    >
      {pillLabel}
    </button>
  );
}
