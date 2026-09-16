import type { SqlDatabase, SqlStatement } from "./runtime/types";
import type { PoolClient, Pool } from "pg";
const jsonColumns = new Set([
  "hierarchy_json",
  "config",
  "payload",
  "evidence",
  "result",
  "definition",
  "permissions_json",
  "grantable_json",
  "scopes_json",
  "groups_json",
  "users_json",
  "snapshot_json",
]);
export function decodeIam<T>(value: T): T {
  if (Array.isArray(value)) return value.map((v) => decodeIam(v)) as T;
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value).map(([key, v]) => [
        key,
        jsonColumns.has(key) && typeof v === "string"
          ? JSON.parse(v)
          : key === "rows" || key === "results"
            ? decodeIam(v)
            : v,
      ]),
    ) as T;
  return value;
}
const wrapped = new WeakMap<object, object>();
function clientProxy<T extends Pool | PoolClient>(client: T): T {
  return new Proxy(client, {
    get(target, key) {
      if (key === "query")
        return async (...args: unknown[]) =>
          decodeIam(await Reflect.apply(target.query, target, args));
      if (key === "connect" && "connect" in target)
        return async () => clientProxy(await (target as Pool).connect());
      const v = Reflect.get(target, key);
      return typeof v === "function" ? v.bind(target) : v;
    },
  });
}
/** IAM uses native JSON values without changing the legacy driver's JSON contract. */
export function iamDb<T extends SqlDatabase>(db: T): T {
  const old = wrapped.get(db);
  if (old) return old as T;
  function statement(s: SqlStatement): SqlStatement {
    return new Proxy(s, {
      get(target, key) {
        if (key === "bind")
          return (...args: unknown[]) => statement(target.bind(...args));
        if (["first", "all", "run"].includes(String(key)))
          return async (...args: unknown[]) =>
            decodeIam(
              await Reflect.apply(Reflect.get(target, key), target, args),
            );
        const v = Reflect.get(target, key);
        return typeof v === "function" ? v.bind(target) : v;
      },
    });
  }
  const proxy = new Proxy(db, {
    get(target, key) {
      if (key === "prepare")
        return (sql: string) => statement(target.prepare(sql));
      if (key === "pool") return clientProxy(Reflect.get(target, key) as Pool);
      const v = Reflect.get(target, key);
      return typeof v === "function" ? v.bind(target) : v;
    },
  });
  wrapped.set(db, proxy);
  wrapped.set(proxy, proxy);
  return proxy;
}
