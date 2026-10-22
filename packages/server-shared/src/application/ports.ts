/** Ports carry business operations, never SQL statements or a runtime handle. */
export interface Clock {
  now(): number;
}

export interface IdentifierSource {
  next(): string;
}

/**
 * Ports are built against one transaction by the adapter. A cross-owner handoff
 * receives the same Ports object; it must not acquire another database/runtime.
 * Receipt, aggregate writes, audit and outbox are committed together by run().
 */
export interface UnitOfWork<Ports> {
  run<Result>(action: (ports: Ports) => Promise<Result>): Promise<Result>;
}

export interface CommandIdentity {
  key: string;
  operation: string;
  recordId: string;
  input: unknown;
}

export interface CommandReceipt<Result> {
  replay(identity: CommandIdentity): Promise<{ result: Result } | null>;
  save(identity: CommandIdentity, result: Result): Promise<void>;
}

export interface AuditSink<Event> {
  append(event: Event): Promise<void>;
}

export interface Outbox<Event> {
  enqueue(event: Event): Promise<void>;
}

export interface Authorization<Actor> {
  require(actor: Actor, permission: string): Promise<void>;
}
