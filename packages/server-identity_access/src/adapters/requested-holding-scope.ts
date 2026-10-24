import{headers}from 'next/headers';
import{createRequestedHoldingScope}from '@erpx/server-identity_access/application/authorization';
export async function requestedHoldingScope(holdingSlug: string) { return createRequestedHoldingScope({ headers })(holdingSlug); }
