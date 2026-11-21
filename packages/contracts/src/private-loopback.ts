/** One loopback port policy for native transports.
 *
 * 3101 is the documented private production runtime. It is admitted only by an
 * explicit production configuration; development keeps rejecting it so a dev
 * app cannot accidentally call the installed production service. The other
 * reserved ports (dev gateway, staging, PostgreSQL) stay closed.
 */
const reserved = new Set([3000, 3001, 3100, 3103, 5432, 5433, 5435, 8080]);
export function allowedPrivateLoopbackPort(port: number, production: boolean): boolean {
  return (
    Number.isInteger(port) &&
    port >= 1 &&
    port <= 65535 &&
    !(port >= 3200 && port <= 3299) &&
    !reserved.has(port) &&
    (production || port !== 3101)
  );
}
