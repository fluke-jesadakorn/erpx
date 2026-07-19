import { safeEqual } from './sessionToken';

export function isTrustedWorkerRequest(req: Request): boolean {
  const expected = process.env.FOLIO_WORKER_TOKEN ?? '';
  if (expected.length < 32) return false;

  const bearer = req.headers.get('authorization');
  const supplied = bearer?.startsWith('Bearer ')
    ? bearer.slice('Bearer '.length)
    : req.headers.get('x-folio-worker-token') ?? '';
  return safeEqual(supplied, expected);
}
