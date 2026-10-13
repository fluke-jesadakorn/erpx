import {z} from 'zod';
import {BusinessRuleError} from '../../shared';
import {attributeAccess,validateAttributeValue,type AttributeDefinition} from "@/lib/identity_access/user-management";
import type {DirectoryInventory} from "@/lib/identity_access/directory-operations";
export const fieldSyncInput=z.object({connectionId:z.string().min(1),personId:z.string().min(1),direction:z.enum(['read','write']),reason:z.string().trim().min(3).max(1000),previewToken:z.string().optional()});
export type FieldSyncInput=z.infer<typeof fieldSyncInput>;
export type DirectoryFieldState={version:number;connection_version:number;tested_version:number;inventory:DirectoryInventory|null;definition:{mappings?:{direction:'read'|'write';field:string;attributeId:string}[]}};
export type DirectoryFieldIdentity={id:string;external_id:string};
export type DirectoryFieldValues={version:number;definition:Record<string,unknown>};
export type DirectoryFieldAttribute={id:string;version:number;definition:AttributeDefinition};
export function mappedFieldChange(actor:{role:string;scopeKind:string},direction:'read'|'write',field:string,attributeId:string,inventory:DirectoryInventory,external:DirectoryInventory['objects'][number],stored:DirectoryFieldValues|null,row:DirectoryFieldAttribute|null){if(!row||row.definition.archived)throw new BusinessRuleError(409,'MAPPING_FIELD_CHANGED','A mapped field changed.');const attribute={...row.definition,id:row.id,version:row.version},access=attributeAccess(attribute,actor.role,actor.scopeKind);if(!access.canView||direction==='read'&&!access.canEdit)throw new BusinessRuleError(403,'ATTRIBUTE_WRITE_FORBIDDEN','Your role cannot synchronize this owned field.');if(!inventory.fields.some(f=>f.name===field&&f[direction]))throw new BusinessRuleError(409,'MAPPING_SCHEMA_CHANGED','The connector no longer supports the mapping.');const from=direction==='read'?external.properties[field]:stored?.definition[attributeId],previous=direction==='read'?stored?.definition[attributeId]:external.properties[field],checked=validateAttributeValue(attribute,from);return {attribute,checked,change:{label:attribute.label,from:previous??null,to:checked}};}
