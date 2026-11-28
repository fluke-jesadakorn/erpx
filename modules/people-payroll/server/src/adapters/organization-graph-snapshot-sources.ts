import {ppRd47GraphSnapshotProjection} from './pp-read-tail-r47-sources';
import 'server-only';

/**
 * Organization's optimistic graph guard retains the complete owned row as JSON.
 * Named projection columns remain stable across historical migration baselines.
 */
export const organizationGraphSnapshotSources = Object.freeze({
  people: ppRd47GraphSnapshotProjection('person','hr_people'),
  engagements: ppRd47GraphSnapshotProjection('engagement','hr_engagements'),
} as const);
