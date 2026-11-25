import 'server-only';
import { hospitalityOwnerProjectionSources as filesProjectionSources } from '@erpx/server-files/public/hospitality-owner-projection-sources';

/** Canonical parameterized attachment-presence builders (G4 read-tail r55). The six
 * former inline V2/V1 literals (receipt probe, batch receipt list and media-filtered
 * payment-slip probe) now delegate here; each caller keeps its attachmentsV2Readable()
 * branch, its Files interpolations and its original binds unchanged. The V2/V1 pair
 * extends the r49/r51 family with the probe/list/exists shapes, owner kind, media
 * filter and scalar-vs-json bind form as parameters. Pure reads only: no writes,
 * table/row locks, advisory statements, native kinds or cross-owner projections. */
export type PpRd55PresenceShape = 'probe' | 'list' | 'distinct' | 'exists';

const MEDIA_TYPES = "'image/jpeg','image/png','image/webp','application/pdf'";

/** Single owner-owned rendering of the json-each list predicate. Both presence
 * builders below previously carried a byte-identical copy; they now interpolate
 * this one fragment so the V1/V2 rendered SQL stays unchanged. */
const JSON_EACH_IN = ' IN(SELECT value FROM erpx_json_each(?))';

/** V2 join form. `payment` selects the owner kind, `media` the media-type predicate,
 * `jsonBind` the scalar owner-id equality vs the json-each list form, and `shape`
 * the probe / list / exists rendering. Each caller keeps its single bind and its
 * attachmentsV2Readable() branch. */
export const ppRd55AttachmentPresenceV2 = (
  shape: PpRd55PresenceShape,
  payment: boolean,
  media: boolean,
  jsonBind: boolean,
) => {
  const owner = payment ? 'expense_payment' : 'expense_claim';
  const a = filesProjectionSources.attachments;
  const l = filesProjectionSources.attachment_links;
  const s = filesProjectionSources.attachment_sets;
  const id = jsonBind ? JSON_EACH_IN : '=?';
  const body = shape === 'probe'
    ? `FROM (${a}) a JOIN (${l}) l ON l.attachment_id=a.id
             JOIN (${s}) s ON s.id=l.set_id
             WHERE a.owner_type='${owner}' AND a.owner_id${id} AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state='bound'
               AND s.target_type='${owner}' AND s.target_id=a.owner_id`
    : shape === 'exists'
      ? `FROM (${a}) a JOIN (${l}) l ON l.attachment_id=a.id JOIN (${s}) s ON s.id=l.set_id
        WHERE a.owner_type='${owner}' AND a.owner_id${id}${media ? `
          AND a.media_type IN (${MEDIA_TYPES})` : ''}
          AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound'
          AND s.target_type='${owner}' AND s.target_id=a.owner_id`
      : `FROM (${a}) a JOIN (${l}) l ON l.attachment_id=a.id JOIN (${s}) s ON s.id=l.set_id WHERE a.owner_type='${owner}' AND a.owner_id${id} AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound' AND s.target_type='${owner}' AND s.target_id=a.owner_id`;
  const cols = shape === 'list' || shape === 'distinct' ? 'DISTINCT a.owner_id id' : '1';
  const open = shape === 'exists' ? 'EXISTS(' : '';
  const close = shape === 'exists' ? ')' : shape === 'probe' ? ' LIMIT 1' : '';
  return `${open}SELECT ${cols} ${body}${close}`;
};

/** V1 fallback. `payment`, `media`, `jsonBind` and `shape` mirror the V2 builder;
 * the Files projection, bare-column predicates and original bind arity stay verbatim. */
export const ppRd55AttachmentPresenceV1 = (
  shape: PpRd55PresenceShape,
  payment: boolean,
  media: boolean,
  jsonBind: boolean,
) => {
  const owner = payment ? 'expense_payment' : 'expense_claim';
  const a = filesProjectionSources.attachments;
  const id = jsonBind ? JSON_EACH_IN : '=?';
  const where = `FROM (${a}) attachments WHERE owner_type='${owner}' AND owner_id${id}${media ? ` AND media_type IN (${MEDIA_TYPES})` : ''} AND revoked_at IS NULL`;
  const cols = shape === 'list' || shape === 'distinct' ? 'DISTINCT owner_id id' : '1';
  const open = shape === 'exists' ? 'EXISTS(' : '';
  const close = shape === 'exists' ? ')' : shape === 'probe' ? ' LIMIT 1' : '';
  return `${open}SELECT ${cols} ${where}${close}`;
};
