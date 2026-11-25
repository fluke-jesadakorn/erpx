import {scopeTuple} from './read-tail-r29-sql';

/** One byte-identical column-projection composer shared by the Sales-owned
 * commercial and final-root owner sources. Columns and relation are owner-local
 * fixed tokens; no caller input enters the rendered statement. */
export const tableRead=(columns:string,table:string)=>`SELECT ${columns} FROM ${table}`;

export const actorScopeTuple=scopeTuple('actor');
/** One parameterized assignment-authority scope-read family. The authority
 * predicate, scope tuple, relation and optional extra guard are owner-local
 * fixed tokens composed by callers; no caller input enters the rendered
 * statement, so each call stays byte-identical to its original literal. */
export const authorityScopeRead=(authority:string,tuple:string,relation:string,extra='')=>`SELECT ${authority}${extra} allowed FROM ${tuple} ${relation}`;
