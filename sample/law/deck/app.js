/**
 * ==========================================================================
 * Executive AI Pitch Slide Deck - Presenter Engine (Vanilla JS)
 * ==========================================================================
 */

// ==========================================================================
// 1. Slide Navigation System
// ==========================================================================

const slides = document.querySelectorAll('.slide');
const prevBtn = document.getElementById('btn-prev');
const nextBtn = document.getElementById('btn-next');
const progressBar = document.getElementById('progress-bar');
const indicatorsContainer = document.getElementById('slide-indicators');

let currentSlideIndex = 0;
const totalSlides = slides.length;

// สร้างจุด Indicator ตามจำนวนสไลด์จริง
function buildIndicators() {
  indicatorsContainer.innerHTML = '';
  for (let i = 0; i < totalSlides; i++) {
    const dot = document.createElement('div');
    dot.className = `indicator-dot ${i === 0 ? 'active' : ''}`;
    dot.addEventListener('click', () => goToSlide(i));
    indicatorsContainer.appendChild(dot);
  }
}

// ฟังก์ชันนำทางไปยังสไลด์ที่กำหนด
window.goToSlide = function(index) {
  if (index < 0 || index >= totalSlides) return;
  
  // จัดการสถานะสไลด์เก่าและใหม่
  slides[currentSlideIndex].classList.remove('active', 'prev');
  if (index > currentSlideIndex) {
    slides[currentSlideIndex].classList.add('prev');
  }
  
  currentSlideIndex = index;
  slides[currentSlideIndex].classList.add('active');
  slides[currentSlideIndex].classList.remove('prev');
  
  // อัปเดต Indicators
  const dots = document.querySelectorAll('.indicator-dot');
  dots.forEach((dot, i) => {
    dot.classList.toggle('active', i === currentSlideIndex);
  });
  
  // ควบคุมสถานะการกดปุ่ม Prev/Next
  prevBtn.disabled = currentSlideIndex === 0;
  nextBtn.disabled = currentSlideIndex === totalSlides - 1;
  if (currentSlideIndex === totalSlides - 1) {
    nextBtn.textContent = 'สิ้นสุดการนำเสนอ';
  } else {
    nextBtn.textContent = 'ถัดไป ›';
  }
  
  // อัปเดต Progress Bar
  const progressPercent = (currentSlideIndex / (totalSlides - 1)) * 100;
  progressBar.style.width = `${progressPercent}%`;
};

// ปุ่มนำทางด่วน
prevBtn.addEventListener('click', () => goToSlide(currentSlideIndex - 1));
nextBtn.addEventListener('click', () => {
  if (currentSlideIndex < totalSlides - 1) {
    goToSlide(currentSlideIndex + 1);
  }
});

// รองรับการใช้แป้นคีย์บอร์ด (ลูกศรซ้าย/ขวา และ Spacebar)
document.addEventListener('keydown', (e) => {
  if (e.key === 'ArrowRight' || e.key === ' ') {
    e.preventDefault();
    if (currentSlideIndex < totalSlides - 1) goToSlide(currentSlideIndex + 1);
  } else if (e.key === 'ArrowLeft') {
    e.preventDefault();
    if (currentSlideIndex > 0) goToSlide(currentSlideIndex - 1);
  }
});

// ==========================================================================
// 2. Interactive LINE RAG Chat Simulation
// ==========================================================================

const chatContainer = document.getElementById('chat-container');
const queryButtons = document.querySelectorAll('.btn-demo-query');
const vecFill = document.querySelector('#vector-loading-bar .fill');
const vecStatus = document.getElementById('vector-status-text');
const vecScore = document.getElementById('vector-similarity-score');

