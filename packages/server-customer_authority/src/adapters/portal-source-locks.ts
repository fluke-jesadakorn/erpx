import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {customerPortalActionSourceReadLocks} from '@erpx/server-customer_360/public/portal-source-locks';
/** Source declarations contain no application, Files service, IAM or query. */
export function customerPortalSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE customer_access_bindings,customer_access_events,customer_access_identities,customer_access_reads IN SHARE ROW EXCLUSIVE MODE'),...customerPortalActionSourceReadLocks(database)];}
