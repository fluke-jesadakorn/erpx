import React, { type ReactNode } from 'react';
import { Topbar } from './Topbar';
import { Sidebar } from './Sidebar';
import { Crumbs, type Crumb } from '@/components/ui/Crumbs';
import { loadActor } from '@/server/guard';
import { getDashboardData } from '@/dashboard/queries';
import { MobileBottomNav } from './MobileBottomNav';

interface LayOutProps {
  children: ReactNode;
  crumbs?: Crumb[];
}

export async function LayOut({ children, crumbs }: LayOutProps) {
  const [actor, data] = await Promise.all([
    loadActor(),
    getDashboardData(),
  ]);
  const users = (data.users || []).filter((u: any) => u.is_active) as any[];

  return (
    <div className="app-shell flex min-h-screen flex-col">
      <Topbar users={users} currentUser={actor as any} />
      <div className="flex flex-1 min-h-0">
        <Sidebar currentUser={actor as any} />
        <main className="flex min-w-0 flex-1 flex-col pb-24 md:pb-0">
          {crumbs && crumbs.length > 0 && (
              <div className="bg-paper-2 rounded-none px-4 sm:px-6 pt-3 pb-1 border-b border-rule">
              <Crumbs crumbs={crumbs} />
            </div>
          )}
          <div className="flex-1 min-h-0">
            {children}
          </div>
        </main>
      </div>
      <MobileBottomNav />
    </div>
  );
}

export default LayOut;
