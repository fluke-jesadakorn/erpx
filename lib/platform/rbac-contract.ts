export function validateChildAssignmentWindow(args: {
  parentValidFrom: Date;
  parentValidUntil: Date;
  parentRevokedAt?: Date | null;
  childValidFrom: Date;
  childValidUntil: Date;
  now?: Date;
}): void {
  const now = (args.now ?? new Date()).getTime();
  const parentValidFrom = args.parentValidFrom.getTime();
  const parentValidUntil = args.parentValidUntil.getTime();
  const childValidFrom = args.childValidFrom.getTime();
  const childValidUntil = args.childValidUntil.getTime();
  if (![now, parentValidFrom, parentValidUntil, childValidFrom, childValidUntil].every(Number.isFinite)) {
    throw new Error('assignment window contains an invalid timestamp');
  }
  if (
    args.parentRevokedAt
    || parentValidUntil <= now
    || childValidFrom < parentValidFrom
    || childValidUntil > parentValidUntil
    || childValidUntil <= childValidFrom
  ) {
    throw new Error('child assignment must start within an active parent assignment');
  }
}
