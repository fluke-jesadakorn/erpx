'use client';

import Link from 'next/link';
import { usePathname, useSearchParams } from 'next/navigation';
import { Icon, type IconName } from '@/components/icons';

interface Item {
  label: string;
  href: string;
  icon: IconName;
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
    { label: 'Home', href: '/', icon: 'home', active: isActive(pathname, search, '/') },
    { label: 'Approvals', href: '/inbox?scope=waiting', icon: 'inbox', active: isActive(pathname, search, '/inbox?scope=waiting') },
    { label: 'New', href: '/cockpit?new=1', icon: 'plus', new: true, active: pathname === '/cockpit' && search === '?new=1' },
    { label: 'Inbox', href: '/inbox?scope=watching', icon: 'file-text', active: isActive(pathname, search, '/inbox?scope=watching') },
    { label: 'Me', href: '/me', icon: 'user', active: pathname === '/me' || pathname.startsWith('/me/') },
  ];

  return (
    <nav className="fixed inset-x-0 bottom-0 z-30 flex h-14 border-t border-rule bg-paper-2 md:hidden" aria-label="Mobile navigation">
      {items.map((item) => (
        <Link
          key={item.href}
          href={item.href}
          aria-current={item.active ? 'page' : undefined}
          className={[
            'flex flex-1 flex-col items-center justify-center py-1.5 text-xs text-mute hover:text-ink',
            item.active ? 'border-t-2 border-accent -mt-px text-ink' : 'border-t-2 border-transparent',
          ].join(' ')}
        >
          {item.new ? (
            <span className="-mt-4 inline-flex h-9 w-9 items-center justify-center rounded-full bg-accent text-accent-ink">
              <Icon name={item.icon} size={18} />
            </span>
          ) : (
            <Icon name={item.icon} size={18} />
          )}
          <span className={item.new ? 'mt-0.5' : 'mt-0.5'}>{item.label}</span>
        </Link>
      ))}
    </nav>
  );
}

export default MobileBottomNav;
