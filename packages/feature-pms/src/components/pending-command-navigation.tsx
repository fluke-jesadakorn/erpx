"use client";
import { createContext, useCallback, useContext, useLayoutEffect, useMemo } from 'react';
import { destinationHref, useRouter } from '@erpx/feature-shared/lib/destination-router';
import { useProductNavigationScope } from '@erpx/workplace/components/product-link-context';
import { isProductHandoffHref } from '@erpx/feature-work_projection/lib/product-navigation';
import { installPmsPendingNavigation, pmsPendingDepartureAllowed } from '@erpx/feature-pms/lib/pending-navigation';

type Router = ReturnType<typeof useRouter>;
const noLocalDraftPending = () => false;
export const PmsPendingRouterContext = createContext<Router | null>(null);

export function usePmsPendingNavigation(
  nativeRouter: Router,
  pending: () => boolean,
  language: 'en' | 'th',
  localDraftPending: () => boolean = noLocalDraftPending,
) {
  const scope = useProductNavigationScope();
  const confirm = useCallback(() => {
    const hasDraft = localDraftPending();
    const hasCommand = pending();
    return window.confirm(language === 'th'
      ? hasDraft && hasCommand
        ? 'คำสั่งโรงแรมนี้อาจบันทึกแล้ว และฉบับร่างการปิดงานหรือไฟล์แนบที่รอกู้คืนอาจสูญหายหากออกจากหน้านี้ ต้องการออกจากหน้านี้และปิดการกู้คืนคำขอหรือไม่?'
        : hasDraft
          ? 'ฉบับร่างการปิดงานหรือไฟล์แนบที่รอกู้คืนอาจสูญหายหากออกจากหน้านี้ ต้องการออกจากหน้านี้หรือไม่?'
          : 'คำสั่งโรงแรมนี้อาจบันทึกแล้ว ให้อยู่หน้านี้เพื่อลองคำขอเดิมอีกครั้ง ออกจากหน้านี้และปิดการกู้คืนคำขอหรือไม่?'
      : hasDraft && hasCommand
        ? 'This hotel action may already be saved, and the task-completion draft or attachment retry file may be lost if you leave. Leave this view and close its recovery?'
        : hasDraft
          ? 'The task-completion draft or attachment retry file may be lost if you leave this view. Leave this view?'
          : 'This hotel action may already be saved. Stay here to retry the original request. Leave this view and close its recovery?');
  }, [language, localDraftPending, pending]);
  useLayoutEffect(() => installPmsPendingNavigation(pending, confirm, localDraftPending), [pending, confirm, localDraftPending]);
  return useMemo(() => ({ ...nativeRouter,
    push: (href: string, options?: Parameters<Router['push']>[1]) => {
      const target = destinationHref(href, scope);
      if ((scope.audience && isProductHandoffHref(target)) || pmsPendingDepartureAllowed(pending, target, confirm, localDraftPending)) nativeRouter.push(href, options);
    },
    replace: (href: string, options?: Parameters<Router['replace']>[1]) => {
      const target = destinationHref(href, scope);
      if ((scope.audience && isProductHandoffHref(target)) || pmsPendingDepartureAllowed(pending, target, confirm, localDraftPending)) nativeRouter.replace(href, options);
    },
  }), [nativeRouter, scope, pending, confirm, localDraftPending]);
}

/** List/query controls use the same mounted owner's guard. Other consumers of
 * the shared pagination retain their original router without an owner provider. */
export function usePmsOwnerRouter() {
  const nativeRouter = useRouter(), ownerRouter = useContext(PmsPendingRouterContext);
  return ownerRouter ?? nativeRouter;
}
