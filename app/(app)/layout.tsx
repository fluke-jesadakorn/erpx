import { Suspense } from 'react';
import { redirect } from 'next/navigation';
import { cookies, headers } from 'next/headers';
import { verifySession } from '@/server/sessionToken';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { loadActor } from '@/server/guard';
import { LayOut } from '@/components/LayOut';
import { ActorProvider, type ActorSnapshot } from '@/components/ActorProvider';
import { OnboardingOverlay } from '@/components/onboarding/OnboardingOverlay';
import { GlobalChat } from '@/components/chat/GlobalChat';
import { IntlProvider } from '@/components/i18n/IntlProvider';

export const dynamic = 'force-dynamic';

export default async function AppLayout({ children }: { children: React.ReactNode }) {
  const tok = (await cookies()).get('folio_session')?.value ?? null;
  const payload = await verifySession(tok);
  if (!payload) redirect('/login');

  const profile = await loadActor();
  if (!profile) redirect('/login');
  const h = await headers();
  const session = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  if (!session) redirect('/login');

  if (!session.session.user.department || session.session.user.role === 'unconfigured') {
    return (
      <IntlProvider>
        <main className="min-h-screen grid place-items-center bg-paper-1 p-6">
          <section className="w-full max-w-lg rounded-xl border border-rule/50 bg-paper-2 p-8 text-center shadow-[var(--shadow-popover)]">
            <p className="text-[11px] font-mono uppercase tracking-[0.16em] text-caution">Access not configured</p>
            <h1 className="mt-3 text-2xl font-bold text-ink">Your account needs a department and role</h1>
            <p className="mt-3 text-sm leading-6 text-mute">
              You are signed in, but Folio access has not been assigned. Contact any active HR or IT member and provide employee ID {profile.employee_code}.
            </p>
          </section>
        </main>
      </IntlProvider>
    );
  }

  const canSeeGlLines = hasPermission(session.session, 'finance:gl:view::allow');
  const canPostAccrual = hasPermission(session.session, 'finance:gl:post::allow');
  const canPostSettlement = hasPermission(session.session, 'finance:gl:post::allow');
  const canConfirmGl = hasPermission(session.session, 'finance:gl:confirm::allow');
  const canSettleExpense = hasPermission(session.session, PERM.finance.expense.settle);
  const canFinalApprove = hasPermission(session.session, PERM.finance.expense.approve);
  const canAttach = hasPermission(session.session, PERM.finance.expense.create);
  const canRemoveAttachment = hasPermission(session.session, PERM.admin.system.bypass);
  const canRecall = hasPermission(session.session, PERM.finance.expense.override);

  const snapshot: ActorSnapshot = {
    id: profile.id,
    fullname: profile.fullname,
    employee_code: profile.employee_code,
    role_name: profile.role_name,
    permissions: profile.permissions,
    policies: {
      canSeeGlLines,
      canPostAccrual,
      canPostSettlement,
      canConfirmGl,
      canSettleExpense,
      canFinalApprove,
      canAttach,
      canRemoveAttachment,
      canRecall,
      canAct: false,
    },
  };

  return (
    <ActorProvider value={snapshot}>
      <IntlProvider>
        <OnboardingOverlay />
        <LayOut>
          {children}
          <Suspense fallback={null}>
            <GlobalChat />
          </Suspense>
        </LayOut>
      </IntlProvider>
    </ActorProvider>
  );
}
