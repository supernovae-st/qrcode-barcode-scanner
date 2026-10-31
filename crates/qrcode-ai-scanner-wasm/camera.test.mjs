import assert from 'node:assert/strict';
const { createDetectionTracker, createCameraScanner, projectVideoPoint } = await import(process.env.QRSCAN_CAMERA_MODULE ?? './camera.js');

const detection = (raw = 'a', shift = 0) => ({ symbology: 'qr_code',
  content: { raw, text: raw, charset: 'utf8' },
  corners: [{ x: 10 + shift, y: 10 }, { x: 90 + shift, y: 10 }, { x: 90 + shift, y: 90 }, { x: 10 + shift, y: 90 }] });
const tracker = createDetectionTracker({ holdMs: 700, smoothing: 0.5 });
const frame = timestamp => ({ width: 100, height: 100, timestamp });
const [first] = tracker.update([detection()], frame(0));
assert.deepEqual(first.corners[0], { x: 0.1, y: 0.1 });
const rotated = detection('a', 10);
rotated.corners.push(rotated.corners.shift());
const [next] = tracker.update([rotated], frame(10));
assert.equal(next.id, first.id);
assert.ok(Math.abs(next.corners[0].x - 0.15) < 1e-10);
assert.equal(next.detection, rotated, 'decoded content remains exactly the engine object');
assert.equal(tracker.update([], frame(709))[0].visible, false);
assert.deepEqual(tracker.update([], frame(710)), []);
const [fresh] = tracker.update([detection()], frame(711));
assert.notEqual(fresh.id, first.id);
assert.equal(tracker.update([{ ...detection(), corners: null }], frame(712))[0].corners, null);
const line = detection(); line.corners.forEach(p => p.y = 10);
assert.equal(tracker.update([line], frame(713))[0].corners, null);
assert.equal(tracker.update([detection(), { ...detection(), symbology: 'code128' }], frame(714)).length, 2);
assert.throws(() => tracker.update([], { ...frame(1), width: 0 }), RangeError);
assert.throws(() => createDetectionTracker({ smoothing: NaN }), RangeError);
assert.deepEqual(projectVideoPoint({ x: 0, y: 0 }, { sourceWidth: 200, sourceHeight: 100, viewWidth: 100, viewHeight: 100 }), { x: 0, y: 25 });
assert.deepEqual(projectVideoPoint({ x: 0, y: 0 }, { sourceWidth: 200, sourceHeight: 100, viewWidth: 100, viewHeight: 100, fit: 'cover', mirror: true }), { x: 150, y: 0 });

function fixture() {
  const document = new EventTarget();
  document.hidden = false;
  document.defaultView = new EventTarget();
  document.createElement = () => ({ getContext: () => ({ drawImage() {}, getImageData: () => ({ data: new Uint8ClampedArray(16), width: 2, height: 2 }) }) });
  const track = new EventTarget(); track.stops = 0; track.stop = () => track.stops++;
  const stream = { getTracks: () => [track] };
  const video = { ownerDocument: document, readyState: 2, videoWidth: 2, videoHeight: 2, srcObject: null,
    play: async () => {}, pause() {}, requestVideoFrameCallback(callback) { this.callback = callback; return 1; }, cancelVideoFrameCallback() { this.callback = null; } };
  return { document, track, stream, video };
}
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
// A late permission answer, including a hide/show round trip, cannot activate capture.
for (const departure of ['stop', 'hidden', 'pagehide']) {
  const f = fixture(); let grant, requests = 0;
  const session = createCameraScanner({ video: f.video, scanFrame() {}, onResult() {},
    mediaDevices: { getUserMedia: () => { requests++; return new Promise(resolve => { grant = resolve; }); } } });
  const starting = session.start();
  assert.equal(session.start(), starting, 'concurrent starts share permission request');
  if (departure === 'stop') session.stop();
  if (departure === 'pagehide') f.document.defaultView.dispatchEvent(new Event('pagehide'));
  if (departure === 'hidden') { f.document.hidden = true; f.document.dispatchEvent(new Event('visibilitychange')); f.document.hidden = false; }
  grant(f.stream); await starting;
  assert.equal(requests, 1); assert.equal(f.track.stops, 1); assert.equal(f.video.srcObject, null);
}
// A pending decode never overlaps the next frame; stopping discards its result.
{
  const f = fixture(); let resolveScan, scans = 0, results = 0;
  const session = createCameraScanner({ video: f.video, intervalMs: 0,
    mediaDevices: { getUserMedia: async () => f.stream },
    scanFrame: () => { scans++; return new Promise(resolve => { resolveScan = resolve; }); }, onResult: () => results++ });
  await session.start(); await tick(); const capture = f.video.callback; const decoding = capture();
  await tick(); assert.equal(scans, 1);
  session.stop(); resolveScan({ detections: [] }); await decoding; await tick();
  assert.equal(results, 0); assert.equal(f.video.callback, null); assert.equal(f.track.stops, 1);
}
{
  const f = fixture(); let resolveScan, scans = 0, results = 0;
  const session = createCameraScanner({ video: f.video, intervalMs: 0,
    mediaDevices: { getUserMedia: async () => f.stream },
    scanFrame: () => { scans++; return new Promise(resolve => { resolveScan = resolve; }); }, onResult: () => results++ });
  await session.start(); await tick(); const old = f.video.callback(); await tick();
  session.stop(); await session.start(); await tick(); const restarted = f.video.callback(); await tick();
  assert.equal(scans, 1, 'restart waits for old in-flight worker request');
  resolveScan({ detections: [] }); await old; await tick();
  assert.equal(scans, 2); assert.equal(results, 0);
  session.stop(); resolveScan({ detections: [] }); await restarted;
  assert.equal(results, 0);
}
// Browser revocation ends capture and surfaces the failure; play failure releases tracks.
{
  const f = fixture(), errors = [];
  const session = createCameraScanner({ video: f.video, mediaDevices: { getUserMedia: async () => f.stream }, scanFrame() {}, onResult() {}, onError: error => errors.push(error.message) });
  await session.start(); f.track.dispatchEvent(new Event('ended'));
  assert.deepEqual(errors, ['Camera stream ended.']); assert.equal(f.video.srcObject, null);
  f.video.play = async () => { throw new Error('play denied'); };
  await assert.rejects(session.start(), /play denied/);
  assert.equal(f.video.srcObject, null); assert.equal(f.track.stops, 2);
}
console.log('PASS camera helpers: exact identity, geometry, expiry, late permission, frame serialization, revocation and cleanup');
