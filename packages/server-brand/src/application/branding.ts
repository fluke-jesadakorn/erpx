import { BusinessRuleError } from "@erpx/server-shared/index";
import {
  validateBrandImageDimensions,
  type DocumentBrandMode,
} from "@erpx/server-brand/domain/branding";

/** Pure image validation/encoding used by the retained read/media fixture
 * boundary. Branding mutations are owned by the native Rust command path. */
export async function normalizeBrandMedia<Bytes extends Uint8Array>(
  image: { metadata(buffer: ArrayBuffer): Promise<{ width?: number; height?: number }>; encode(buffer: ArrayBuffer): Promise<Bytes> },
  buffer: ArrayBuffer,
): Promise<Bytes> {
  let metadata;
  try {
    metadata = await image.metadata(buffer);
  } catch {
    throw new BusinessRuleError(422, "LOGO_INVALID", "Upload a valid PNG or WebP image.");
  }
  validateBrandImageDimensions(metadata);
  return image.encode(buffer);
}
export type { DocumentBrandMode };
