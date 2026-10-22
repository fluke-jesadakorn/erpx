export type DestructionDocument = {id:string;checksum:string;object_key:string|null;attachment_source_id:string|null;attachment_link_id:string|null};
export type DestructionUpload = {id:string;checksum:string;object_key:string;attachment_source_id:string|null;attachment_link_id:string|null};
export type CanonicalSource = {id:string;objectKey:string;checksum:string;linkIds:string[];ledger:{id:string;objectKey:string;bytes:number;variant:string}[]};
export type LegalDestructionManifest = {documents:DestructionDocument[];uploads:DestructionUpload[];keys:string[];canonicalSources:CanonicalSource[]};

