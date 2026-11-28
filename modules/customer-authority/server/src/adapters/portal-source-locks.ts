import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {customerPortalActionSourceReadLocks} from '@erpx/customer-360-server/public/portal-source-locks';
/** Source declarations contain no application, Files service, IAM or query. */
export function customerPortalSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE customer_access_bindings,customer_access_events,customer_access_identities,customer_access_reads IN SHARE ROW EXCLUSIVE MODE'),...customerPortalActionSourceReadLocks(database)];}
