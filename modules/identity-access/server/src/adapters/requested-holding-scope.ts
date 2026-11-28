import{headers}from 'next/headers';
import{createRequestedHoldingScope}from '@erpx/identity-access-server/application/authorization';
export async function requestedHoldingScope(holdingSlug: string) { return createRequestedHoldingScope({ headers })(holdingSlug); }