// ฐานข้อมูลจำลองสำหรับจำลองการนำเสนอ (Mock RAG Database)
const mockDemoDatabase = {
  "1": {
    userQuery: "เช็คขอบเขตความลับในสัญญา NDA หน่อยครับ",
    similarity: "96.4%",
    botResponse: `จากการตรวจสอบเอกสาร <strong>Non-Disclosure Agreement</strong> ข้อ 3 ระบุขอบเขตความคุ้มครองความลับดังนี้ครับ:
    <br><br>
    • <strong>สิ่งที่เป็นความลับ:</strong> รหัสซอร์สโค้ดโปรแกรม, รายชื่อลูกค้า, ข้อมูลการตลาด
    <br>
    • <strong>ข้อยกเว้น:</strong> ข้อมูลที่เป็นสาธารณะอยู่แล้ว หรือ ข้อมูลที่ได้รับการเปิดเผยด้วยความยินยอมเป็นลายลักษณ์อักษรจากคู่สัญญาอีกฝ่าย
    <br><br>
    <mark class="highlight">ผู้ฝ่าฝืนมีโทษปรับตามกฎหมายความลับทางการค้า</mark>`
  },
  "2": {
    userQuery: "สัญญาเช่าร้านภูเก็ต ต้องแจ้งยกเลิกก่อนกี่วัน?",
    similarity: "92.1%",
    botResponse: `ในสัญญาเช่าพื้นที่อาคารพาณิชย์ (ร้านสาขาภูเก็ต) <strong>ข้อ 11.2</strong> ระบุเงื่อนไขการบอกเลิกสัญญาดังนี้ครับ:
    <br><br>
    คู่สัญญาฝ่ายใดฝ่ายหนึ่งที่ต้องการยกเลิกสัญญาเช่า จะต้องส่งหนังสือแจ้งเตือนเป็นลายลักษณ์อักษรแก่อีกฝ่ายหนึ่ง <mark class="highlight">ล่วงหน้าไม่น้อยกว่า 60 วัน</mark> ก่อนวันครบกำหนดสัญญา มิฉะนั้นสัญญาจะต่ออายุอัตโนมัติอีก 1 ปี`
  },
  "3": {
    userQuery: "มีเงื่อนไขเกี่ยวกับค่าปรับส่งงานช้าในสัญญาบริการไหม?",
    similarity: "89.7%",
    botResponse: `จากการตรวจสอบ <strong>สัญญาว่าจ้างบริการพัฒนาซอฟต์แวร์ ข้อ 7.4</strong> มีการระบุเงื่อนไขเบี้ยปรับกรณีส่งมอบงานล่าช้าไว้ดังนี้ครับ:
    <br><br>
    หากผู้ให้บริการไม่สามารถส่งมอบงานตามแผนงานที่กำหนด ผู้ว่าจ้างมีสิทธิ์หักเบี้ยปรับในอัตรา <mark class="highlight">0.1% ของมูลค่างวดงานนั้นๆ ต่อวัน</mark> แต่ยอดปรับสะสมสูงสุดจะต้องไม่เกิน 10% ของมูลค่าสัญญาทั้งฉบับ`
  }
};

let isDemoRunning = false;

// ตัวจัดการการจำลองแชท RAG
async function runDemoSimulation(queryId) {
  if (isDemoRunning) return;
  isDemoRunning = true;
  
  const data = mockDemoDatabase[queryId];
  if (!data) return;
  
  // ไฮไลท์ปุ่มที่กำลังรันอยู่
  queryButtons.forEach(btn => {
    btn.classList.toggle('active', btn.getAttribute('data-query') === queryId);
  });
  
  // ล้างข้อความเก่า ยกเว้นข้อความต้อนรับข้อความแรก
  const welcomeMsg = chatContainer.firstElementChild.nextElementSibling;
  chatContainer.innerHTML = '';
  chatContainer.appendChild(welcomeMsg.cloneNode(true));
  
  // 1. เพิ่มข้อความของผู้ใช้งาน (User message)
  appendChatMessage(data.userQuery, 'user');
  
  // 2. แสดงสถานะเวกเตอร์กำลังค้นหาหลังบ้าน (Simulated Back-end Search)
  vecStatus.textContent = "AI กำลังสร้าง Embedding & ค้นหาเชิงความหมาย...";
  vecScore.textContent = "กำลังรัน...";
  vecFill.style.width = "0%";
  
  // อนิเมชันแถบสถานะเวกเตอร์
  await sleep(400);
  vecFill.style.width = "40%";
  await sleep(400);
  vecFill.style.width = "85%";
  
  // 3. แสดงบับเบิ้ล AI กำลังพิมพ์ (Typing indicator)
  const typingId = appendTypingIndicator();
  await sleep(600);
  
  // โหลดแถบเวกเตอร์เสร็จสมบูรณ์
  vecFill.style.width = "100%";
  vecStatus.textContent = "ค้นหาพบชิ้นส่วนข้อความที่ตรงกันใน Postgres!";
  vecScore.textContent = data.similarity;
  
  // 4. ลบตัวพิมพ์ และพิมพ์ข้อความตอบกลับของ Bot จริง
  removeTypingIndicator(typingId);
  appendChatMessage(data.botResponse, 'bot');
  
  isDemoRunning = false;
}

