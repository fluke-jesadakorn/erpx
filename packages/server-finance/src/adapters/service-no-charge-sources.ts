import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Absence means no terminal proof, never a permissive zero. The original
 * old schema remains usable before the additive migration is reviewed. */
export async function serviceNoChargeSchemaAvailable(database:SqlDatabase){return !!await database.prepare("SELECT to_regclass('public.finance_service_no_charge_completed') IS NOT NULL present").first<boolean>('present');}
// A valid new decision cannot commit without updating its exact Invoice.
// Callers already hold finance_invoices SHARE before reading the view; that
// fences decision insertion and migration DDL without a catalogue SELECT in
// the query-free source-lease phase.
export const serviceNoChargeCompletedSources='SELECT id,company_id,confirmation_id,command_id,confirmed_by,confirmed_at,reason,version FROM finance_service_no_charge_completed';
export async function serviceNoChargeTerminalPredicate(database:SqlDatabase,alias:string){
 if(!/^[a-z][a-z0-9_]*$/.test(alias))throw Error('CLOSED_SERVICE_INVOICE_ALIAS_REQUIRED');
 return await serviceNoChargeSchemaAvailable(database)?`(${alias}.status='no_charge_confirmed' AND EXISTS(SELECT 1 FROM (${serviceNoChargeCompletedSources}) no_charge WHERE no_charge.id=${alias}.id AND no_charge.company_id=${alias}.company_id))`:'FALSE';
}
