// Curated blurbs for known Ollama vision models.
// Applied on first read by /api/ai/vision-models when the row's description is NULL.
// Admins can override via ModelsPane after seed.

export const MODEL_DESCRIPTIONS: Record<string, string> = {
  'qwen3-vl:4b': 'Fast 4B vision. Good for clear printed receipts. Weaker on Thai handwriting — pick a larger model for accuracy.',
  'qwen3-vl:8b': 'Mid-size Qwen vision. Better handwriting + table structure, ~2x slower.',
  'gemma4:12b-mlx': 'Larger 12B multimodal. Strongest on Thai handwriting + structured tables, slower (MLX).',
  'gemma3:4b': 'Gemma 3 4B vision. Decent on simple receipts, fast.',
  'gemma3:12b': 'Gemma 3 12B vision. Higher accuracy, larger download.',
  'llama3.2-vision:11b': 'Meta 11B vision. Solid English receipts, weaker Thai.',
  'llava:13b': 'LLaVA 13B. Older baseline, fast but weaker on handwriting.',
  'minicpm-v:8b': 'MiniCPM-V 8B. Compact multimodal, good OCR focus.',
  'qwen3.6:35b-a3b-q4_K_M': 'Qwen 3.6 35B MoE (Q4). Strong general reasoning + vision, ~23GB RAM, slow.',
  'qwen3.6:35b-mlx': 'Qwen 3.6 35B MoE (MLX). Apple Silicon-tuned, vision capable, slow.',
  'MiniMax-M3': 'MiniMax M3 — general reasoning + vision. Strong on Thai receipts, ~200ms typical latency.',
};

export function descriptionFor(name: string): string | null {
  return MODEL_DESCRIPTIONS[name] ?? null;
}

export interface ModelRatings {
  speed: number;
  accuracy: number;
}

export const MODEL_RATINGS: Record<string, ModelRatings> = {
  'qwen3-vl:4b':             { speed: 5, accuracy: 3 },
  'qwen3-vl:8b':             { speed: 4, accuracy: 4 },
  'gemma3:4b':               { speed: 5, accuracy: 3 },
  'gemma3:12b':              { speed: 3, accuracy: 4 },
  'gemma4:12b-mlx':          { speed: 2, accuracy: 5 },
  'llama3.2-vision:11b':     { speed: 3, accuracy: 3 },
  'llava:13b':               { speed: 3, accuracy: 2 },
  'minicpm-v:8b':            { speed: 4, accuracy: 3 },
  'qwen3.6:35b-a3b-q4_K_M': { speed: 2, accuracy: 5 },
  'qwen3.6:35b-mlx':         { speed: 2, accuracy: 5 },
  'MiniMax-M3':              { speed: 4, accuracy: 4 },
};

export function ratingsFor(name: string): ModelRatings | null {
  return MODEL_RATINGS[name] ?? null;
}