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
  for (const width of [350, 420]) {
    const context = await browser.newContext({ viewport: { width, height: 800 }, hasTouch: true });
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
      await page.keyboard.press('Escape');
      await tooltipVisible(page, false);
      await help.tap();
      await tooltipVisible(page, true);
      const bounds = await page.locator('#spoken-punctuation-help').boundingBox();
      assert.ok(bounds && bounds.x >= 0 && bounds.x + bounds.width <= width, 'Tooltip fits horizontally');
      const content = await panel.locator('.settings-content').boundingBox();
      assert.ok(content && bounds.y >= content.y && bounds.y + bounds.height <= Math.min(800, content.y + content.height), 'Tooltip stays inside the visible scroll area');
      if (screenshotDir) await page.screenshot({ path: join(screenshotDir, `settings-${width}.png`) });
      await help.tap();
      await tooltipVisible(page, false);
      console.log(`Settings ${width}px: ${switches.length} full-size switches; tooltip hover/focus/touch/Escape passed`);
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
}
