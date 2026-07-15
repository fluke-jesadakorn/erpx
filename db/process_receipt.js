const fs = require('fs');
const path = require('path');
const { Client } = require('pg');
const axios = require('axios');

// Manually load .env variables
const envPath = path.join(__dirname, '../.env');
if (fs.existsSync(envPath)) {
  const envText = fs.readFileSync(envPath, 'utf8');
  envText.split('\n').forEach(line => {
    const trimmed = line.trim();
    if (trimmed && !trimmed.startsWith('#')) {
      const eqIdx = trimmed.indexOf('=');
      if (eqIdx > 0) {
        const key = trimmed.substring(0, eqIdx).trim();
        const value = trimmed.substring(eqIdx + 1).trim();
        process.env[key] = value;
      }
    }
  });
}

const dbConfig = {
  host: process.env.DB_HOST || 'localhost',
  port: parseInt(process.env.DB_PORT || '5432'),
  user: process.env.DB_USER || 'contract',
  password: process.env.DB_PASSWORD || 'contractpw',
  database: process.env.DB_NAME || 'world_erp_db',
};

const OLLAMA_URL = process.env.OLLAMA_URL || 'http://localhost:11434';
const EMBED_MODEL = process.env.EMBED_MODEL || 'bge-m3:latest';
const LOCAL_LLM_MODEL = process.env.LOCAL_LLM_MODEL || 'qwen2.5:7b';
const LOCAL_VISION_MODEL = process.env.LOCAL_VISION_MODEL || 'qwen3-vl:4b';

// Default mock raw receipt text (can be overridden by command line arguments)
const defaultReceiptText = `
Receipt/Abbreviated Tax Invoice
Starbucks Coffee, True Digital Park Branch
Date: 24/06/2026 14:30
Items:
1. Americano Hot Venti (Large) = 145.00 THB
2. Ham and Cheese Croissant Sandwich = 135.00 THB
Additional Service Charge: 40.00 THB
Subtotal: 299.07 THB
VAT 7%: 20.93 THB
Grand Total: 320.00 THB
Payment Method: Cash
`;

