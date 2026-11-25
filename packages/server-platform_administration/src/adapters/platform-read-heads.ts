/** r22: native-owned static read/probe heads shared by the owner's remaining
 * TypeScript reads. The statement bytes live in the Rust-owned contract
 * `platform-read-head-sources.json`; every call site still prepares and
 * executes its own statement in its supplied transaction with its own alias,
 * column/WHERE tail and binds. Row locks, advisory locks, the pg_catalog
 * identity fence and the native routes are left untouched. */
import contract from "./platform-read-head-sources.json" with { type: "json" };

const heads = contract as {
  readonly selectOne: string;
  readonly selectOneFromWhere: string;
  readonly selectProjectionFrom: string;
  readonly selectRowByPredicate: string;
  readonly recordsUsageProjection: string;
  readonly workforceScopeProbe: string;
  readonly databaseIdentityRead: string;
};

export const platformReadHeads = heads;

/** Substitute owner-controlled fragments into one frozen head. Values are
 * inserted literally, so SQL fragments containing `$` are never reinterpreted
 * as replacement patterns. */
export function fillReadHead(template: string, values: Record<string, string>): string {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error("PLATFORM_READ_HEAD_CONTRACT_INVALID");
    rendered = rendered.split(token).join(value);
  }
  return rendered;
}
