import React, { type ReactNode } from 'react';
import { Topbar } from './Topbar';
import { Sidebar } from './Sidebar';
import { Crumbs, type Crumb } from '@/components/ui/Crumbs';
import { loadActor } from '@/server/guard';
import { getDashboardData } from '@/dashboard/queries';

interface LayOutProps {
  children: ReactNode;
  crumbs?: Crumb[];
  fullBleed?: boolean;
}

export async function LayOut({ children, crumbs, fullBleed }: LayOutProps) {
  const [actor, data] = await Promise.all([
    loadActor(),
    getDashboardData(),
  ]);
  const users = (data.users || []) as any[];

  return (
    <div className="min-h-screen flex flex-col">
      <Topbar users={users} currentUser={actor as any} />
      <div className="flex flex-1 min-h-0">
        <Sidebar currentUser={actor as any} />
        <main className="flex-1 min-w-0 flex flex-col">
          {crumbs && crumbs.length > 0 && (
            <div className="glass-panel-heavy px-4 sm:px-6 pt-3 pb-1 border-b border-rule rounded-b-2xl">
              <Crumbs crumbs={crumbs} />
            </div>
          )}
          <div className={fullBleed ? 'flex-1 min-h-0' : 'flex-1 min-h-0 px-4 sm:px-6 lg:px-8 py-6'}>
            {children}
          </div>
        </main>
      </div>
    </div>
  );
}

export default LayOut;
