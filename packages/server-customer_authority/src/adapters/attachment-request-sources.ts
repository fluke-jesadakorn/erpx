import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/server-commerce/public/owner-source-locks';
import {customerPortalSourceReadLocks} from '@erpx/server-customer_authority/adapters/portal-source-locks';

/** Same canonical source order as the original complete Portal owner unit.
 * This declaration performs no query before the fresh request snapshot. */
export function portalAttachmentRequestSourceLocks(database:SqlDatabase){
 return [database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),...commerceCompleteOwnerSourceReadLocks(database),...customerPortalSourceReadLocks(database)];
}
