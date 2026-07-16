import React from 'react';
import Link from 'next/link';
import { LangPickerTrigger } from '@/components/lang/LangPicker';
import { ThemeToggle } from '@/components/theme';
import { PersonaMenu } from './PersonaMenu';
import { NotificationBell } from './NotificationBell';
import { NudgesLink } from './NudgesLink';

interface TopbarProps {
  users: any[];
  currentUser: any;
}

function envLabel(): string {
  const env = (typeof process !== 'undefined' && (process as any).env?.NEXT_PUBLIC_ENV) || 'dev';
  return env.toUpperCase();
}

export const Topbar: React.FC<TopbarProps> = ({ users, currentUser }) => {
  return (
    <header role="banner" className="glass-panel-heavy sticky top-0 z-40 flex h-14 items-center rounded-b-2xl border-b border-rule px-3 sm:px-4">
      <div className="hidden h-full shrink-0 items-center sm:flex">
        <Link href="/" aria-label="Go to home" className="inline-flex items-baseline gap-2">
          <span className="font-display text-xl font-medium tracking-tight text-ink leading-none">Folio</span>
          <span className="border border-rule px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wider text-mute leading-none">
            {envLabel()}
          </span>
        </Link>
      </div>

      <div className="min-w-0 flex-1" />

      <div className="ml-auto flex shrink-0 items-center gap-1.5 sm:gap-2">
        <LangPickerTrigger />
        <ThemeToggle />
        <NudgesLink />
        <NotificationBell />
        {currentUser && <PersonaMenu users={users} currentUser={currentUser} />}
      </div>
    </header>
  );
};

export default Topbar;
