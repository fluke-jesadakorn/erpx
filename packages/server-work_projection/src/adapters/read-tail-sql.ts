/** Shared calendar read-tail SQL fragments. One definition serves every fact unit. */
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
