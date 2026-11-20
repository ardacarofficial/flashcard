import { chromium } from 'playwright-core';
const exe = process.env.PW;
const b = await chromium.launch({ executablePath: exe, args: ['--no-sandbox'] });
const ctx = await b.newContext({ viewport: { width: +(process.env.W||1280), height: 800 }, locale: 'tr-TR' });
await ctx.addInitScript(() => { localStorage.setItem('fc.locale', 'tr'); });
const page = await ctx.newPage();
page.on('pageerror', (e) => console.log('ERR', e.message));
await page.goto('http://127.0.0.1:1440');
await page.waitForSelector('nav ul button');
await page.waitForTimeout(800);
const steps = JSON.parse(process.argv[2]);
for (const st of steps) {
  if (st.click) { await page.locator(st.click).first().click(); await page.waitForTimeout(500); }
  if (st.nav !== undefined) { await page.locator('nav ul button').nth(st.nav).click(); await page.waitForTimeout(500); }
  if (st.key) { await page.keyboard.press(st.key); await page.waitForTimeout(400); }
  if (st.shot) await page.screenshot({ path: st.shot });
  if (st.eval) console.log(await page.evaluate(st.eval));
}
await b.close();
