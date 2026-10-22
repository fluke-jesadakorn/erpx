/** Application-owned persistence contracts. No platform runtime is required. */
export type SqlValue = string | number | boolean | null | Date | Uint8Array;
export type SqlResult<T = Record<string, unknown>> = {
  results: T[];
  success: boolean;
  meta: { changes: number; duration: number; last_row_id?: number };
};
export interface SqlStatement {
  bind(...values: unknown[]): SqlStatement;
  first<T = Record<string, unknown>>(column?: string): Promise<T | null>;
  all<T = Record<string, unknown>>(): Promise<SqlResult<T>>;
  run<T = Record<string, unknown>>(): Promise<SqlResult<T>>;
  raw<T = unknown[]>(options?: { columnNames?: boolean }): Promise<T[]>;
}
export interface SqlDatabase {
  prepare(sql: string): SqlStatement;
  batch<T = Record<string, unknown>>(statements: SqlStatement[]): Promise<SqlResult<T>[]>;
  exec(sql: string): Promise<{ count: number; duration: number }>;
}
export interface TransactionalSqlDatabase extends SqlDatabase {
  transaction<T>(action: (tx: SqlDatabase) => Promise<T>,options?:{sessionLock:{namespace:string;key:string};preferIndexedReads?:boolean;pipelineBatches?:boolean}): Promise<T>;
}
/** Use capabilities across Next dev reloads, which can replace class identities. */
export function isTransactionalSqlDatabase(
  database: SqlDatabase,
): database is TransactionalSqlDatabase {
  return typeof (database as Partial<TransactionalSqlDatabase>).transaction === "function";
}
export type FileMetadata = {
  key: string;
  size: number;
  etag: string;
  httpEtag: string;
  uploaded: Date;
  httpMetadata: { contentType?: string; cacheControl?: string; contentDisposition?: string };
  customMetadata: Record<string, string>;
};
export type StoredFile = FileMetadata & {
  body: ReadableStream<Uint8Array>;
  range?: { offset: number; length: number };
  arrayBuffer(): Promise<ArrayBuffer>;
  text(): Promise<string>;
};
export interface FileStore {
  /** Protected stores can reuse an active transaction from their owning database. */
  readonly supportsTransactionDeletion?: boolean;
  put(key: string, value: string | ArrayBuffer | ArrayBufferView | ReadableStream<Uint8Array> | Blob,
    options?: { onlyIf?: { etagDoesNotMatch: string }; httpMetadata?: FileMetadata['httpMetadata']; customMetadata?: Record<string, string> }): Promise<FileMetadata | null>;
  get(key: string, options?: { range?: { offset?: number; length?: number; suffix?: number } }): Promise<StoredFile | null>;
  head(key: string): Promise<FileMetadata | null>;
  delete(key: string | string[], authorization?: { database: SqlDatabase }): Promise<void>;
  destroyPreserved?(keys:string[], authorization:{database:SqlDatabase;destructionId:string}):Promise<void>;
  list(options?: { prefix?: string; cursor?: string; limit?: number }): Promise<{ objects: FileMetadata[]; truncated: boolean; cursor?: string }>;
}
