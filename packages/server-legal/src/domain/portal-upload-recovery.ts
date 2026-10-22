export type LegalPortalUploadNotApplied={id:string;requestFingerprint:string;outcome:'not-applied'};
/** Only fields already captured in the caller's upload draft. This is not an
 * authority token or a hash of the private owner/source/receipt witness. */
export function legalPortalUploadFingerprintInput(input:{shareId:string;name:string;mediaType:string;bytes:number;checksum:string}){
 return JSON.stringify([input.shareId,input.name,input.mediaType,input.bytes,input.checksum]);
}
