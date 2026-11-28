import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/commerce-server/public/owner-source-locks';
import {customerPortalSourceReadLocks} from '@erpx/customer-authority-server/adapters/portal-source-locks';

/** Same canonical source order as the original complete Portal owner unit.
 * This declaration performs no query before the fresh request snapshot. */
export function portalAttachmentRequestSourceLocks(database:SqlDatabase){
 return [database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),...commerceCompleteOwnerSourceReadLocks(database),...customerPortalSourceReadLocks(database)];
}
