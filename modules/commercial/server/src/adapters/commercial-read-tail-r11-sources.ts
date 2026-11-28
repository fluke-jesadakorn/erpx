import "server-only";
import { packageSourceSelect } from "./commercial-read-tail-r27-sources";
/** Commercial read-tail r11 parameterized package-source projection family.
 * The head is now owned once by the owner-local r27 package-source renderer;
 * this builder delegates and still renders both original history-source
 * projections byte-identically: the grants source and the retained-history
 * source share the same `package_id,package_version` projection and Company
 * predicate shape. Every caller keeps its own bind/prepare and its exports.
 * Arguments are code-owned source/predicate fragments only, never request
 * input. The r1-r10 helpers, advisory/row locks and native owner statement
 * kinds stay untouched. Pure read-only projection: no writes, no row locks and
 * no advisory lock. */
export function packageSourceProjection(source: string, company: string, predicate: string) {
  return packageSourceSelect("package_id,package_version", source, ` WHERE company_id=${company} AND ${predicate}`);
}
