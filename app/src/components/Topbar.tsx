import React from 'react';
import Link from 'next/link';
import { Icon } from '@/components/icons';
import { LangPickerTrigger } from '@/components/lang/LangPicker';
import { ThemeToggle } from '@/components/theme';
import { PersonaMenu } from './PersonaMenu';
import { NotificationBell } from './NotificationBell';

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
    <header role="banner" className="glass-panel-heavy sticky top-0 z-40 h-[3.5rem] border-b border-rule rounded-b-2xl flex items-center">
      <div className="flex items-center gap-3 pl-4 pr-2 h-full shrink-0">
        <Link href="/" aria-label="Go to home" className="inline-flex items-baseline gap-2">
          <span className="font-display text-xl font-medium tracking-tight text-ink leading-none">Folio</span>
          <span className="text-sm px-1.5 py-0.5 border border-rule font-mono uppercase tracking-wider text-mute leading-none">
            {envLabel()}
          </span>
        </Link>
      </div>

      <div className="flex-1 px-3 max-w-xl min-w-0" />

      <div className="ml-auto flex items-center gap-2 pr-4 shrink-0">
        <LangPickerTrigger />
        <ThemeToggle />
        <NotificationBell />
        {currentUser && <PersonaMenu users={users} currentUser={currentUser} />}
      </div>
    </header>
  );
};

export default Topbar;
