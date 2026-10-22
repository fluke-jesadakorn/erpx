export type BoundedUploadRequest=Pick<Request,"headers"|"body">;
export function createBoundedAttachmentUpload(error:(status:number,code:string,message:string)=>Error){
const ATTACHMENT_UPLOAD_MAX_BYTES = 10 * 1024 * 1024;

async function readBoundedUploadBody(request:BoundedUploadRequest, expectedBytes: number) {
  const declared = Number(request.headers.get("content-length"));
  if (!Number.isSafeInteger(expectedBytes) || expectedBytes < 1 || expectedBytes > ATTACHMENT_UPLOAD_MAX_BYTES || declared !== expectedBytes)
    throw error(422, "UPLOAD_SIZE", "Content-Length must exactly match the bounded file reservation.");
  if (!request.body) throw error(422, "UPLOAD_SIZE", "The upload body is required.");

  const bytes = new Uint8Array(expectedBytes);
  const reader = request.body.getReader();
  let offset = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value || offset + value.byteLength > expectedBytes) {
        await reader.cancel("UPLOAD_SIZE").catch(() => undefined);
        throw error(422, "UPLOAD_SIZE", "The uploaded bytes exceed the file reservation.");
      }
      bytes.set(value, offset);
      offset += value.byteLength;
    }
  } finally {
    reader.releaseLock();
  }
  if (offset !== expectedBytes)
    throw error(422, "UPLOAD_SIZE", "The uploaded bytes do not match Content-Length.");
  return bytes.buffer;
}
return {readBoundedUploadBody};}
