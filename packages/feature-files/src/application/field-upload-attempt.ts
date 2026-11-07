/** An in-memory attempt retains the original bytes and command identity until
 * both reservation and upload receipts are validated. It is lost on reload. */
export type FieldUploadContext = { identity: string; generation: number; valid: boolean };
export type FieldReservation = { uploadIntentId: string; attachmentLinkId: string; uploadUrl: string; expiresAt: number };
export type FieldUploadInput<FileBody> = { scope: string; base: string; setId: string; body: string; file: FileBody; mediaType: string };
export type FieldUploadAttempt<FileBody> = FieldUploadInput<FileBody> & { key: string; context: FieldUploadContext; reservation?: FieldReservation; busy: boolean; reserveUncertain: boolean; uploadUncertain: boolean };

type Ports<FileBody> = {
  context: () => FieldUploadContext | null;
  key: () => string;
  reserve: (attempt: FieldUploadAttempt<FileBody>) => Promise<unknown>;
  upload: (attempt: FieldUploadAttempt<FileBody>, reservation: FieldReservation) => Promise<unknown>;
  terminalRejection: (error: unknown, phase: "reserve" | "upload") => boolean;
};

const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
function reservation(value: unknown): value is FieldReservation {
  return object(value) && typeof value.uploadIntentId === 'string' && !!value.uploadIntentId && typeof value.attachmentLinkId === 'string' && !!value.attachmentLinkId && typeof value.uploadUrl === 'string' && !!value.uploadUrl && typeof value.expiresAt === 'number' && Number.isSafeInteger(value.expiresAt);
}
function uploaded(value: unknown, expected: FieldReservation): boolean {
  return object(value) && value.uploadIntentId === expected.uploadIntentId && value.attachmentLinkId === expected.attachmentLinkId && typeof value.sourceId === 'string' && !!value.sourceId && value.status === 'scan_pending' && typeof value.state === 'string';
}

export function createFieldUploadAttempt<FileBody>(ports: Ports<FileBody>) {
  const pending = new Map<string, FieldUploadAttempt<FileBody>>();
  const current = (a: FieldUploadAttempt<FileBody>) => {
    const c = ports.context();
    if (!c?.valid || c.identity !== a.context.identity || c.generation !== a.context.generation) throw Error('Return to the original account, Company and role before retrying this upload.');
  };
  async function run(a: FieldUploadAttempt<FileBody>) {
    if (a.busy) throw Error('Wait for the original upload.');
    current(a); a.busy = true;
    try {
      if (!a.reservation) {
        let result:unknown;const previouslyUncertain=a.reserveUncertain;a.reserveUncertain=true;
        try { result=await ports.reserve(a); }
        catch(error) { if(!previouslyUncertain && ports.terminalRejection(error,'reserve')){current(a);pending.delete(a.scope);}throw error; }
        current(a);
        if (!reservation(result)) {a.reserveUncertain=true;throw Error('Unable to confirm the original file reservation.');}
        a.reservation = result; a.reserveUncertain=false;
      }
      let result:unknown;const previouslyUncertain=a.uploadUncertain;a.uploadUncertain=true;
      try {result=await ports.upload(a, a.reservation);}
      catch(error) {if(!previouslyUncertain && ports.terminalRejection(error,'upload')){current(a);pending.delete(a.scope);}throw error;}
      current(a);
      if (!uploaded(result, a.reservation)) {a.uploadUncertain=true;throw Error('Unable to confirm the original upload.');}
      if (pending.get(a.scope) !== a) throw Error('Return to the original upload.');
      pending.delete(a.scope);
      return { reservation: a.reservation, receipt: result };
    } finally { a.busy = false; }
  }
  async function submit(input: FieldUploadInput<FileBody>) {
    if (pending.has(input.scope)) throw Error('Confirm the original upload before adding another file.');
    if (pending.size >= 32) throw Error('Confirm an existing upload before starting another.');
    const context = ports.context();
    if (!context?.valid) throw Error('Review your current work role before uploading.');
    const a: FieldUploadAttempt<FileBody> = { ...input, key: ports.key(), context: { ...context }, busy: false, reserveUncertain:false, uploadUncertain:false };
    pending.set(a.scope, a); return run(a);
  }
  async function retry(scope: string) {
    const a = pending.get(scope); if (!a) throw Error('There is no original upload to confirm.');
    if(a.busy)throw Error('Wait for the original upload.');
    const c=ports.context();if(!c?.valid||c.identity!==a.context.identity)throw Error('Return to the original account, Company and role before retrying this upload.');
    a.context={...c};return run(a);
  }
  return { submit, retry, pending: (scope: string) => pending.get(scope) };
}
