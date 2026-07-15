import type { MessageDict } from './types';

const dict: MessageDict = {
  en: {
    'chat.open': 'Open AI chat for {name}',
    'chat.label': '🤖 Chat ({tile})',
    'chat.header': '🤖 AI · tile:{tile}',
    'chat.closeAria': 'Close chat',
    'chat.empty': 'Ask anything about the {tile} tile. Pick a quick prompt to start.',
    'chat.thinking': '⏳ Thinking…',
    'chat.aiUnavailable': '🤖 AI unavailable — {error}',
    'chat.inputPlaceholder': 'Type a question…',
    'chat.error.ai': 'AI call failed',
    'chat.error.network': 'network error',
    'chat.copy': '⧉ Copy',

    'chat.chart.rendering': 'rendering chart…',

    'chat.pin.title': 'Pin this chart to your Cockpit',
    'chat.pin.pinned': '✓ Pinned',
    'chat.pin.action': '📌 Pin to my Cockpit',
  },
  th: {
    'chat.open': 'เปิดแชท AI สำหรับ {name}',
    'chat.label': '🤖 แชท ({tile})',
    'chat.header': '🤖 AI · tile:{tile}',
    'chat.closeAria': 'ปิดแชท',
    'chat.empty': 'ถามอะไรก็ได้เกี่ยวกับ tile {tile} เลือก prompt ด่วนเพื่อเริ่มต้น',
    'chat.thinking': '⏳ กำลังคิด…',
    'chat.aiUnavailable': '🤖 AI ไม่พร้อมใช้งาน — {error}',
    'chat.inputPlaceholder': 'พิมพ์คำถาม…',
    'chat.error.ai': 'การเรียก AI ล้มเหลว',
    'chat.error.network': 'ข้อผิดพลาดเครือข่าย',
    'chat.copy': '⧉ คัดลอก',

    'chat.chart.rendering': 'กำลังเรนเดอร์แผนภูมิ…',

    'chat.pin.title': 'ปักหมุดแผนภูมินี้ไปที่ Cockpit ของคุณ',
    'chat.pin.pinned': '✓ ปักหมุดแล้ว',
    'chat.pin.action': '📌 ปักหมุดไปที่ Cockpit ของฉัน',
  },
  de: {
    'chat.open': 'KI-Chat für {name} öffnen',
    'chat.label': '🤖 Chat ({tile})',
    'chat.header': '🤖 KI · tile:{tile}',
    'chat.closeAria': 'Chat schließen',
    'chat.empty': 'Fragen Sie alles zur Kachel {tile}. Wählen Sie eine Schnellaufforderung zum Starten.',
    'chat.thinking': '⏳ Denke nach…',
    'chat.aiUnavailable': '🤖 KI nicht verfügbar — {error}',
    'chat.inputPlaceholder': 'Frage eingeben…',
    'chat.error.ai': 'KI-Aufruf fehlgeschlagen',
    'chat.error.network': 'Netzwerkfehler',
    'chat.copy': '⧉ Kopieren',

    'chat.chart.rendering': 'Diagramm wird gerendert…',

    'chat.pin.title': 'Dieses Diagramm an Ihr Cockpit anheften',
    'chat.pin.pinned': '✓ Angeheftet',
    'chat.pin.action': '📌 An mein Cockpit anheften',
  },
};

export default dict;