import {listPageSize,validListCursor,type Cursor,type ListPage,type ListPagePolicy} from "../domain/list-page";
export function createListPageReader<T extends {id:string;page_key:string}>(ports:{hash(value:string):string;decode(cursor:string):unknown;encode(cursor:Cursor):string;invalidCursor():Error;rows(request:{cursor?:Cursor;descending:boolean;limit:number}):Promise<T[]>}) {
 return async function readListPage(options:ListPagePolicy):Promise<ListPage<T>> {
 const size=listPageSize(options.limit),scope=ports.hash(JSON.stringify([options.scope,!!options.descending,size]));let cursor:Cursor|undefined;
 if(options.cursor){try{if(options.cursor.length>4096)throw Error();const parsed=ports.decode(options.cursor);if(!validListCursor(parsed,scope))throw Error();cursor=parsed;}catch{throw ports.invalidCursor();}}
 const reverse=cursor?.reverse??false,descending=Boolean(options.descending)!==reverse;const rows=await ports.rows({cursor,descending,limit:size+1});const overflow=rows.length>size;const items=rows.slice(0,size);if(reverse)items.reverse();
 const encode=(row:T,backwards:boolean)=>ports.encode({key:row.page_key,id:row.id,reverse:backwards,scope});const previousCursor=items.length&&(reverse?overflow:!!cursor)?encode(items[0],true):null;const nextCursor=items.length&&(reverse?!!cursor:overflow)?encode(items[items.length-1],false):null;
 return {items,previousCursor,nextCursor,hasMore:nextCursor!==null};
 };
}
