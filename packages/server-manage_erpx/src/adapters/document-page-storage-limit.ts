import {betaPolicyStorageBytesSql} from './attachments-service-facts';
/** Fixed caller-bound projection of Management's original Company allowance.
 * Embeds the one canonical storage-bytes read so the quota and document page
 * allowance cannot drift. */
export const documentPageStorageLimitSql=`COALESCE((${betaPolicyStorageBytesSql}),9007199254740991)`;
