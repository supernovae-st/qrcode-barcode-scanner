/** Browser helpers: no DOM access or camera permission request at import time. */
const identity = ({ symbology, content }) => JSON.stringify([
  symbology, content.raw, content.fnc1 ?? null, content.eci ?? null,
]);

function normalizedCorners(detection, width, height) {
  const points = detection.corners;
  if (!Array.isArray(points) || points.length !== 4 ||
      points.some(p => !Number.isFinite(p.x) || !Number.isFinite(p.y))) return null;
  const area = Math.abs(points.reduce((sum, p, i) => {
    const next = points[(i + 1) % 4];
    return sum + p.x * next.y - next.x * p.y;
  }, 0)) / 2;
  // A reader's scan line is not a measured rectangle. Never invent its height.
  if (area < 1) return null;
  return points.map(p => ({ x: Math.max(0, Math.min(1, p.x / width)), y: Math.max(0, Math.min(1, p.y / height)) }));
}

/** Stabilize measured quadrilaterals between reads. Content stays engine-owned. */
export function createDetectionTracker({ holdMs = 700, smoothing = 0.35 } = {}) {
  if (!Number.isFinite(holdMs) || holdMs < 0 || !Number.isFinite(smoothing) || smoothing <= 0 || smoothing > 1)
    throw new RangeError('holdMs must be nonnegative and smoothing in (0, 1].');
  let tracks = new Map(), sequence = 0, lastTimestamp = -Infinity;
  return {
    clear() { tracks.clear(); lastTimestamp = -Infinity; },
    update(detections, { width, height, timestamp }) {
      if (![width, height, timestamp].every(Number.isFinite) || width <= 0 || height <= 0)
        throw new RangeError('A frame needs positive dimensions and a finite timestamp.');
      if (timestamp < lastTimestamp) tracks.clear();
      lastTimestamp = timestamp;
      const next = new Map();
      for (const detection of detections) {
        const key = identity(detection), previous = tracks.get(key);
        let corners = normalizedCorners(detection, width, height);
        if (corners && previous?.corners && timestamp - previous.lastSeenAt <= holdMs) {
          // Reader corner origins can rotate between frames. Align the cyclic
          // order before interpolating, including a reversed winding.
          const variants = [corners, [...corners].reverse()].flatMap(points =>
            points.map((_, shift) => points.map((__, i) => points[(i + shift) % 4])));
          const distance = points => points.reduce((sum, p, i) =>
            sum + (p.x - previous.corners[i].x) ** 2 + (p.y - previous.corners[i].y) ** 2, 0);
          corners = variants.reduce((best, points) => distance(points) < distance(best) ? points : best);
          corners = corners.map((p, i) => ({
            x: previous.corners[i].x + smoothing * (p.x - previous.corners[i].x),
            y: previous.corners[i].y + smoothing * (p.y - previous.corners[i].y),
          }));
        }
        next.set(key, { id: previous && timestamp - previous.lastSeenAt <= holdMs ? previous.id : `scan-${++sequence}`,
          detection, corners, visible: true, lastSeenAt: timestamp });
      }
      for (const [key, track] of tracks) {
        if (!next.has(key) && timestamp - track.lastSeenAt < holdMs)
          next.set(key, { ...track, visible: false });
      }
      tracks = next;
      // Give callers snapshots: later updates and edits must not mutate state.
      return [...tracks.values()].map(track => ({ ...track, corners: track.corners?.map(p => ({ ...p })) ?? null }));
    },
  };
}

/** Map a normalized engine point to a video element's CSS pixel rectangle. */
export function projectVideoPoint(point, { sourceWidth, sourceHeight, viewWidth, viewHeight, fit = 'contain', mirror = false }) {
  if (![sourceWidth, sourceHeight, viewWidth, viewHeight].every(n => Number.isFinite(n) && n > 0) ||
      !Number.isFinite(point.x) || !Number.isFinite(point.y) || !['contain', 'cover'].includes(fit))
    throw new RangeError('Invalid video geometry.');
  const scale = Math[fit === 'cover' ? 'max' : 'min'](viewWidth / sourceWidth, viewHeight / sourceHeight);
  return {
    x: (viewWidth - sourceWidth * scale) / 2 + (mirror ? 1 - point.x : point.x) * sourceWidth * scale,
    y: (viewHeight - sourceHeight * scale) / 2 + point.y * sourceHeight * scale,
  };
}

