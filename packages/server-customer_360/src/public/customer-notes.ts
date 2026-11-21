import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeCustomerNotesClient,
  createNativeCustomerNotesCommandClient,
} from '../adapters/native-notes';

export type CustomerNotesView = {
  customer: { id: string; name: string; code: string; version: number };
  order?: {
    id: string;
    party_id: string;
    party_name: string;
    number: string;
    version: number;
  };
  notes: Array<{
    id: string;
    company_id: string;
    customer_id: string;
    order_id: string | null;
    channel: string;
    occurred_at: number;
    body: string;
    status: string;
    version: number;
    created_by: string;
    created_at: number;
    updated_at: number;
    canChange: boolean;
  }>;
  history: Array<{
    id: string;
    note_id: string;
    version: number;
    action: string;
    body: string;
    reason: string;
    actor_id: string;
    created_at: number;
  }>;
  canCreate: boolean;
};

export type CustomerNoteCommandResult = {
  id: string;
  version: number;
  status: string;
};

export function nativeCustomerNotesAvailable() {
  return process.env.ERPX_NATIVE_CUSTOMER_NOTES_CONFIG !== undefined;
}

function raw() {
  const value = process.env.ERPX_NATIVE_CUSTOMER_NOTES_CONFIG;
  if (value === undefined)
    throw new ApiError(
      503,
      'NATIVE_CUSTOMER_NOTES_CONFIGURATION_REQUIRED',
      'The native customer notes service is not configured.',
    );
  return value;
}

export async function nativeCustomerNotes(
  incoming: Pick<Headers, 'get'>,
  actor: WorkContext,
  input: { customerId?: string; orderId?: string },
): Promise<CustomerNotesView> {
  try {
    return await createNativeCustomerNotesClient(raw())(incoming, actor, input);
  } catch (error) {
    if (error instanceof ApiError) throw error;
    const failure = error as { code?: string; status?: number };
    throw new ApiError(
      failure.status ?? 503,
      failure.code ?? 'NATIVE_CUSTOMER_NOTES_UNAVAILABLE',
      'The native customer notes service is unavailable.',
    );
  }
}

export async function nativeCustomerNoteCommand(
  incoming: Pick<Headers, 'get'>,
  actor: WorkContext,
  input: Record<string, unknown>,
  key: string,
): Promise<CustomerNoteCommandResult> {
  try {
    return await createNativeCustomerNotesCommandClient(raw())(
      incoming,
      actor,
      input,
      key,
    );
  } catch (error) {
    if (error instanceof ApiError) throw error;
    const failure = error as { code?: string; status?: number };
    throw new ApiError(
      failure.status ?? 503,
      failure.code ?? 'NATIVE_CUSTOMER_NOTES_UNAVAILABLE',
      'The native customer notes service is unavailable.',
    );
  }
}
