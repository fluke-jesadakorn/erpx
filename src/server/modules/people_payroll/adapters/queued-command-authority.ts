import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** The worker/lifecycle facts used by native IAM remain shared foundation. */
export async function lockQueuedPeopleAuthority(database:SqlDatabase){await database.prepare('LOCK TABLE hr_people,hr_engagements,people_access_cutoffs,people_lifecycle_cases,people_lifecycle_grants,people_package_revisions,people_applied_packages,people_provision_operations,people_provision_grants IN SHARE MODE').run();}
