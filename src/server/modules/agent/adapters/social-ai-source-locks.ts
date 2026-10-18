import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {AiProviderSnapshot} from '../domain/providers';
/** The existing Holding provider selection and immutable connection revision.
 * No lease is claimed for environment credentials, Control or remote policy. */
export function socialAiProviderSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE ai_company_settings,ai_connections,ai_connection_revisions IN SHARE ROW EXCLUSIVE MODE');}
export function socialAiProviderSourceWitness(database:SqlDatabase,snapshot:AiProviderSnapshot){
 return database.prepare(`SELECT jsonb_build_object('settings',(SELECT to_jsonb(s) FROM ai_company_settings s WHERE s.company_id=?),'connection',(SELECT to_jsonb(c) FROM ai_connections c WHERE c.company_id=? AND c.id=?),'revision',(SELECT to_jsonb(r) FROM ai_connection_revisions r WHERE r.connection_id=? AND r.revision=?))::text image`).bind(snapshot.companyId,snapshot.companyId,snapshot.connectionId,snapshot.connectionId,snapshot.revision).first<string>('image');
}
