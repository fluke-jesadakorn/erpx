import "server-only";

/** Byte-identical parameterized join head shared by the Legal external-upload
 * reads (backfill, canonical image, destruction manifest, submission and the
 * detail service). Callers supply only the fixed projection and the two fixed
 * table aliases; no table, alias or predicate is accepted from a request. */
export const legalExternalUploadShareJoinHead = (
  projection: string, uploadAlias: string, shareAlias: string,
) =>
  `SELECT ${projection} FROM legal_external_uploads ${uploadAlias} JOIN legal_shares ${shareAlias} ON ${shareAlias}.id=${uploadAlias}.share_id `;

/** Byte-identical parameterized preservation/hold-target read head shared by
 * the ordinary and MinIO mutation read ports. The fixed projection, aliases
 * and the state inequality token are supplied by the trusted caller and
 * rendered verbatim; no table, alias or predicate comes from a request. */
export const legalFilePreservationHoldTargetHead = (
  projection: string, targetAlias: string, holdAlias: string, inequality: string,
) =>
  `SELECT ${projection} FROM legal_file_preservations p JOIN legal_hold_targets ${targetAlias} ON ${targetAlias}.item_id=p.item_id JOIN legal_holds ${holdAlias} ON ${holdAlias}.id=${targetAlias}.hold_id WHERE ${holdAlias}.state${inequality}'released'`;
