import type {PortalPurchaseControls} from '@erpx/feature-customer_authority/lib/portal-command-attempt';
import type {QuickAccessPreference} from '@erpx/feature-work_projection/lib/quick-access';
import type {PortalSupportThread} from '@erpx/server-customer_authority/public/portal-support';

export type PortalSupportThreadDisplay = Omit<PortalSupportThread, 'requestedAt'>;

export function portalPurchaseDisplay(purchase: PortalPurchaseControls | undefined): PortalPurchaseControls | undefined {
  return purchase && {
    settleExisting: purchase.settleExisting,
    history: purchase.history,
    revision: purchase.revision,
    identityRevision: purchase.identityRevision,
  };
}

export function portalActionsDisplay(actions: readonly Record<string, unknown>[]): {action: 'request_support' | 'acknowledge_delivery'}[] {
  return actions.flatMap(({action}) => action === 'request_support' || action === 'acknowledge_delivery' ? [{action}] : []);
}

export function portalSharedFilesDisplay(files: readonly {linkId: string; label: string; previewAvailable: true}[]) {
  return files.map(file => ({linkId: file.linkId, label: file.label, previewAvailable: file.previewAvailable}));
}

export function portalSupportThreadDisplay(thread: PortalSupportThread): PortalSupportThreadDisplay {
  return {
    requestId: thread.requestId,
    note: thread.note,
    response: thread.response && {
      id: thread.response.id,
      version: thread.response.version,
      message: thread.response.message,
      publishedAt: thread.response.publishedAt,
    },
    acknowledgedAt: thread.acknowledgedAt,
    closedAt: thread.closedAt,
  };
}

export function portalPreferenceDisplay(preference: QuickAccessPreference): QuickAccessPreference {
  return {pinnedIds: preference.pinnedIds === null ? null : [...preference.pinnedIds], version: preference.version};
}
