const { Client } = require('pg');
const axios = require('axios');

// Database connection config
const dbConfig = {
  host: process.env.DB_HOST || 'localhost',
  port: parseInt(process.env.DB_PORT || '5432'),
  user: process.env.DB_USER || 'contract',
  password: process.env.DB_PASSWORD || 'contractpw',
  database: process.env.DB_NAME || 'world_erp_db',
};

const OLLAMA_URL = process.env.OLLAMA_URL || 'http://localhost:11434';
const EMBED_MODEL = process.env.EMBED_MODEL || 'bge-m3:latest';

async function run() {
  const client = new Client(dbConfig);
  try {
    await client.connect();
    console.log('Connected to database successfully.');

    // Fetch accounts that don't have embeddings yet, or all of them
    const res = await client.query('SELECT code, name, name_th FROM chart_of_accounts');
    const accounts = res.rows;
    console.log(`Found ${accounts.length} accounts to process.`);

    for (const account of accounts) {
      // Combine English and Thai name to give the embedding model rich context
      const textToEmbed = `${account.name} | ${account.name_th}`;
      console.log(`Generating embedding for: "${textToEmbed}"...`);

      try {
        const ollamaRes = await axios.post(`${OLLAMA_URL}/api/embed`, {
          model: EMBED_MODEL,
          input: textToEmbed
        });

        // The embeddings can be in ollamaRes.data.embedding or ollamaRes.data.embeddings[0]
        const embedding = ollamaRes.data.embeddings 
          ? ollamaRes.data.embeddings[0] 
          : ollamaRes.data.embedding;

        if (!embedding) {
          throw new Error('No embedding returned from Ollama response.');
        }

        // Format the embedding array for PostgreSQL pgvector
        // pgvector accepts arrays in the format '[0.1, 0.2, ...]'
        const vectorStr = `[${embedding.join(',')}]`;

        await client.query(
          'UPDATE chart_of_accounts SET embedding = $1 WHERE code = $2',
          [vectorStr, account.code]
        );
        console.log(`Successfully saved embedding for code ${account.code} (${account.name}).`);
      } catch (err) {
        console.error(`Error processing code ${account.code}:`, err.message);
      }
    }

    console.log('Embedding generation complete.');
  } catch (err) {
    console.error('Database connection or query error:', err);
  } finally {
    await client.end();
  }
}

run();
