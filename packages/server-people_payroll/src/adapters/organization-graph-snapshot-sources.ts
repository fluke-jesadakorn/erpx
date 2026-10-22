import 'server-only';

/**
 * Organization's optimistic graph guard retains the complete owned row as JSON.
 * Named projection columns remain stable across historical migration baselines.
 */
export const organizationGraphSnapshotSources = Object.freeze({
  people: 'SELECT person.id,person.company_id,to_jsonb(person) snapshot FROM hr_people person',
  engagements: 'SELECT engagement.id,engagement.company_id,to_jsonb(engagement) snapshot FROM hr_engagements engagement',
} as const);
