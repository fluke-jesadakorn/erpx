export type ListPage<T> = { items:T[];nextCursor:string|null;previousCursor:string|null;hasMore:boolean };
export type Cursor = { key:string;id:string;reverse:boolean;scope:string };
export type ListPagePolicy = { scope:unknown;cursor?:string;limit?:number;descending?:boolean };
export function listPageSize(limit?:number) {return Number.isSafeInteger(limit)?Math.max(1,Math.min(limit!,100)):30;}
export function validListCursor(value:unknown,scope:string):value is Cursor {return !!value&&typeof value==="object"&&"scope" in value&&value.scope===scope&&"key" in value&&typeof value.key==="string"&&"id" in value&&typeof value.id==="string"&&"reverse" in value&&typeof value.reverse==="boolean";}
