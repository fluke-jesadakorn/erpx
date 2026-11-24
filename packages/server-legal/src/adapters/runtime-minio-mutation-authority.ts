import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {runtimeMinioPreservationSources} from '@erpx/server-files/public/runtime-minio-preservation-sources';
import {legalFilesAdvisoryLockSql} from './legal-read-tail-r9-sources';
import {legalFilePreservationHoldTargetHead} from './legal-read-tail-r44-sources';
/** The original remote-store hold reads stay on their supplied transaction. */
export function runtimeMinioMutationAuthority(db:SqlDatabase){const files=runtimeMinioPreservationSources(db);return {lock:()=>db.prepare(legalFilesAdvisoryLockSql).run(),preservationReady:files.ready,pendingOwner:files.pendingOwner,held:(key:string)=>db.prepare(`${legalFilePreservationHoldTargetHead('1','t','h','!=')} AND (p.object_key=? OR (p.source_type='attachment' AND ${files.preservedDerivativeObject}))`).bind(key,key).first()};}
