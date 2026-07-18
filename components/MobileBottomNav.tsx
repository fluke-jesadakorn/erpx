'use client';

import Link from 'next/link';
import { usePathname, useSearchParams } from 'next/navigation';
import { Home, Inbox, Plus, FileText, User, type LucideIcon } from 'lucide-react';

interface Item {
  label: string;
  href: string;
  icon: LucideIcon;
  active: boolean;
  new?: boolean;
}

function isActive(pathname: string, search: string, href: string) {
  if (href === '/') return pathname === '/' || pathname === '';
  if (href === '/inbox?scope=waiting') return pathname === '/inbox' && search === '?scope=waiting';
  if (href === '/inbox?scope=watching') return pathname === '/inbox' && search === '?scope=watching';
  return pathname === href;
}

export function MobileBottomNav() {
  const pathname = usePathname() || '/';
  const params = useSearchParams();
  const search = params ? `?${params.toString()}` : '';
  const items: Item[] = [
    { label: 'Home', href: '/', icon: Home, active: isActive(pathname, search, '/') },
    { label: 'Approvals', href: '/inbox?scope=waiting', icon: Inbox, active: isActive(pathname, search, '/inbox?scope=waiting') },
    { label: 'New', href: '/cockpit?new=1', icon: Plus, new: true, active: pathname === '/cockpit' && search === '?new=1' },
    { label: 'Inbox', href: '/inbox?scope=watching', icon: FileText, active: isActive(pathname, search, '/inbox?scope=watching') },
    { label: 'Me', href: '/me', icon: User, active: pathname === '/me' || pathname.startsWith('/me/') },
  ];

  return (
    <nav className="panel-floating safe-bottom fixed inset-x-3 bottom-3 z-sticky flex min-h-16 overflow-visible rounded-2xl md:hidden" aria-label="Mobile navigation">
      {items.map((item) => {
        const IconCmp = item.icon;
        return (
          <Link
            key={item.href}
            href={item.href}
            aria-current={item.active ? 'page' : undefined}
            className={[
              'relative flex flex-1 flex-col items-center justify-center rounded-xl py-1.5 text-[10px] text-mute transition-colors hover:bg-paper-3/45 hover:text-ink',
              item.active ? 'text-accent' : '',
            ].join(' ')}
          >
            {item.new ? (
              <span className="-mt-5 inline-flex h-11 w-11 items-center justify-center rounded-2xl border border-accent/60 bg-accent text-paper shadow-[0_12px_26px_-12px_color-mix(in_oklab,var(--accent)_85%,transparent)]">
                <IconCmp size={18} />
              </span>
            ) : (
              <span className={item.active ? 'rounded-lg bg-accent-soft/65 p-1.5' : 'p-1.5'}><IconCmp size={17} /></span>
            )}
            <span className={item.new ? 'mt-0.5' : 'mt-0.5'}>{item.label}</span>
          </Link>
        );
      })}
    </nav>
  );
}

export default MobileBottomNav;
