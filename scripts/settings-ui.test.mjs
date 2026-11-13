// Browser-only layout/interaction check. Start `npm run dev` separately.
// Native recording, credentials, and text insertion are not exercised here.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from 'playwright-core';

const baseUrl = process.env.AIRTYPE_UI_URL ?? 'http://127.0.0.1:1420';
assert.ok(['127.0.0.1', 'localhost'].includes(new URL(baseUrl).hostname), 'Use a local development server');
const screenshotDir = process.env.AIRTYPE_UI_SCREENSHOT_DIR;
if (screenshotDir) await mkdir(screenshotDir, { recursive: true });

const browser = await chromium.launch({
  executablePath: process.env.AIRTYPE_CHROME_PATH ?? '/usr/bin/google-chrome',
  headless: true,
  chromiumSandbox: true,
});

async function tooltipVisible(page, visible) {
  await page.waitForFunction((wanted) => {
    const help = document.querySelector('#spoken-punctuation-help');
    return help && help.hidden === !wanted;
  }, visible, { timeout: 5000 });
}

try {
  for (const { width, height } of [
    { width: 350, height: 400 }, { width: 420, height: 400 },
    { width: 350, height: 800 }, { width: 420, height: 800 },
  ]) {
    const context = await browser.newContext({ viewport: { width, height }, hasTouch: true });
    try {
      const page = await context.newPage();
      page.setDefaultTimeout(5000);
      await page.goto(baseUrl, { waitUntil: 'networkidle', timeout: 15000 });
      await page.getByRole('button', { name: 'Open settings' }).click();
      const panel = page.getByRole('dialog', { name: 'Settings', exact: true });
      await panel.waitFor({ state: 'visible' });
      assert.equal(await panel.getByText('Lowercase text', { exact: true }).count(), 0);

      const switches = await panel.locator('.toggle').evaluateAll((nodes) => nodes.map((node) => {
        const rect = node.getBoundingClientRect();
        const row = node.closest('.setting-row').getBoundingClientRect();
        const info = node.closest('.setting-row').querySelector('.setting-info')?.getBoundingClientRect();
        return { width: rect.width, height: rect.height, left: rect.left, right: rect.right,
          rowLeft: row.left, rowRight: row.right, textRight: info?.right };
      }));
      assert.ok(switches.length >= 4, 'Inspect all switch rows');
      for (const item of switches) {
        assert.equal(item.width, 44, `Switch width at viewport ${width}`);
        assert.equal(item.height, 24, `Switch height at viewport ${width}`);
        assert.ok(item.left >= item.rowLeft && item.right <= item.rowRight + 1, 'Switch stays inside its row');
        if (item.textRight !== undefined) assert.ok(item.textRight + 11 <= item.left, 'Text leaves space for switch');
      }

      const help = page.getByRole('button', { name: 'Help with spoken punctuation' });
      await help.scrollIntoViewIfNeeded();
      await tooltipVisible(page, false);
      await help.hover();
      await tooltipVisible(page, true);
      await page.mouse.move(0, 0);
      await tooltipVisible(page, false);
      await help.focus();
      await tooltipVisible(page, true);
      const metrics = () => page.locator('#spoken-punctuation-help').evaluate((tip) => ({
        client: tip.clientHeight, total: tip.scrollHeight, top: tip.scrollTop,
        panel: tip.closest('.settings-content').scrollTop,
        focus: document.activeElement?.getAttribute('aria-label'),
      }));
      const before = await metrics();
      if (width === 350 && height === 400) assert.ok(before.total > before.client, 'Exercise clipped help at minimum window size');
      if (before.total > before.client) {
        await page.keyboard.press('PageDown');
        const after = await metrics();
        assert.ok(after.top > 0, 'PageDown scrolls clipped tooltip');
        assert.ok(Math.abs(after.panel - before.panel) < 1, 'Tooltip scrolling must not scroll the outer panel');
        assert.equal(after.focus, 'Help with spoken punctuation', 'Keyboard scrolling preserves trigger focus');
        await page.keyboard.press('Home');
        assert.equal((await metrics()).top, 0);
        await page.keyboard.press('ArrowDown');
        assert.ok((await metrics()).top > 0, 'ArrowDown scrolls clipped help');
        await page.keyboard.press('ArrowUp');
        assert.equal((await metrics()).top, 0);
        await page.keyboard.press('End');
        const end = await metrics();
        assert.ok(end.top >= end.total - end.client - 1, 'End exposes the last help lines');
        await page.keyboard.press('PageUp');
        assert.ok((await metrics()).top < end.top, 'PageUp scrolls toward the beginning');
      }
      await page.keyboard.press('Escape');
      await tooltipVisible(page, false);
      await help.tap();
      await tooltipVisible(page, true);
      const bounds = await page.locator('#spoken-punctuation-help').boundingBox();
      assert.ok(bounds && bounds.x >= 0 && bounds.x + bounds.width <= width, 'Tooltip fits horizontally');
      const content = await panel.locator('.settings-content').boundingBox();
      assert.ok(content && bounds.y >= content.y && bounds.y + bounds.height <= Math.min(height, content.y + content.height), 'Tooltip stays inside the visible scroll area');
      if (screenshotDir) await page.screenshot({ path: join(screenshotDir, `settings-${width}x${height}.png`) });
      await help.tap();
      await tooltipVisible(page, false);
      console.log(`Settings ${width}x${height}: ${switches.length} full-size switches; tooltip pointer/focus/touch/keyboard/Escape passed`);
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
}
