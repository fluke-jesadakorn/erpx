"use client";
import { contextIsSwitching, registerContextSwitchBlocker, registerUnsavedWork } from "@/lib/work_projection/context-switch";

const recordTail = /\/records\/(?:pms_record|pms_entry|pms_hotel_sources|pms_hotel_record)\/[^/]+\/overview\/?$/;
const overlayKeys = ['detailKind', 'detailId', 'detailMode', 'detailArgs', 'detailPage', 'returnTo', 'record'];
const recordKinds = new Set(['pms_record', 'pms_entry', 'pms_hotel_sources', 'pms_hotel_record']);

/** Record drawers retain the same mounted command owner. All business selection
 * and list query parameters remain part of the view identity. Hash links do not
 * replace that owner. This is navigation policy, never command authority. */
export function samePmsCommandView(currentHref: string, nextHref: string) {
  const current = new URL(currentHref), next = new URL(nextHref, current);
  if (current.origin !== next.origin) return false;
  const overlay = recordTail.test(current.pathname) || recordTail.test(next.pathname) ||
    recordKinds.has(current.searchParams.get('detailKind') ?? '') || recordKinds.has(next.searchParams.get('detailKind') ?? '');
  if (current.pathname.replace(recordTail, '') !== next.pathname.replace(recordTail, '')) return false;
  if (overlay) for (const key of overlayKeys) { current.searchParams.delete(key); next.searchParams.delete(key); }
  current.searchParams.sort(); next.searchParams.sort();
  return current.searchParams.toString() === next.searchParams.toString();
}

export function pmsPendingDepartureAllowed(
  pending: () => boolean,
  nextHref: string,
  confirm: () => boolean,
  localDraftPending: () => boolean = () => false,
) {
  // An unresolved native command can only be retried with the exact request
  // held by this mounted owner. Keep that owner mounted until it resolves.
  // Local post-ack drafts can still be explicitly discarded after confirmation.
  if (samePmsCommandView(window.location.href, nextHref)) return true;
  return !pending() && (!localDraftPending() || confirm());
}

/** Protect only this mounted owner's unresolved/in-flight command, even when
 * its action has no editable fields. Never writes history or rewrites Next's
 * private router state. Browsers without a cancelable Navigation API traversal
 * retain beforeunload protection but cannot safely cancel same-document Back. */
export function installPmsPendingNavigation(
  pending: () => boolean,
  confirm: () => boolean,
  localDraftPending: () => boolean = () => false,
) {
  const ownerWorkPending = () => pending() || localDraftPending();
  const unregister = registerUnsavedWork(ownerWorkPending);
  // A lost acknowledgement can only be retried with the exact body, key and
  // original work identity held by this mounted owner. Context changes remount
  // that owner, so a generic discard confirmation must not drop its recovery.
  const unregisterContextBlocker = registerContextSwitchBlocker(() => pending()
    ? 'Resolve the original hotel command before changing company, account, or role.'
    : null);
  const unload = (event: BeforeUnloadEvent) => {
    if (ownerWorkPending() && !contextIsSwitching()) { event.preventDefault(); event.returnValue = ''; }
  };
  const click = (event: MouseEvent) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const anchor = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>('a[href]') : null;
    if (!anchor || anchor.hasAttribute('data-product-switch') || anchor.hasAttribute('download') ||
      anchor.target && anchor.target !== '_self') return;
    const destination = new URL(anchor.href, window.location.href);
    // Cross-product/native external navigation retains its existing global or
    // beforeunload guard. Only ordinary same-origin owner departures are local.
    if (destination.origin !== window.location.origin || !['http:', 'https:'].includes(destination.protocol)) return;
    if (!pmsPendingDepartureAllowed(pending, destination.href, confirm, localDraftPending)) { event.preventDefault(); event.stopImmediatePropagation(); }
  };
  const traverse = (event: Event) => {
    const navigation = event as Event & { navigationType?: string; destination?: { url?: string; sameDocument?: boolean } };
    if (navigation.navigationType !== 'traverse' || event.defaultPrevented || !event.cancelable || navigation.destination?.sameDocument !== true || !navigation.destination.url) return;
    if (!pmsPendingDepartureAllowed(pending, navigation.destination.url, confirm, localDraftPending)) event.preventDefault();
  };
  const navigation = (window as Window & { navigation?: EventTarget }).navigation;
  window.addEventListener('beforeunload', unload);
  document.addEventListener('click', click, true);
  navigation?.addEventListener('navigate', traverse);
  return () => {
    unregister();
    unregisterContextBlocker();
    window.removeEventListener('beforeunload', unload);
    document.removeEventListener('click', click, true);
    navigation?.removeEventListener('navigate', traverse);
  };
}
