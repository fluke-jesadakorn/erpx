/** Display values only. Current worker eligibility, Project membership and
 * purchase decisions remain with the original server owners. */
export type ProjectWorkerDisplay={user_id:string;legal_name:string};
export type ProjectMemberPickerOptions={
 people:readonly (ProjectWorkerDisplay&{department_id:string|null;level_id:string|null;level_name:string|null})[];
 levels:readonly {id:string;name:string;rank:number}[];
 departments:readonly {id:string;display_name:string}[];
};
export function projectWorkerDisplayChoices(people:readonly ProjectWorkerDisplay[]):ProjectWorkerDisplay[]{
 return people.map(person=>({user_id:person.user_id,legal_name:person.legal_name}));
}
/** Retain the existing department/level filters, without serializing native
 * HR identities, engagement/appointment versions or department-head sources. */
export function projectMemberPickerOptions(source:ProjectMemberPickerOptions):ProjectMemberPickerOptions{
 return {people:source.people.map(person=>({user_id:person.user_id,legal_name:person.legal_name,department_id:person.department_id,level_id:person.level_id,level_name:person.level_name})),
  levels:source.levels.map(level=>({id:level.id,name:level.name,rank:level.rank})),
  departments:source.departments.map(department=>({id:department.id,display_name:department.display_name}))};
}
