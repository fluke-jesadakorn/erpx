import React from 'react';
import Link from 'next/link';
import { LangPickerTrigger } from '@/components/lang/LangPicker';
import { ThemeToggle } from '@/components/theme';
import { PersonaMenu } from './PersonaMenu';
import { NotificationBell } from './NotificationBell';
import { NudgesLink } from './NudgesLink';
import { MobileMenuButton, SearchButton } from './MobileDrawer';
import { TopbarCrumbs } from './TopbarCrumbs';

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
    <header role="banner" className="sticky top-0 z-sticky flex h-16 items-center px-3 py-2 sm:px-4">
      <div className="panel-floating flex h-12 w-full items-center gap-2 rounded-2xl px-2.5 sm:px-3.5">
        <MobileMenuButton className="lg:hidden" />
      <div className="hidden h-full shrink-0 items-center sm:flex">
        <Link href="/" aria-label="Go to home" className="inline-flex items-center gap-2.5">
          <span aria-hidden className="flex h-7 w-7 items-center justify-center rounded-lg border border-accent/35 bg-accent-soft/70 text-[11px] font-semibold text-accent shadow-[inset_0_1px_0_rgba(255,255,255,0.16)]">F</span>
          <span className="font-display text-lg font-semibold tracking-[-0.04em] text-ink leading-none">Folio</span>
          <span className="glass-chip px-1.5 py-0.5 text-[9px] font-mono uppercase tracking-[0.14em] text-mute leading-none">
            {envLabel()}
          </span>
        </Link>
      </div>

      <div className="mx-2 hidden h-5 w-px bg-rule md:block" />
      <div className="hidden min-w-0 flex-1 md:block">
        <TopbarCrumbs />
      </div>
      <div className="min-w-0 flex-1 md:hidden" />

      <div className="ml-auto flex shrink-0 items-center gap-1.5 sm:gap-2">
        <SearchButton className="hidden sm:inline-flex" />
        <LangPickerTrigger />
        <ThemeToggle />
        <NudgesLink />
        <NotificationBell />
        {currentUser && <PersonaMenu users={users} currentUser={currentUser} />}
      </div>
      </div>
    </header>
  );
};

export default Topbar;
