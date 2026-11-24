
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {attachmentSetPurposeSql} from './read-tail-dedupe-sql';
export function productionCommandSources(db:SqlDatabase){return {async attachmentSets(companyId:string,attachmentSetIds:string[]){const placeholders=attachmentSetIds.map(()=>"?").join(",");return db.prepare(attachmentSetPurposeSql(placeholders)).bind(companyId, ...attachmentSetIds).all<{id:string;purpose_key:string;component_kind:string;component_ref:string|null}>();}};}
