import type {FileObjectMetadata as FileMetadata} from '../file-object';
export type OrphanReference={table:string;column:string;rows:number};
export type OrphanProof={bytes:number;objectSha256:string;manifestSha256:string};
export type OrphanRecord={id:string;object_key:string;object_sha256:string;manifest_sha256:string;bytes:number;preserved_directory:string};
export const orphanDiagnosticTables=Object.freeze(['attachment_orphan_preservations','attachment_orphan_preservation_events','attachment_reconciliation_runs']);
export function preservationMetadataInput(head:FileMetadata){return JSON.stringify({key:head.key,size:head.size,etag:head.etag,uploaded:head.uploaded.toISOString(),httpMetadata:head.httpMetadata,customMetadata:head.customMetadata});}
export function orphanReferenceProofInput(key:string){return JSON.stringify({key,references:[],diagnosticTablesExcluded:[...orphanDiagnosticTables].sort()});}
export function validatePreservationGate(gates:{local:boolean;frozen:boolean;enabled:boolean}){if(!gates.local||!gates.frozen||gates.enabled)throw Error('PRESERVATION_REQUIRES_LOCAL_FROZEN_GATE_DISABLED');}
export function validateReferenceIdentifier(table:string,column:string){if(!/^[a-z_][a-z0-9_]*$/i.test(table)||!/^[a-z_][a-z0-9_]*$/i.test(column))throw Error('UNSAFE_PROVENANCE_IDENTIFIER');}
export function validateOrphanManifest(manifest:{key:string;version:string;size:number;etag:string},key:string){if(manifest.key!==key||!/^[a-f0-9-]{36}$/.test(manifest.version)||!/^[a-f0-9]{64}$/.test(manifest.etag)||!Number.isSafeInteger(manifest.size))throw Error('ORPHAN_MANIFEST_INVALID');}
