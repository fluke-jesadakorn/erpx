import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
// Original transaction witnesses keep one identity for the explicit native DB unit.
export type Source={kind:'staff_task'|'staff_project'|'pms_record';id:string};
export type Handover={key:string;id:string;sources:Source[]};
export type SocialUnit={providerLeased:boolean;imagesLeased:boolean;companyId:string;actorKey:string;checks:Array<()=>Promise<void>>;images:Array<{read:()=>Promise<string|null>;completed:string|null}>;handover?:Handover};
export const units=new WeakMap<SqlDatabase,SocialUnit>();
