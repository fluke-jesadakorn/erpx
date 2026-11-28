import "server-only";
import contract from './planning-read-tail-sources.json' with {type:'json'};
/** Owner-local renderer for the native Planning read-tail contract bound into
 * Rust with include_str!. Only fixed server-owned projections, aliases and
 * predicates are substituted; a template that lost one of its tokens fails
 * closed. */
export const fillPlanningReadTail = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('PLANNING_READ_TAIL_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
export const planningReadTailSources = contract;
