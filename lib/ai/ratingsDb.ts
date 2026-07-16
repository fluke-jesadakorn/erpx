import 'server-only';
import { query } from '../db';
import type { ModelRatings } from './modelDescriptions';

interface RatingRow {
  speed: number;
  accuracy: string | number;
}

export async function ratingsForDb(name: string): Promise<ModelRatings | null> {
  const res = await query<RatingRow>(
    `SELECT speed, accuracy FROM ai.model_ratings WHERE model_name = $1`,
    [name],
  );
  const row = res.rows[0];
  if (!row) return null;
  return { speed: row.speed, accuracy: Number(row.accuracy) };
}

export async function recomputeRatings(): Promise<void> {
  await query(`SELECT ai.recompute_model_ratings()`);
}