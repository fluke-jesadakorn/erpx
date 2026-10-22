import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function assertion(db:SqlDatabase,id:string,step:string,query:string,args:(string|number|null)[]=[]){return db.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN (${query}) THEN 1 ELSE 0 END)`).bind(id,step,...args);}
