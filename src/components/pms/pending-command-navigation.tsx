"use client";
import { createContext, useCallback, useContext, useLayoutEffect, useMemo } from 'react';
import { destinationHref, useRouter } from '@/lib/shared/destination-router';
import { useProductNavigationScope } from '@/components/workplace/product-link-context';
import { isProductHandoffHref } from '@/lib/work_projection/product-navigation';
import { installPmsPendingNavigation, pmsPendingDepartureAllowed } from '@/lib/pms/pending-navigation';

type Router = ReturnType<typeof useRouter>;
export const PmsPendingRouterContext = createContext<Router | null>(null);

export function usePmsPendingNavigation(nativeRouter: Router, pending: () => boolean, language: 'en' | 'th') {
  const scope = useProductNavigationScope();
  const confirm = useCallback(() => window.confirm(language === 'th'
    ? 'คำสั่งโรงแรมนี้อาจบันทึกแล้ว ให้อยู่หน้านี้เพื่อลองคำขอเดิมอีกครั้ง ออกจากหน้านี้และปิดการกู้คืนคำขอหรือไม่?'
    : 'This hotel action may already be saved. Stay here to retry the original request. Leave this view and close its recovery?'), [language]);
  useLayoutEffect(() => installPmsPendingNavigation(pending, confirm), [pending, confirm]);
  return useMemo(() => ({ ...nativeRouter,
    push: (href: string, options?: Parameters<Router['push']>[1]) => {
      const target = destinationHref(href, scope);
      if ((scope.audience && isProductHandoffHref(target)) || pmsPendingDepartureAllowed(pending, target, confirm)) nativeRouter.push(href, options);
    },
    replace: (href: string, options?: Parameters<Router['replace']>[1]) => {
      const target = destinationHref(href, scope);
      if ((scope.audience && isProductHandoffHref(target)) || pmsPendingDepartureAllowed(pending, target, confirm)) nativeRouter.replace(href, options);
    },
  }), [nativeRouter, scope, pending, confirm]);
}

/** List/query controls use the same mounted owner's guard. Other consumers of
 * the shared pagination retain their original router without an owner provider. */
export function usePmsOwnerRouter() {
  const nativeRouter = useRouter(), ownerRouter = useContext(PmsPendingRouterContext);
  return ownerRouter ?? nativeRouter;
}
