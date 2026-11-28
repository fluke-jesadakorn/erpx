import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {AiProviderSnapshot} from '@erpx/agent-server/domain/providers';
/** Parameterized Agent source-witness projection shared by the settings, connection and
 * revision snapshots. Only alias, table and predicate vary; each bind stays at its call site. */
const agentSourceWitnessSql = (alias: string, table: string, predicate: string) =>
  `SELECT to_jsonb(${alias}) FROM ${table} ${alias} WHERE ${predicate}`;
/** The existing Holding provider selection and immutable connection revision.
 * No lease is claimed for environment credentials, Control or remote policy. */
export function socialAiProviderSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE ai_company_settings,ai_connections,ai_connection_revisions IN SHARE ROW EXCLUSIVE MODE');}
export function socialAiProviderSourceWitness(database:SqlDatabase,snapshot:AiProviderSnapshot){
 return database.prepare(`SELECT jsonb_build_object('settings',(${agentSourceWitnessSql('s','ai_company_settings','s.company_id=?')}),'connection',(${agentSourceWitnessSql('c','ai_connections','c.company_id=? AND c.id=?')}),'revision',(${agentSourceWitnessSql('r','ai_connection_revisions','r.connection_id=? AND r.revision=?')}))::text image`).bind(snapshot.companyId,snapshot.companyId,snapshot.connectionId,snapshot.connectionId,snapshot.revision).first<string>('image');
}
