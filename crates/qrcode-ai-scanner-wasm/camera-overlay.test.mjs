// Browser-only geometry, interaction and cleanup proof. Uses an existing
// Playwright installation; QRSCAN_PLAYWRIGHT_MODULE can locate that module.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
const playwright = await import(process.env.QRSCAN_PLAYWRIGHT_MODULE ?? 'playwright');
const { chromium } = playwright.default ?? playwright;
const directory = process.env.QRSCAN_CAMERA_PACKAGE ? pathToFileURL(process.env.QRSCAN_CAMERA_PACKAGE + '/') : new URL('./', import.meta.url);
const files = new Map(['/camera.js', '/camera-overlay.js'].map(path => [path, new URL(path.slice(1), directory)]));
const server = createServer(async (request, response) => {
  if (request.url === '/') {
    response.setHeader('content-type', 'text/html');
    response.end('<!doctype html><html><head><style>body{margin:0}#capture{position:relative;padding:20px;width:440px;height:340px;border:4px solid;}video{display:block;width:400px;height:300px;object-fit:contain;background:#222}</style></head><body><div id="capture"><video></video></div><script type="module">import {createCameraOverlay} from "/camera-overlay.js";window.overlay=createCameraOverlay({video:document.querySelector("video"),container:document.querySelector("#capture"),onSelect:track=>window.chosen=track?.id});window.ready=true;</script></body></html>');
  } else if (files.has(request.url)) {
    response.setHeader('content-type', 'text/javascript'); response.end(await readFile(files.get(request.url)));
  } else { response.statusCode = 404; response.end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const browser = await chromium.launch({ ...(process.env.QRSCAN_BROWSER_CHANNEL ? { channel: process.env.QRSCAN_BROWSER_CHANNEL } : {}), headless: true });
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1000, height: 800 }, reducedMotion: 'reduce' });
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await page.waitForFunction(() => window.ready);
  await page.evaluate(() => {
    window.tracks = [
      { id: 'qr-one', visible: true, lastSeenAt: 0, corners: [{x:.1,y:.1},{x:.4,y:.1},{x:.4,y:.4},{x:.1,y:.4}],
        detection: {symbology:'qr_code',content:{text:'https://example.test/one',raw:'one'},payload:{kind:'url',url:'https://example.test/one'}}},
      { id: 'qr-two', visible: true, lastSeenAt: 0, corners: [{x:.6,y:.5},{x:.9,y:.5},{x:.9,y:.8},{x:.6,y:.8}],
        detection: {symbology:'qr_code',content:{text:'<img src=x onerror=alert(1)>',raw:'two'},payload:{kind:'url',url:'javascript:alert(1)'}}},
      { id:'linear',visible:true,lastSeenAt:0,corners:null,detection:{symbology:'code128',content:{text:'STOCK-4827',raw:'stock'},payload:{kind:'text'}}},
    ];
    window.overlay.update(window.tracks,{width:1600,height:800,timestamp:0});
  });
  const root = page.locator('[data-qr-camera-overlay]');
  assert.equal(await root.locator('path:visible').count(), 2, 'no invented rectangle for a geometry-less barcode');
  const bounds = await root.boundingBox(), videoBounds = await page.locator('video').boundingBox();
  assert.deepEqual(bounds, videoBounds, 'overlay follows video content inside padded and bordered container');
  const points = await root.locator('[data-target-id="qr-one"] polygon').getAttribute('points');
  assert.equal(points, '40,70 160,70 160,130 40,130', '1600×800 contain in400×300 has50px vertical letterbox');
  assert.equal(await root.locator('a').getAttribute('href'), 'https://example.test/one');
  assert.equal(await root.locator('a').getAttribute('rel'), 'noopener noreferrer');
  const second = root.getByRole('button', {name:'QR 2',exact:true});
  await second.focus(); await page.keyboard.press('Enter');
  assert.equal(await page.evaluate(() => window.chosen), 'qr-two');
  assert.equal(await second.getAttribute('aria-pressed'), 'true');
  assert.equal(await root.locator('a').count(), 0, 'javascript payload never becomes navigable');
  assert.equal(await root.locator('img').count(), 0, 'decoded content never enters HTML');
  assert.equal(await root.locator('.qr-camera-pill').textContent(), '<img src=x onerror=alert(1)>');
  await page.evaluate(() => window.overlay.update(window.tracks.map(track => ({...track,visible:false})),{width:1600,height:800,timestamp:200}));
  assert.equal(await second.getAttribute('data-visible'), 'false');
  assert.equal(await second.evaluate(node => node === document.activeElement), true, 'tracked updates retain keyboard focus');
  await page.evaluate(() => { document.querySelector('video').style.objectFit='cover'; window.dispatchEvent(new Event('resize')); });
  assert.equal(await root.locator('[data-target-id="qr-one"] polygon').getAttribute('points'), '-40,30 140,30 140,120 -40,120', 'cover applies horizontal crop');
  await page.evaluate(async () => {
    const {createCameraOverlay}=await import('/camera-overlay.js');
    window.overlay.destroy(); window.overlay=createCameraOverlay({video:document.querySelector('video'),container:document.querySelector('#capture'),fit:'cover',mirror:true});
    window.overlay.update(window.tracks,{width:1600,height:800,timestamp:300});
  });
  assert.equal(await root.locator('[data-target-id="qr-one"] polygon').getAttribute('points'), '440,30 260,30 260,120 440,120', 'mirror follows displayed camera');
  await page.evaluate(() => {
    window.overlay.select('linear'); window.overlay.update([window.tracks[2]],{width:1600,height:800,timestamp:400});
  });
  assert.equal(await root.getByRole('button').count(), 0, 'single-target chooser is hidden');
  assert.equal(await root.locator('.qr-camera-pill').textContent(), 'STOCK-4827');
  await page.evaluate(() => window.overlay.update(window.tracks,{width:1600,height:800,timestamp:500}));
  if (process.env.QRSCAN_OVERLAY_SCREENSHOT) await page.screenshot({path:process.env.QRSCAN_OVERLAY_SCREENSHOT});
  await page.evaluate(() => window.overlay.clear());
  assert.equal(await root.isVisible(), false);
  await page.evaluate(() => {window.overlay.destroy();window.overlay.destroy();window.dispatchEvent(new Event('resize'));});
  assert.equal(await root.count(), 0, 'destroy removes the overlay and observers');
  assert.equal(await page.locator('#capture').getAttribute('style'), null, 'host styles remain untouched');
  assert.deepEqual(errors, []);
  console.log('PASS camera overlay: contain/cover/mirror geometry, accessible selection, URL/HTML safety, null geometry, focus stability and cleanup');
} finally { await browser.close(); await new Promise(resolve => server.close(resolve)); }
