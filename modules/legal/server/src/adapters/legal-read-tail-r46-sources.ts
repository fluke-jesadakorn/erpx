import "server-only";

/** One verified Legal active-destruction guard, parameterized only by the two
 * trusted aliases. Renders byte-identically to each inline copy: the leading
 * AND, the exact spacing and the ('executing','completed') IN-list are fixed;
 * no table, alias or predicate is accepted from a request. The narrower
 * queued-purchases `state='executing'` guard is a different family and stays
 * in its owner adapter. */
export const legalActiveDestructionGuardHead = (destructionAlias = 'destruction', itemAlias = 'i') =>
  `AND NOT EXISTS(SELECT 1 FROM legal_destructions ${destructionAlias} WHERE ${destructionAlias}.item_id=${itemAlias}.id AND ${destructionAlias}.state IN ('executing','completed'))`;

/** Byte-identical obligation/hold spine heads shared by the executive metric
 * projection and the main evidence reads. Callers supply the complete trusted
 * read prefix (a metric-count prefix or a fixed evidence projection) and the
 * trusted leading whitespace gap; the JOIN/WHERE spine and both aliases are
 * fixed by the head. The fixed IN-lists remain verbatim at every call site. */
export const legalExecutiveObligationSpineHead = (selectPrefix: string, gap = '      ') =>
  `${selectPrefix}\n${gap}FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id\n${gap}WHERE `;

export const legalExecutiveHoldSpineHead = (selectPrefix: string, gap = '      ') =>
  `${selectPrefix}\n${gap}FROM legal_holds h JOIN legal_items i ON i.id=h.item_id\n${gap}WHERE `;
