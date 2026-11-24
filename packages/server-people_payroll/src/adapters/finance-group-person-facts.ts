import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import { ppRd28PersonLegalNamesSql } from './pp-read-tail-r28-sources';
export function groupReportingPersonNames(database:SqlDatabase,companyId:string,ids:string[]){return database.prepare(ppRd28PersonLegalNamesSql(ids.map(()=>"?").join(","))).bind(companyId,...ids).all<{user_id:string;legal_name:string}>();}
