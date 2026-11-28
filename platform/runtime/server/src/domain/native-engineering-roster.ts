/** Exact local engineering leases; the roster carries no database credentials. */
export const nativeEngineeringBindings=['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'] as const;
export type NativeEngineeringEntry={binding:typeof nativeEngineeringBindings[number];token:string;oid:number};
export type NativeEngineeringRoster={version:1;run:string;sourceFingerprint:string;fileManifestSha256:string;postmasterStartedMs:number;entries:NativeEngineeringEntry[]};
export function validateNativeEngineeringRoster(value:unknown,run:string,source:string,manifest:string):value is NativeEngineeringRoster {
 if(!value||typeof value!=='object'||Array.isArray(value))return false;
 const r=value as NativeEngineeringRoster;
 if(r.version!==1||r.run!==run||r.sourceFingerprint!==source||r.fileManifestSha256!==manifest||!Number.isSafeInteger(r.postmasterStartedMs)||r.postmasterStartedMs<=0||!Array.isArray(r.entries)||r.entries.length!==8)return false;
 const tokens=new Set<string>(),oids=new Set<number>();
 for(let i=0;i<nativeEngineeringBindings.length;i++){const e=r.entries[i];if(!e||e.binding!==nativeEngineeringBindings[i]||!/^[a-f0-9]{32}$/.test(e.token)||!Number.isSafeInteger(e.oid)||e.oid<=0||e.oid>4294967295||tokens.has(e.token)||oids.has(e.oid))return false;tokens.add(e.token);oids.add(e.oid);}
 return true;
}
