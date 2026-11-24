import 'server-only';

/** Owner-local point-existence head (r44). The two admitted Marketing probes
 * share one byte-identical point-existence head: a caller passes an owner-owned
 * source fragment and its fixed WHERE tail, plus the admitted FROM gap. Each
 * probe emits its original statement byte for byte and keeps its bind order;
 * no caller-authored SQL crosses this boundary. FOR SHARE locks,
 * command_receipts settlement and native read ports stay untouched. */
export const marketingPointExistsSql = (
  source: string,
  where: string,
  gap: '' | ' ' = ' ',
): string => `SELECT 1 FROM${gap}${source} WHERE ${where}`;
