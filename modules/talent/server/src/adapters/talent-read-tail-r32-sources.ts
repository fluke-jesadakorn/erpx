import "server-only";
import {selectProjection} from './talent-read-tail-r37-sources';
/** Owner-local source-row family (round 32). The projection and FROM source
 * are parameterized; emitted SQL stays byte-identical to the former per-adapter
 * inline copies in the metric and record purchase adapters. Text only: nothing
 * is cached or executed here. */
export const talentSourceRows = (cols: string, from: string) => `${selectProjection(cols)} FROM ${from}`;
