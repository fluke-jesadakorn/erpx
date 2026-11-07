import type {PoolClient} from 'pg';

type ForeignKey={oid:string;parent_oid:string;child_schema:string;child_table:string;parent_schema:string;parent_table:string;child_kind:string;parent_kind:string;child_rls:boolean;parent_rls:boolean;child_select:boolean;parent_select:boolean;validated:boolean;inherited:boolean;match_type:string;enabled_triggers:number;bad_triggers:number;child_columns:string[];parent_columns:string[];key_length:number;ordinary_operators:boolean};
const quote=(name:string)=>{if(!name||name.includes('\0'))throw Error('FOREIGN_KEY_IDENTIFIER_INVALID');return `"${name.replaceAll('"','""')}"`;};
const relation=(schema:string,name:string)=>`${quote(schema)}.${quote(name)}`;
/** PostgreSQL equivalent of the seed's SQLite foreign_key_check. Caller owns a
 * repeatable-read transaction; all constraints, privileges and actual rows are checked. */
export async function auditDevContainerForeignKeys(client:PoolClient):Promise<{constraint:string}[]>{
 const state=await client.query<{schema:string;replication_role:string}>("SELECT current_schema() AS schema,current_setting('session_replication_role') AS replication_role");const schema=state.rows[0]?.schema;if(!schema||state.rows[0].replication_role!=='origin')throw Error('FOREIGN_KEY_AUDIT_CONTEXT_INVALID');
 const found=await client.query<ForeignKey>(`SELECT c.oid::text AS oid,c.conparentid::text AS parent_oid,cn.nspname AS child_schema,cr.relname AS child_table,pn.nspname AS parent_schema,pr.relname AS parent_table,cr.relkind AS child_kind,pr.relkind AS parent_kind,(cr.relrowsecurity OR cr.relforcerowsecurity) AS child_rls,(pr.relrowsecurity OR pr.relforcerowsecurity) AS parent_rls,has_table_privilege(cr.oid,'SELECT') AS child_select,has_table_privilege(pr.oid,'SELECT') AS parent_select,c.convalidated AS validated,(c.conparentid<>0) AS inherited,c.confmatchtype AS match_type,cardinality(c.conkey)::int AS key_length,
 (SELECT count(*)::int FROM pg_trigger t WHERE t.tgconstraint=c.oid AND t.tgenabled IN ('O','A')) AS enabled_triggers,(SELECT count(*)::int FROM pg_trigger t WHERE t.tgconstraint=c.oid AND t.tgenabled NOT IN ('O','A')) AS bad_triggers,
 array_agg(ca.attname::text ORDER BY k.ord) AS child_columns,array_agg(pa.attname::text ORDER BY k.ord) AS parent_columns,bool_and(op.oprname='=' AND ons.nspname='pg_catalog') AS ordinary_operators
 FROM pg_constraint c JOIN pg_namespace cn ON cn.oid=c.connamespace JOIN pg_class cr ON cr.oid=c.conrelid JOIN pg_class pr ON pr.oid=c.confrelid JOIN pg_namespace pn ON pn.oid=pr.relnamespace
 CROSS JOIN LATERAL unnest(c.conkey,c.confkey,c.conpfeqop) WITH ORDINALITY AS k(child_att,parent_att,eq_op,ord)
 JOIN pg_attribute ca ON ca.attrelid=cr.oid AND ca.attnum=k.child_att AND NOT ca.attisdropped JOIN pg_attribute pa ON pa.attrelid=pr.oid AND pa.attnum=k.parent_att AND NOT pa.attisdropped
 JOIN pg_operator op ON op.oid=k.eq_op JOIN pg_namespace ons ON ons.oid=op.oprnamespace
 WHERE c.contype='f' AND cn.nspname=$1 GROUP BY c.oid,cn.nspname,cr.oid,pn.nspname,pr.oid ORDER BY c.oid`,[schema]);
 const total=await client.query<{count:number}>("SELECT count(*)::int AS count FROM pg_constraint c JOIN pg_namespace n ON n.oid=c.connamespace WHERE c.contype='f' AND n.nspname=$1",[schema]);if(!found.rows.length||found.rows.length>10000||total.rows[0]?.count!==found.rows.length)throw Error('FOREIGN_KEY_AUDIT_SCOPE_INVALID');
 const byId=new Map(found.rows.map(fk=>[fk.oid,fk]));
 const ids=new Set(byId.keys());
 for(const fk of found.rows){if(!/^\d+$/.test(fk.parent_oid)||(fk.inherited && !ids.has(fk.parent_oid))||(!fk.inherited && fk.parent_oid!=='0'))throw Error('FOREIGN_KEY_AUDIT_SCOPE_INVALID');if(!/^\d+$/.test(fk.oid)||!['r','p'].includes(fk.child_kind)||!['r','p'].includes(fk.parent_kind)||fk.child_rls||fk.parent_rls||!fk.child_select||!fk.parent_select||!fk.validated||fk.bad_triggers!==0||fk.enabled_triggers<(fk.inherited?2:4)||!['s','f'].includes(fk.match_type)||!fk.ordinary_operators||!fk.child_columns.length||fk.child_columns.length>32||fk.child_columns.length!==fk.parent_columns.length||fk.child_columns.length!==fk.key_length)throw Error('FOREIGN_KEY_AUDIT_UNSUPPORTED_OR_UNSAFE');}
 // A derived constraint must lead to an included logical root; otherwise a
 // cross-schema or broken ancestry could silently escape the row audit.
 for(const fk of found.rows){let current=fk;const visited=new Set<string>();while(current.inherited){if(visited.has(current.oid))throw Error('FOREIGN_KEY_AUDIT_SCOPE_INVALID');visited.add(current.oid);const parent=byId.get(current.parent_oid);if(!parent)throw Error('FOREIGN_KEY_AUDIT_SCOPE_INVALID');current=parent;}}
 // Lock every referenced relation before scanning so DDL cannot change the
 // audited tables while this transaction inspects their rows.
 const descendants=await client.query<{unsafe:boolean}>(`WITH RECURSIVE selected(oid) AS (SELECT c.conrelid FROM pg_constraint c JOIN pg_namespace n ON n.oid=c.connamespace WHERE c.contype='f' AND n.nspname=$1 UNION SELECT c.confrelid FROM pg_constraint c JOIN pg_namespace n ON n.oid=c.connamespace WHERE c.contype='f' AND n.nspname=$1 UNION SELECT i.inhrelid FROM pg_inherits i JOIN selected s ON s.oid=i.inhparent) SELECT EXISTS(SELECT 1 FROM selected s JOIN pg_class r ON r.oid=s.oid WHERE r.relrowsecurity OR r.relforcerowsecurity OR NOT has_table_privilege(r.oid,'SELECT')) AS unsafe`,[schema]);if(descendants.rows[0]?.unsafe!==false)throw Error('FOREIGN_KEY_AUDIT_UNSUPPORTED_OR_UNSAFE');
 const relations=[...new Set(found.rows.flatMap(fk=>[relation(fk.child_schema,fk.child_table),relation(fk.parent_schema,fk.parent_table)]))].sort();
 for(let i=0;i<relations.length;i+=100)await client.query(`LOCK TABLE ${relations.slice(i,i+100).join(',')} IN ACCESS SHARE MODE`);
 // Inherited foreign keys against referenced-table partitions are catalog
 // enforcement details: each partition contains only part of the parent keyspace.
 // Scan the root constraint against its complete referenced relation instead.
 const roots=found.rows.filter(fk=>!fk.inherited);
 const violations:{constraint:string}[]=[];
 for(let offset=0;offset<roots.length;offset+=40){const batch=roots.slice(offset,offset+40),queries=batch.map(fk=>{
  const columns=fk.child_columns.map(column=>`c.${quote(column)}`),allPresent=columns.map(column=>`${column} IS NOT NULL`).join(' AND '),anyPresent=columns.map(column=>`${column} IS NOT NULL`).join(' OR '),anyNull=columns.map(column=>`${column} IS NULL`).join(' OR '),equal=fk.parent_columns.map((column,i)=>`p.${quote(column)} OPERATOR(pg_catalog.=) ${columns[i]}`).join(' AND ');
  const missing=`(${allPresent}) AND NOT EXISTS (SELECT 1 FROM ${relation(fk.parent_schema,fk.parent_table)} p WHERE ${equal})`,invalid=fk.match_type==='f'?`((${anyNull}) AND (${anyPresent})) OR (${missing})`:missing;
  return `SELECT '${fk.oid}' AS constraint WHERE EXISTS (SELECT 1 FROM ${relation(fk.child_schema,fk.child_table)} c WHERE ${invalid})`;
 });const rows=await client.query<{constraint:string}>(queries.join(' UNION ALL '));violations.push(...rows.rows);}
 return violations;
}
