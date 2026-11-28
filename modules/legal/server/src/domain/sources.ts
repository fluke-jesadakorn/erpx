import {z} from "zod";
export const legalPreservationInput=z.object({action:z.literal("source.attach"),id:z.string(),expectedVersion:z.number().int().positive(),attachmentId:z.string()});
export type LegalPreservedSource={object_key:string;media_type:string;original_name:string;owner_type:string;owner_id:string;checksum_sha256:string;source_state:string;attachment_link_id:string|null};
