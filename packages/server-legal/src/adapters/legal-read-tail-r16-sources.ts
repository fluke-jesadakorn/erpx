import "server-only";
import contract from "./legal-read-tail-r16-sources.json" with { type: "json" };
/** Native-owned Legal item-list read prefix shared by owner adapter point reads.
 * The caller still prepares and executes with its own predicate and binds; the
 * fixed item projection and trailing WHERE space are owner-controlled. */
const sql = contract.itemSelect;
if (!sql || !sql.endsWith(" WHERE ")) throw new Error("LEGAL_READ_TAIL_R16_CONTRACT_INVALID");
export const legalItemSelectSql = sql;
