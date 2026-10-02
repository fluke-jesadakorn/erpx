/** Fictional Thai store catalog. Prices/costs are sample minor THB, not market quotations. */
export const posDemoCompany='acme-pos-thb';
export const posDemoBranches={restaurant:'acme-pos-restaurant',retail:'acme-pos-retail'};
type DemoProduct={sku:string;name:string;subject:string;category:string;priceMinor:number;kind:'stock'|'recipe';recipe?:{sku:string;quantityMilli:number}[];image:string;barcode?:string};
const menus:[string,string,string,string,number][]=[
 ['BASIL-CHICKEN','ข้าวกะเพราไก่','Thai basil chicken on jasmine rice with a fried egg','อาหารจานเดียว',8900],
 ['BASIL-PORK','ข้าวกะเพราหมู','Thai basil minced pork on jasmine rice','อาหารจานเดียว',8900],
 ['FRIED-RICE','ข้าวผัดไข่','Thai egg fried rice with cucumber and lime','อาหารจานเดียว',7900],
 ['CHICKEN-RICE','ข้าวมันไก่','Thai poached chicken rice with sliced chicken and ginger sauce','อาหารจานเดียว',8900],
 ['PAD-THAI','ผัดไทยกุ้ง','Pad Thai noodles with shrimp, crushed peanuts and lime','เส้นและแกง',10900],
 ['NOODLES','ผัดซีอิ๊วไก่','Thai wide rice noodles stir fried with chicken and Chinese broccoli','เส้นและแกง',8900],
 ['TOM-YUM','ต้มยำกุ้ง','Thai tom yum shrimp soup in a white bowl, lemongrass and herbs','เส้นและแกง',14900],
 ['GREEN-CURRY','แกงเขียวหวานไก่','Thai green curry with chicken and eggplant in a white bowl','เส้นและแกง',12900],
 ['PORK-SKEWERS','หมูสะเต๊ะ','Thai grilled pork satay skewers with peanut sauce','ทานเล่น',9900],
 ['SPRING-ROLLS','ปอเปี๊ยะทอด','Golden Thai vegetable spring rolls with sweet chili sauce','ทานเล่น',7900],
 ['CHICKEN-WINGS','ปีกไก่ทอด','Crispy fried chicken wings on a ceramic plate','ทานเล่น',9900],
 ['FRENCH-FRIES','เฟรนช์ฟรายส์','Golden French fries in a small basket with ketchup','ทานเล่น',6900],
 ['THAI-TEA','ชาไทยเย็น','Orange Thai iced milk tea in a clear glass with ice','เครื่องดื่ม',5900],
 ['GREEN-TEA','ชาเขียวเย็น','Iced green milk tea in a clear glass','เครื่องดื่ม',5900],
 ['LATTE','ลาเต้เย็น','Iced cafe latte with espresso and milk in a clear glass','เครื่องดื่ม',6900],
 ['AMERICANO','อเมริกาโน่เย็น','Iced black Americano coffee in a clear glass','เครื่องดื่ม',5900],
 ['COCOA','โกโก้เย็น','Iced cocoa chocolate milk in a clear glass','เครื่องดื่ม',6900],
 ['LEMON-SODA','มะนาวโซดา','Fresh lime soda with lime slices and ice in a clear glass','เครื่องดื่ม',5900],
 ['ORANGE-JUICE','น้ำส้ม','Fresh orange juice in a clear glass with orange slices','เครื่องดื่ม',6900],
 ['WATER','น้ำดื่ม','Unbranded clear bottled drinking water with a blue cap','เครื่องดื่ม',2000],
 ['MANGO-RICE','ข้าวเหนียวมะม่วง','Thai mango sticky rice with coconut milk on a white plate','ขนมหวาน',9900],
 ['COCONUT-ICE','ไอศกรีมกะทิ','Thai coconut ice cream with peanuts in a small bowl','ขนมหวาน',5900],
 ['BROWNIE','บราวนี่','One rich dark chocolate brownie square on a small plate','ขนมหวาน',6900],
 ['CHEESECAKE','ชีสเค้ก','One slice of plain creamy cheesecake on a small plate','ขนมหวาน',8900],
];
const part=(sku:string,quantityMilli:number)=>({sku:`ING-${sku}`,quantityMilli});
const recipes:Record<string,{sku:string;quantityMilli:number}[]>={
 'BASIL-CHICKEN':[part('RICE',180),part('CHICKEN',120),part('VEGETABLE',30),part('EGG',1000)],
 'BASIL-PORK':[part('RICE',180),part('PORK',120),part('VEGETABLE',30)],
 'FRIED-RICE':[part('RICE',180),part('EGG',1000),part('VEGETABLE',80)],
 'CHICKEN-RICE':[part('RICE',180),part('CHICKEN',150),part('VEGETABLE',40)],
 'PAD-THAI':[part('NOODLES',180),part('SHRIMP',100),part('EGG',1000),part('VEGETABLE',60)],
 'NOODLES':[part('NOODLES',180),part('CHICKEN',100),part('VEGETABLE',80)],
 'TOM-YUM':[part('SHRIMP',150),part('VEGETABLE',80),part('LIME',30)],
 'GREEN-CURRY':[part('CHICKEN',150),part('VEGETABLE',80),part('COCONUT',150)],
 'PORK-SKEWERS':[part('PORK',180)],'SPRING-ROLLS':[part('NOODLES',80),part('VEGETABLE',150)],
 'CHICKEN-WINGS':[part('CHICKEN',200)],'FRENCH-FRIES':[part('POTATO',200)],
 'THAI-TEA':[part('TEA',20),part('MILK',150)],'GREEN-TEA':[part('GREEN-TEA',20),part('MILK',150)],
 'LATTE':[part('COFFEE',20),part('MILK',180)],'AMERICANO':[part('COFFEE',20)],
 'COCOA':[part('COCOA',30),part('MILK',180)],'LEMON-SODA':[part('LIME',50),part('SODA',200)],
 'ORANGE-JUICE':[part('ORANGE',300)],'MANGO-RICE':[part('STICKY-RICE',150),part('MANGO',180),part('COCONUT',50)],
 'COCONUT-ICE':[part('ICE-CREAM',150)],'BROWNIE':[part('BROWNIE',1000)],'CHEESECAKE':[part('CHEESECAKE',1000)],
};
export const posRestaurantProducts:DemoProduct[]=menus.map(([sku,name,subject,category,priceMinor])=>({sku:`MENU-${sku}`,name,subject,category,priceMinor,kind:sku==='WATER'?'stock':'recipe',recipe:recipes[sku],image:`menu-${sku.toLowerCase()}`}));
const retail:[string,string,string,string,number][]=[
 ['MILK','นมสด','Unbranded carton of fresh milk with a white and blue design','เครื่องดื่ม',3500],
 ['OAT-MILK','นมโอ๊ต','Unbranded beige carton of oat milk with a small oat illustration','เครื่องดื่ม',5900],
 ['SOY-MILK','นมถั่วเหลือง','Unbranded pale green carton of soy milk with soybean illustration','เครื่องดื่ม',2900],
 ['COFFEE','กาแฟพร้อมดื่ม','Unbranded brown bottled iced coffee','เครื่องดื่ม',3900],
 ['ORANGE','น้ำส้มพร้อมดื่ม','Unbranded clear bottle of orange juice','เครื่องดื่ม',3900],
 ['SPARKLING','น้ำโซดา','Unbranded clear glass bottle of sparkling water with a silver cap','เครื่องดื่ม',2500],
 ['RICE','ข้าวหอมมะลิ','Unbranded clear pouch of Thai white jasmine rice grains','ของใช้ประจำวัน',7900],
 ['NOODLES','เส้นก๋วยเตี๋ยว','Unbranded clear pouch of dry white rice noodles','ของใช้ประจำวัน',4900],
 ['SUGAR','น้ำตาล','Unbranded white paper bag of granulated sugar','ของใช้ประจำวัน',2900],
 ['COOKING-OIL','น้ำมันพืช','Unbranded clear bottle of golden vegetable cooking oil','ของใช้ประจำวัน',6500],
 ['CHIPS','มันฝรั่งทอด','Unbranded yellow bag of potato chips, a few golden chips beside it','ขนม',2900],
 ['COOKIES','คุกกี้','Unbranded kraft pouch of chocolate chip cookies with visible cookies','ขนม',4900],
 ['CRACKERS','แครกเกอร์','Unbranded red and cream box of square crackers','ขนม',2900],
 ['ALMONDS','อัลมอนด์','Unbranded clear resealable pouch of almonds','ขนม',6900],
 ['CHOCOLATE','ช็อกโกแลต','Unbranded dark brown chocolate bar wrapper with two chocolate squares','ขนม',4900],
 ['TISSUE','กระดาษทิชชู','Unbranded white and mint tissue box, one tissue emerging','ของใช้',3900],
 ['SOAP','สบู่','Unbranded lavender soap bar with plain violet paper sleeve','ของใช้',2900],
 ['SHAMPOO','แชมพู','Unbranded white shampoo bottle with green cap','ของใช้',8900],
 ['HAND-WASH','สบู่ล้างมือ','Unbranded pale pink liquid hand soap in a clear pump bottle','ของใช้',5900],
 ['DISH-WASH','น้ำยาล้างจาน','Unbranded translucent green dishwashing liquid bottle','ของใช้',4900],
];
export const posRetailProducts:DemoProduct[]=retail.flatMap(([sku,name,subject,category,price],i)=>['S','L'].map((size,j)=>({sku:`SHOP-${sku}-${size}`,name:`${name} · ${size==='S'?'เล็ก':'ใหญ่'}`,subject,category,priceMinor:price*(j+1),kind:'stock' as const,image:`shop-${sku.toLowerCase()}`,barcode:`885990${String(i*2+j+1).padStart(6,'0')}`})));
export const posImagePrompts=[...posRestaurantProducts,...posRetailProducts].filter((p,i,rows)=>rows.findIndex(r=>r.image===p.image)===i).map(p=>({id:p.image,prompt:`Use case: product-mockup. Asset type: ERPX fictional Thai POS catalog photograph. Subject: ${p.subject}. One serving or one retail package, centered on a warm off-white background, soft natural studio lighting, appetizing and realistic textures, eye-level three-quarter view, square composition, the product fills the frame. No people, no brand names, no readable text, no price, no watermark. Accurate ${p.name} product, suitable for a small product tile.`}));
export const posIngredients=[
 ['ING-RICE','ข้าวหอมมะลิ','kg',200000,6000],['ING-CHICKEN','เนื้อไก่','kg',200000,9000],['ING-PORK','เนื้อหมู','kg',200000,15000],['ING-SHRIMP','กุ้ง','kg',200000,25000],['ING-VEGETABLE','ผักและสมุนไพร','kg',200000,6000],['ING-EGG','ไข่ไก่','each',600000,500],['ING-NOODLES','เส้นก๋วยเตี๋ยว','kg',200000,6000],['ING-POTATO','มันฝรั่ง','kg',200000,8000],['ING-LIME','มะนาว','kg',100000,5000],['ING-COCONUT','กะทิ','litre',200000,8000],['ING-TEA','ชาไทย','kg',100000,30000],['ING-GREEN-TEA','ชาเขียว','kg',100000,30000],['ING-COFFEE','เมล็ดกาแฟ','kg',100000,45000],['ING-MILK','นมสำหรับเครื่องดื่ม','litre',200000,6000],['ING-COCOA','ผงโกโก้','kg',100000,25000],['ING-SODA','โซดา','litre',100000,2000],['ING-ORANGE','ส้ม','kg',200000,4000],['ING-STICKY-RICE','ข้าวเหนียว','kg',100000,5000],['ING-MANGO','มะม่วง','kg',100000,9000],['ING-ICE-CREAM','ไอศกรีมกะทิ','kg',100000,18000],['ING-BROWNIE','บราวนี่เตรียมไว้','each',600000,2500],['ING-CHEESECAKE','ชีสเค้กเตรียมไว้','each',600000,3500],
] as const;
