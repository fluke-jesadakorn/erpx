/** Adapter boundaries translate business failures into the existing transport error. */
export class BusinessRuleError extends Error {
  constructor(readonly status: number, readonly code: string, message: string) { super(message); }
}
