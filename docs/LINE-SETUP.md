# LINE Setup

## 1. สร้าง LINE Official Account + Messaging API

1. ไปที่ https://developers.line.biz/console/
2. สร้าง Provider → สร้าง Channel (Messaging API)
3. ที่ Channel settings:
   - **Channel secret** → เก็บไว้ใส่ `LINE_CHANNEL_SECRET` ใน `.env`
   - **Channel access token** (issue ใหม่ long-lived) → เก็บไว้ใส่ `LINE_CHANNEL_ACCESS_TOKEN`

## 2. ตั้ง Webhook URL

1. ไปที่ Messaging API tab
2. Webhook URL = URL ของ n8n flow:
   ```
   https://n8n.jesadakorn.com/webhook/contract-rag-line
   ```
3. กด **Update** → **Verify** (ต้องเห็น Success)
4. เปิด **Use webhook** = ON

## 3. ปิด auto-reply ของ LINE Official Account

ไปที่ LINE Official Account Manager → Settings → Response settings:
- ปิด "Auto-response messages" (ถ้าเปิด LINE จะตอบเองก่อน n8n)
- ปิด "Greeting messages" (optional)

## 4. เพิ่มเพื่อน LINE bot

- QR code อยู่ใน Channel → Messaging API → QR code
- Scan ด้วย LINE app → ทดสอบส่งไฟล์ PDF หรือพิมพ์คำถาม

## 5. Webhook path เดียวจัดการทุก event

Flow `01 - LINE Bot RAG (final)` ใช้ webhook path `contract-rag-line` และ route event ตาม message type:
- **file message** → download → extract text → chunk → embed → insert
- **text `/help`** → reply เมนู
- **text อื่น ๆ** → embed query → vector search → RAG answer → reply

LINE มี webhook URL เดียวต่อ channel เลยต้อง route ใน flow เอง (ใช้ IF nodes)

## ตัวแปรที่ต้องตั้ง

ใน `.env` ของ `folio`:
```
LINE_CHANNEL_ACCESS_TOKEN=<your token>
LINE_CHANNEL_SECRET=<your secret>
```
