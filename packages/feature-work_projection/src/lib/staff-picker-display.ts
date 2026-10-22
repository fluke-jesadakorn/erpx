export type StaffPickerDisplayOptions = {
  people: readonly { user_id: string; legal_name: string; department_id: string | null; level_id: string | null; level_name: string | null }[];
  levels: readonly { id: string; name: string; rank: number }[];
  departments: readonly { id: string; display_name: string }[];
};

/** Keep original picker filters; native employment and management facts stay
 * with the server owners that authorize the actual Task command. */
export function staffPickerDisplayOptions(source: StaffPickerDisplayOptions): StaffPickerDisplayOptions {
  return {
    people: source.people.map(({ user_id, legal_name, department_id, level_id, level_name }) => ({ user_id, legal_name, department_id, level_id, level_name })),
    levels: source.levels.map(({ id, name, rank }) => ({ id, name, rank })),
    departments: source.departments.map(({ id, display_name }) => ({ id, display_name })),
  };
}