async function main() {
  const inputText = process.argv[2] || defaultReceiptText;
  
  const systemPrompt = `You are a professional financial AI parsing agent. Analyze the receipt and extract values into structured JSON.
Return JSON conforming to this schema (no markdown, no backticks, strictly JSON):
{
  "vendorName": "String (store name)",
  "transactionDate": "YYYY-MM-DD",
  "subtotal": Number,
  "vatAmount": Number,
  "totalAmount": Number,
  "paymentMethod": "cash" | "credit_card" | "transfer",
  "items": [
    { "description": "String (item description in English or Thai)", "amount": Number }
  ]
}`;

  let messages = [];
  let modelToUse = LOCAL_LLM_MODEL;

  // Check if input is a local image file path
  const inputClean = inputText.trim();
  const isImage = (/\.(jpg|jpeg|png|webp)$/i).test(inputClean);
  const imageExists = isImage && fs.existsSync(inputClean);

  if (imageExists) {
    console.log(`Detected local image input: "${inputClean}"`);
    console.log(`Using local Vision Model via Ollama: "${LOCAL_VISION_MODEL}"`);
    modelToUse = LOCAL_VISION_MODEL;
    const imgBase64 = fs.readFileSync(inputClean, { encoding: 'base64' });
    messages = [
      {
        role: 'user',
        content: `${systemPrompt}\n\nAnalyze this receipt image and extract values into the JSON schema format:`,
        images: [imgBase64]
      }
    ];
  } else {
    if (isImage) {
      console.log(`Note: Input looked like an image path but file was not found. Treating as text.`);
    }
    console.log(`Detected text input. Using local LLM Model via Ollama: "${LOCAL_LLM_MODEL}"`);
    console.log('--- RAW RECEIPT INPUT ---');
    console.log(inputText.trim());
    console.log('-------------------------\n');
    messages = [
      { role: 'system', content: systemPrompt },
      { role: 'user', content: `Parse this receipt text:\n${inputText}` }
    ];
  }

  const client = new Client(dbConfig);
  try {
    // 1. Call local Ollama chat with JSON formatting
    console.log(`Sending request to local Ollama chat API (${modelToUse})...`);
    
    const ollamaRes = await axios.post(`${OLLAMA_URL}/api/chat`, {
      model: modelToUse,
      messages: messages,
      format: 'json',
      stream: false,
      options: {
        temperature: 0.1
      }
    });

    const rawJsonStr = ollamaRes.data.message.content.trim();
    console.log('--- EXTRACTED JSON FROM LOCAL AI ---');
    console.log(rawJsonStr);
    console.log('------------------------------------\n');

    const parsedData = JSON.parse(rawJsonStr);

    // 2. SERIOUS CORRECTIVE MATH CHECK
    console.log('Performing Mathematical Correction & Integrity Checks...');
    const computedTotal = parseFloat((parsedData.subtotal + parsedData.vatAmount).toFixed(2));
    const statedTotal = parseFloat(parsedData.totalAmount.toFixed(2));
    
    let isCorrupted = false;
    let correctionNotes = 'Verified matching subtotal + VAT math.';

    if (Math.abs(computedTotal - statedTotal) > 0.01) {
      isCorrupted = true;
      correctionNotes = `⚠️ MATH DISCREPANCY DETECTED: Subtotal (${parsedData.subtotal}) + VAT (${parsedData.vatAmount}) = ${computedTotal}, but Total stated is ${statedTotal} (Difference of ${Math.abs(computedTotal - statedTotal).toFixed(2)} THB). Potential scan error or unlisted service charge.`;
      console.warn('\x1b[33m%s\x1b[0m', correctionNotes);
    } else {
      console.log('✅ Mathematical equation checks out (Subtotal + VAT = TotalAmount).');
    }

    // 3. Connect to Database
    await client.connect();
    await client.query('BEGIN');

    // Default to Submitter = 1 (John Staff, Staff)
    const submitterId = 1;

    // Insert Header into expenses
    console.log('Saving Expense Header into PostgreSQL...');
    const headerRes = await client.query(`
      INSERT INTO expenses (
        submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount, 
        payment_method, status, is_corrupted, correction_notes, ocr_raw_json
      )
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
      RETURNING id
    `, [
      submitterId,
      parsedData.vendorName,
      parsedData.transactionDate,
      parsedData.subtotal,
      parsedData.vatAmount,
      parsedData.totalAmount,
      parsedData.paymentMethod,
      'ocr_extracted',
      isCorrupted,
      correctionNotes,
      JSON.stringify(parsedData)
    ]);

    const expenseId = headerRes.rows[0].id;
    console.log(`Saved Header successfully. Expense ID: EXP-${expenseId}`);

    // 4. Process Line Items and generate embeddings
    console.log('\nProcessing items and mapping to Chart of Accounts (COA) via BGE-M3...');
    for (const item of parsedData.items) {
      console.log(`- Item: "${item.description}" (${item.amount} THB)`);
      
      let bestCode = null;
      let score = 0;

      try {
        // Compute embedding from Ollama
        const embedRes = await axios.post(`${OLLAMA_URL}/api/embed`, {
          model: EMBED_MODEL,
          input: item.description
        });

        const embedding = embedRes.data.embeddings 
          ? embedRes.data.embeddings[0] 
          : embedRes.data.embedding;

        if (embedding) {
          const vectorStr = `[${embedding.join(',')}]`;
          
          // Query closest account in COA
          const matchRes = await client.query(`
            SELECT code, name, name_th, (1 - (embedding <=> $1::vector)) as similarity
            FROM chart_of_accounts
            ORDER BY similarity DESC
            LIMIT 1
          `, [vectorStr]);

          if (matchRes.rows.length > 0) {
            bestCode = matchRes.rows[0].code;
            score = matchRes.rows[0].similarity;
            console.log(`  ➡️ Mapped to: [${bestCode}] ${matchRes.rows[0].name_th} (${matchRes.rows[0].name}) with score: ${(score * 100).toFixed(2)}%`);
          }
        }
      } catch (err) {
        console.warn(`  ⚠️ Failed to map item semantically: ${err.message}. Saving without COA mapping.`);
      }

      // Save line item
      await client.query(`
        INSERT INTO expense_items (expense_id, description, amount, mapped_account_code, confidence_score)
        VALUES ($1, $2, $3, $4, $5)
      `, [expenseId, item.description, item.amount, bestCode, score]);
    }

    // Save initial audit trail log
    await client.query(`
      INSERT INTO approval_logs (expense_id, actor_id, previous_status, new_status, comments)
      VALUES ($1, $2, NULL, 'ocr_extracted', 'Receipt processed locally via Ollama and saved to DB')
    `, [expenseId, submitterId]);

    await client.query('COMMIT');
    console.log(`\n✅ EXPENSE PIPELINE COMPLETED SUCCESSFULLY.`);
    console.log(`Transaction saved to database and linked to accounting categories. Check your Web Admin Dashboard to view it!`);

  } catch (error) {
    console.error('Error during receipt processing:', error);
    if (client) {
      await client.query('ROLLBACK').catch(() => {});
    }
  } finally {
    await client.end();
  }
}

main();
