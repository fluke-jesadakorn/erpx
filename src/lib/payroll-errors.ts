export type PayrollApiError = { code?: string; message?: string };

export function payrollErrorMessage(error: unknown, fallback = "Could not complete this Payroll step. Please try again."): string {
  const payload = error && typeof error === "object" ? error as PayrollApiError : null;
  const message = typeof payload?.message === "string" ? payload.message : error instanceof Error ? error.message : null;
  if (message && !/[\u0E00-\u0E7F]/.test(message)) return message;
  return payload?.code ? `${fallback} (${payload.code})` : fallback;
}
