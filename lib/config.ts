import 'node:process';

const str = (v: string | undefined, d: string) => (v && v.length ? v : d);
const num = (v: string | undefined, d: number) => (v ? parseInt(v, 10) : d);

export const config = {
  sessionSecret: process.env.SESSION_SECRET || '',
  encryptionKey: process.env.ENCRYPTION_KEY || '',
  ollamaUrl: str(process.env.OLLAMA_URL, 'http://localhost:11434'),
  visionModel: str(process.env.OCR_VISION_MODEL || process.env.VISION_MODEL, 'qwen3-vl:4b'),
  embedModel: str(process.env.EMBED_MODEL, 'bge-m3:latest'),
  maxPdfBytes: 50 * 1024 * 1024,
  storage: {
    backend: (process.env.STORAGE_BACKEND || 'minio') as 'minio' | 'local',
    root: process.env.STORAGE_ROOT || '',
    minio: {
      endPoint: str(process.env.MINIO_ENDPOINT, 'localhost'),
      port: num(process.env.MINIO_PORT, 9000),
      publicHost: process.env.MINIO_PUBLIC_HOST || '',
      publicPort: num(process.env.MINIO_PUBLIC_PORT, 0),
      useSSL: (process.env.MINIO_USE_SSL || 'false') === 'true',
      accessKey: str(process.env.MINIO_ACCESS_KEY, 'minioadmin'),
      secretKey: str(process.env.MINIO_SECRET_KEY, 'minioadmin'),
      bucket: str(process.env.MINIO_BUCKET, 'folio-storage'),
    },
  },
};