/** Fictional in-memory fixture; never an input to PMS commands or public publications. */
export const demoDate = '2026-10-09';
export const demoProperties = [
  { id: 'aster-city', name: 'Aster House · Bangkok', rooms: 40, location: 'Bangkok', occupancy: 78 },
  { id: 'aster-coast', name: 'Aster Coast · Hua Hin', rooms: 100, location: 'Hua Hin', occupancy: 72 },
  { id: 'aster-retreat', name: 'Aster Retreat · Chiang Mai', rooms: 200, location: 'Chiang Mai', occupancy: 81 },
] as const;
export const demoRoomTypes = [
  { id: 'standard', name: 'Standard Queen', beds: '1 Queen', size: 28, capacity: 2, price: 240000, description: 'A quiet place to slow down. Natural oak, soft linen and daylight.', amenities: ['Wi-Fi', 'Rain shower', 'Workspace', 'Air conditioning'], images: ['standard-main', 'standard-window', 'standard-bathroom'] },
  { id: 'deluxe', name: 'Deluxe Twin', beds: '2 Single', size: 36, capacity: 2, price: 320000, description: 'More room to settle in, with twin beds and a generous reading corner.', amenities: ['Wi-Fi', 'Rain shower', 'Reading corner', 'Coffee'], images: ['deluxe-main', 'deluxe-window', 'deluxe-bathroom'] },
  { id: 'suite', name: 'One-bedroom Suite', beds: '1 King', size: 60, capacity: 3, price: 580000, description: 'Separate living and sleeping spaces, with a small kitchenette.', amenities: ['Wi-Fi', 'Bathtub', 'Living room', 'Kitchenette'], images: ['suite-main', 'suite-living', 'suite-bathroom'] },
] as const;
export type DemoRoom = { id: string; number: string; typeId: string; building: string; floor: string; cleanliness: 'dirty' | 'clean' | 'inspected'; state: 'vacant' | 'reserved' | 'in_house' | 'maintenance'; guest: string; arrival: number; departure: number; due: number; dnd: boolean; task: string };
export function demoRooms(propertyId: string): DemoRoom[] {
  const count = demoProperties.find(property => property.id === propertyId)?.rooms ?? 40;
  return Array.from({ length: count }, (_, index) => {
    const floor = Math.floor(index / 10) + 1, position = index % 10 + 1;
    const state: DemoRoom['state'] = index % 17 === 0 ? 'maintenance' : index % 5 === 0 ? 'vacant' : index % 4 === 0 ? 'reserved' : 'in_house';
    const cleanliness: DemoRoom['cleanliness'] = state === 'vacant' && index % 2 === 0 ? 'dirty' : index % 7 === 0 ? 'clean' : 'inspected';
    return { id: `${propertyId}-${index}`, number: `${floor}${String(position).padStart(2, '0')}`, typeId: demoRoomTypes[index % 3].id, building: index < count / 2 ? 'Garden' : 'Courtyard', floor: String(floor), cleanliness, state, guest: ['Mali S.', 'Alex R.', 'Narin P.', 'Jamie L.', 'Corporate group'][index % 5], arrival: index % 4, departure: index % 4 + 2 + index % 3, due: index % 3 === 0 ? 480000 : 0, dnd: index % 11 === 0, task: cleanliness === 'dirty' ? 'Departure clean' : cleanliness === 'clean' ? 'Independent inspection' : state === 'maintenance' ? 'Air conditioning inspection' : '' };
  });
}
export const demoSections = [
  ['dashboard', 'Today', 'วันนี้'], ['reservations', 'Reservations', 'การจอง'], ['front-desk', 'Front desk', 'ต้อนรับ'], ['rooms', 'Rooms', 'ห้องพัก'], ['housekeeping', 'Housekeeping', 'แม่บ้าน'], ['guest-accounts', 'Guest Accounts', 'บัญชีผู้เข้าพัก'], ['cashier', 'Cashier', 'แคชเชียร์'], ['night-audit', 'Night Audit', 'ปิดวัน'], ['inventory', 'Minibar & linen', 'มินิบาร์และผ้า'], ['guests', 'Guests', 'ผู้เข้าพัก'], ['rates', 'Rates', 'ราคา'], ['reports', 'Reports', 'รายงาน'], ['settings', 'Settings', 'ตั้งค่า'], ['portal', 'Guest website', 'หน้าจอง'], ['readiness', 'Delivery & comparison', 'ฟีเจอร์และคู่เทียบ'],
] as const;
export const demoMoney = (minor: number) => new Intl.NumberFormat('en-TH', { style: 'currency', currency: 'THB', maximumFractionDigits: 0 }).format(minor / 100);
