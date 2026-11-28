import "server-only";
import {pointRead} from './point-read';
/** One byte-identical single-key read family. Only owner-local fixed relations
 * and column names are accepted; no caller input enters the rendered statement. */
export const byKey = (table: string, column: string) => pointRead('*', table, column);
