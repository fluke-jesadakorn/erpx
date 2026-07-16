'use client';
import { useEffect, useState } from 'react';
import { usePathname, useSearchParams } from 'next/navigation';
import { useActor } from '@/components/ActorProvider';
import { useHasPerm } from '@/perm/client';
import { deriveScope } from './scope';
import { ChatTrigger } from './ChatTrigger';
import { ChatPanel } from './ChatPanel';

export function GlobalChat() {
  const pathname = usePathname() ?? '';
  const search = useSearchParams()?.toString() ?? '';
  const { actor } = useActor();
  const perms = (actor?.permissions as string[] | undefined) ?? null;
  const allowed = useHasPerm(perms, 'tile:chat:view::allow');
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k' && !e.shiftKey) {
        const target = e.target as HTMLElement | null;
        const tag = target?.tagName?.toLowerCase();
        if (tag === 'input' || tag === 'textarea' || target?.isContentEditable) return;
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!allowed) return null;
  if (pathname === '/login' || pathname === '/chat') return null;
  const scope = deriveScope(pathname, search);
  if (!scope) return null;

  return (
    <>
      <ChatTrigger
        open={open}
        onToggle={() => setOpen((o) => !o)}
        pillLabel={`💬 ${scope.displayName}`}
        tooltipLabel={`Open AI chat for ${scope.displayName}`}
      />
      {open && <ChatPanel scope={scope} onClose={() => setOpen(false)} />}
    </>
  );
}