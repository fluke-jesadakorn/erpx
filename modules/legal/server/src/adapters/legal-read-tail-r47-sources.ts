import "server-only";
import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/files-server/public/final-root-owner-projection-sources";

/** One verified Legal attachment-availability spine shared by the evidence read
 * and both law-corpus reads. Renders byte-identically to every inline copy: only
 * the trusted link/file aliases vary; the bound-file projections, the bound state,
 * company, source, revocation and checksum predicates and the fixed parentheses
 * remain verbatim. Callers pass trusted constant aliases only: no request text is
 * interpolated and no bind is added. */
export const legalAttachmentAvailableSql = (linkAlias = 'l', fileAlias = 'f') =>
  `(d.object_key IS NULL OR EXISTS(SELECT 1 FROM (${filesProjectionSources.attachment_links}) ${linkAlias} JOIN (${filesProjectionSources.attachments}) ${fileAlias} ON ${fileAlias}.id=${linkAlias}.attachment_id WHERE ${linkAlias}.id=d.attachment_link_id AND ${linkAlias}.state='bound' AND ${fileAlias}.company_id=i.company_id AND ${fileAlias}.source_state='available' AND ${fileAlias}.revoked_at IS NULL AND ${fileAlias}.checksum_sha256=d.checksum))`;
