import "server-only";
import contract from "./read-head-sources.json" with { type: "json" };

/** Native-owned read-tail head contract. The six reusable renderers below
 * substitute only owner-fixed fragments into these frozen templates; no caller
 * supplies SQL and no statement token is carried in the TypeScript read tail.
 * The native owner binds the same JSON with `include_str!`. */
export const heads = contract as {
  readonly projectionHead: string;
  readonly existsGuard: string;
  readonly subqueryHead: string;
  readonly aliasRows: string;
  readonly settlementRef: string;
  readonly recordImage: string;
};

/** Render one frozen contract template with literal substitution. A function
 * replacement keeps `$` sequences in caller fragments from being interpreted as
 * replacement patterns; each placeholder occurs once. */
export const renderHead = (template: string, values: Record<string, string>): string =>
  Object.entries(values).reduce(
    (sql, [key, value]) => sql.replace(`{${key}}`, () => value),
    template,
  );
