import { BedDouble, CalendarDays, CircleDollarSign, ClipboardCheck, CookingPot, CreditCard, LayoutDashboard, MoonStar, Package, ReceiptText, Settings2, ShoppingBag, Sparkles, Users, Wallet, ChartNoAxesCombined } from "lucide-react";
import { frontText } from "@/lib/front-language";
import type { ModuleTile } from "./module-tiles";

export function pmsMenu(language: "th" | "en"): ModuleTile[] {
  const tr = (en: string, th: string) => language === "th" ? th : en;
  return [
    { id:"dashboard", label:tr("Today","วันนี้"), description:tr("Your hotel, at a glance","ทุกงานสำคัญของโรงแรม"), icon:LayoutDashboard, tone:"blue" },
    { id:"reservations", label:frontText("Reservations",language), description:tr("Plan the next arrival","เตรียมพร้อมทุกการเข้าพัก"), icon:CalendarDays, tone:"violet" },
    { id:"front-desk", label:frontText("Front desk",language), description:tr("Welcome & farewell","ต้อนรับและส่งผู้เข้าพัก"), icon:Users, tone:"blue" },
    { id:"rooms", label:frontText("Rooms",language), description:tr("Every room, in view","สถานะห้องที่ดูได้ทันที"), icon:BedDouble, tone:"teal" },
    { id:"housekeeping", label:frontText("Housekeeping",language), description:tr("Make every stay shine","ดูแลห้องให้พร้อมเข้าพัก"), icon:Sparkles, tone:"teal" },
    { id:"guest-accounts", label:frontText("Guest Account",language), description:tr("Bills & guest balances","ค่าใช้จ่ายและยอดผู้เข้าพัก"), icon:ReceiptText, tone:"violet" },
    { id:"cashier", label:frontText("Cashier",language), description:tr("Cash, counted together","รับเงินและตรวจนับกะ"), icon:Wallet, tone:"amber" },
    { id:"night-audit", label:frontText("Night audit",language), description:tr("End the day with clarity","ปิดวันอย่างมั่นใจ"), icon:MoonStar, tone:"blue" },
    { id:"inventory", label:tr("Minibar & linen","มินิบาร์และผ้า"), description:tr("Supplies for every stay","ของใช้สำหรับทุกการเข้าพัก"), icon:Package, tone:"teal" },
    { id:"guests", label:frontText("Guests",language), description:tr("Know your guests","ดูแลข้อมูลผู้เข้าพัก"), icon:Users, tone:"violet" },
    { id:"reports", label:frontText("Reports",language), description:tr("See the bigger picture","มองภาพรวมธุรกิจ"), icon:ChartNoAxesCombined, tone:"blue" },
    { id:"settings", label:frontText("Settings",language), description:tr("Set up a great stay","ตั้งค่าโรงแรมให้พร้อม"), icon:Settings2, tone:"amber" },
  ];
}

export function posMenu(language: "th" | "en"): ModuleTile[] {
  const tr = (en: string, th: string) => language === "th" ? th : en;
  return [
    { id:"sale", label:frontText("Sale",language), description:tr("A smooth service starts here","เริ่มขายและบริการที่นี่"), icon:ShoppingBag, tone:"teal" },
    { id:"kitchen", label:frontText("Kitchen",language), description:tr("From order to table","ส่งทุกออเดอร์ถึงโต๊ะ"), icon:CookingPot, tone:"amber" },
    { id:"shifts", label:frontText("Cash shifts",language), description:tr("Open, count & hand over","เปิดกะ ตรวจนับ ส่งต่อ"), icon:Wallet, tone:"blue" },
    { id:"history", label:frontText("History",language), description:tr("Find any receipt","ค้นใบเสร็จและคืนเงิน"), icon:ReceiptText, tone:"violet" },
    { id:"catalog", label:tr("Catalog","สินค้าและเมนู"), description:tr("Create your next bestseller","จัดสินค้าที่ลูกค้าชอบ"), icon:Package, tone:"teal" },
    { id:"customers", label:frontText("Customers",language), description:tr("Make every visit personal","ดูแลลูกค้าทุกการมาเยือน"), icon:Users, tone:"blue" },
    { id:"reports", label:frontText("Reports",language), description:tr("Sales, costs & performance","ยอดขาย ต้นทุน และผลลัพธ์"), icon:CircleDollarSign, tone:"violet" },
    { id:"settings", label:frontText("Settings",language), description:tr("Registers & payments","ตั้งค่าจุดขายและรับเงิน"), icon:CreditCard, tone:"amber" },
    { id:"closing", label:tr("Day close","ปิดวัน"), description:tr("Review & close with confidence","ตรวจยอดและปิดวัน"), icon:ClipboardCheck, tone:"blue" },
  ];
}