/** Permission-aware capture loop. Supply a worker-backed scanFrame callback so
 * decoding does not block animation. stop() also cancels a pending permission
 * request and discards late scan results. No URLs open automatically. */
export function createCameraScanner({ video, scanFrame, onResult, onError = () => {},
  maxDimension = 1280, intervalMs = 100, constraints = { facingMode: { ideal: 'environment' } },
  mediaDevices = globalThis.navigator?.mediaDevices } = {}) {
  if (!video || typeof scanFrame !== 'function' || typeof onResult !== 'function' ||
      !Number.isFinite(maxDimension) || maxDimension < 1 || !Number.isFinite(intervalMs) || intervalMs < 0)
    throw new TypeError('A video, scanFrame, onResult and valid capture limits are required.');
  const document = video.ownerDocument;
  let epoch = 0, stream = null, timer, frameHandle, pendingStart = null, decoding = null;
  const cancelFrame = () => {
    clearTimeout(timer);
    if (frameHandle !== undefined) video.cancelVideoFrameCallback?.(frameHandle);
    frameHandle = undefined;
  };
  const stop = () => {
    epoch++;
    cancelFrame();
    pendingStart = null;
    document?.removeEventListener('visibilitychange', onVisibility);
    document?.defaultView?.removeEventListener('pagehide', stop);
    if (stream) {
      const owned = stream;
      stream = null;
      owned.getTracks().forEach(track => { track.removeEventListener?.('ended', onEnded); track.stop(); });
      if (video.srcObject === owned) { video.pause(); video.srcObject = null; }
    }
  };
  function onVisibility() { if (document.hidden) stop(); }
  function onEnded() { stop(); onError(new Error('Camera stream ended.')); }
  return {
    stop,
    start() {
      if (pendingStart) return pendingStart;
      if (stream) return Promise.resolve();
      const token = ++epoch;
      pendingStart = (async () => {
        if (document?.hidden) return;
        if (!mediaDevices?.getUserMedia) throw new Error('Camera capture requires a secure context and mediaDevices.');
        // Install before requesting permission: leaving during the prompt must
        // invalidate its eventual answer, even after the page becomes visible.
        document?.addEventListener('visibilitychange', onVisibility);
        document?.defaultView?.addEventListener('pagehide', stop);
        const acquired = await mediaDevices.getUserMedia({ audio: false, video: constraints });
        if (epoch !== token || document?.hidden) { acquired.getTracks().forEach(track => track.stop()); return; }
        stream = acquired;
        acquired.getTracks().forEach(track => track.addEventListener?.('ended', onEnded));
        video.srcObject = acquired;
        video.muted = true;
        video.playsInline = true;
        await video.play();
        if (epoch !== token) return;
        const canvas = document.createElement('canvas');
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) throw new Error('Camera capture needs a 2D canvas.');
        const schedule = () => {
          if (epoch !== token) return;
          timer = setTimeout(() => {
            if (epoch !== token) return;
            if (video.requestVideoFrameCallback) frameHandle = video.requestVideoFrameCallback(capture);
            else void capture();
          }, intervalMs);
        };
        const capture = async () => {
          if (epoch !== token) return;
          try {
            // A worker request cannot be cancelled by stopping its media stream.
            // A restart waits for that request before issuing another one.
            if (decoding) await decoding.catch(() => {});
            if (epoch !== token) return;
            if (video.readyState >= 2 && video.videoWidth > 0 && video.videoHeight > 0) {
              const ratio = Math.min(1, maxDimension / Math.max(video.videoWidth, video.videoHeight));
              canvas.width = Math.max(1, Math.round(video.videoWidth * ratio));
              canvas.height = Math.max(1, Math.round(video.videoHeight * ratio));
              context.drawImage(video, 0, 0, canvas.width, canvas.height);
              const frame = context.getImageData(0, 0, canvas.width, canvas.height);
              const operation = Promise.resolve().then(() => scanFrame(frame.data, frame.width, frame.height));
              decoding = operation;
              let report;
              try { report = await operation; }
              finally { if (decoding === operation) decoding = null; }
              if (epoch !== token) return;
              onResult(report, { width: frame.width, height: frame.height, timestamp: performance.now() });
            }
            schedule();
          } catch (error) {
            if (epoch === token) { stop(); onError(error); }
          }
        };
        schedule();
      })().catch(error => {
        if (epoch === token) { stop(); onError(error); throw error; }
      }).finally(() => { if (epoch === token) pendingStart = null; });
      return pendingStart;
    },
  };
}
