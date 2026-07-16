import { Suspense } from 'react';
import { redirect } from 'next/navigation';
import { cookies } from 'next/headers';
import { verifySession } from '@/server/sessionToken';
import { matchPerm } from '@/perm/server';
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
  const perms = profile.permissions;

  const canSeeGlLines = matchPerm(perms, 'finance:gl:view::allow');
  const canPostAccrual = matchPerm(perms, 'finance:gl:post::allow');
  const canPostSettlement = matchPerm(perms, 'finance:gl:post::allow');
  const canConfirmGl = matchPerm(perms, 'finance:gl:confirm::allow');
  const canSettleExpense = matchPerm(perms, 'finance:expense:settle::allow');
  const canFinalApprove = matchPerm(perms, 'finance:expense:approve::allow');
  const canAttach = matchPerm(perms, 'finance:expense:create::allow');
  const canRemoveAttachment = profile.role_name === 'cfo' || profile.role_name === 'ceo' || profile.role_name === 'admin';
  const canRecall = profile.role_name === 'cfo' || profile.role_name === 'ceo' || profile.role_name === 'finance';

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