// ฟังก์ชันสร้างบับเบิ้ลข้อความแชท
function appendChatMessage(text, sender) {
  const msgDiv = document.createElement('div');
  msgDiv.className = `msg ${sender === 'user' ? 'user-msg' : 'bot-msg'}`;
  msgDiv.innerHTML = text;
  
  chatContainer.appendChild(msgDiv);
  scrollChatToBottom();
}

// ฟังก์ชันแสดงตัวกำลังพิมพ์ (Typing dot)
function appendTypingIndicator() {
  const typingDiv = document.createElement('div');
  const uniqueId = `typing-${Date.now()}`;
  typingDiv.id = uniqueId;
  typingDiv.className = 'msg bot-msg typing-spinner';
  typingDiv.innerHTML = '<span></span><span></span><span></span>';
  
  chatContainer.appendChild(typingDiv);
  scrollChatToBottom();
  return uniqueId;
}

function removeTypingIndicator(id) {
  const element = document.getElementById(id);
  if (element) element.remove();
}

// ช่วยเลื่อนหน้าต่างแชทลงด้านล่างอัตโนมัติเมื่อมีบับเบิ้ลใหม่
function scrollChatToBottom() {
  chatContainer.scrollTop = chatContainer.scrollHeight;
}

// Helper: นอนหลับชั่วคราว
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// ผูกเหตุการณ์คลิกให้ปุ่ม Demo
queryButtons.forEach(btn => {
  btn.addEventListener('click', () => {
    const queryId = btn.getAttribute('data-query');
    runDemoSimulation(queryId);
  });
});

// ==========================================================================
// 3. Run on Load
// ==========================================================================
buildIndicators();
progressBar.style.width = '0%';

// ==========================================================================
// 4. Dark / Light Theme Toggle
//    - Initial value is set by the no-flash bootstrap in <head>
//    - Persist user choice in localStorage; respect system pref on first visit
//    - System pref changes are honored only if the user has not made an explicit choice
// ==========================================================================

const themeBtn = document.getElementById('btn-theme');
const themeIcon = document.getElementById('theme-icon');
const THEME_KEY = 'lawpoc-theme';
const mediaQuery = window.matchMedia('(prefers-color-scheme: light)');

function currentTheme() {
  return document.documentElement.getAttribute('data-theme') || 'dark';
}

function syncThemeIcon(theme) {
  // Show the icon of the CURRENT theme so the button reflects page state
  themeIcon.textContent = theme === 'light' ? '☀️' : '🌙';
  themeBtn.setAttribute('aria-label', theme === 'light' ? 'เปลี่ยนเป็นโหมดมืด' : 'เปลี่ยนเป็นโหมดสว่าง');
  themeBtn.setAttribute('title', theme === 'light' ? 'เปลี่ยนเป็นโหมดมืด' : 'เปลี่ยนเป็นโหมดสว่าง');
}

function applyTheme(theme, persist) {
  document.documentElement.setAttribute('data-theme', theme);
  syncThemeIcon(theme);
  if (persist) {
    try { localStorage.setItem(THEME_KEY, theme); } catch (e) { /* storage may be disabled */ }
  }
}

// Toggle on click — always persist (explicit user choice)
themeBtn.addEventListener('click', () => {
  const next = currentTheme() === 'dark' ? 'light' : 'dark';
  applyTheme(next, true);
});

// Initialize icon to match the theme already applied by the head bootstrap
syncThemeIcon(currentTheme());

// If the user hasn't explicitly chosen, follow OS-level theme changes live
mediaQuery.addEventListener('change', (e) => {
  let saved = null;
  try { saved = localStorage.getItem(THEME_KEY); } catch (err) { /* ignore */ }
  if (!saved) applyTheme(e.matches ? 'light' : 'dark', false);
});
