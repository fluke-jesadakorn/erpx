import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Native Holding acceptance/revision/engineering sources only. Configuration
 * owners retain their normal revision triggers and final release predicate;
 * this lease makes no claim about a separate Control database or provider. */
export async function lockScheduledReleaseSources(database:SqlDatabase){await database.prepare('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_revisions,company_legal_release_revisions,engineering_verification_contexts IN SHARE MODE').run();}
