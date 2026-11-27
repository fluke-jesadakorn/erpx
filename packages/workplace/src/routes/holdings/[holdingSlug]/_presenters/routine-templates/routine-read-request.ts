import 'server-only';
import { headers } from 'next/headers';
import { developmentModuleAdmissionDeferred } from '@erpx/contracts/development-module-admission';
import { routineReadHeaders } from '@erpx/feature-operations/public/native-routines-read';
import { productHostForHost } from '@erpx/feature-runtime/lib/product-hosts';

export async function routineReadRequest(
  holdingSlug: string,
  suffix: string,
  actor: Parameters<typeof routineReadHeaders>[1],
): Promise<Request> {
  const incoming = await headers();
  const apps = productHostForHost(incoming.get('host')) === 'apps';
  const admission = apps
    ? developmentModuleAdmissionDeferred('apps', process.env) ? 'deferred' : 'purchased'
    : undefined;
  return new Request(
    `http://native-routines.invalid/api/v1/holdings/${encodeURIComponent(holdingSlug)}/${suffix}`,
    { headers: routineReadHeaders(incoming, actor, admission) },
  );
}
