import { projectVideoPoint } from './camera.js';

const SVG = 'http://www.w3.org/2000/svg';
const readable = value => String(value).replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/g, '\ufffd');
const styles = `
[data-qr-camera-overlay] { position:absolute; overflow:hidden; pointer-events:none; z-index:1; font:500 14px/1.3 system-ui,sans-serif; color:var(--qr-scanner-text, CanvasText); }
[data-qr-camera-overlay] * { box-sizing:border-box; }
[data-qr-camera-overlay][hidden], [data-qr-camera-overlay] [hidden] { display:none!important; }
[data-qr-camera-overlay] svg { display:block; width:100%; height:100%; overflow:hidden; }
[data-qr-camera-overlay] path { fill:none; stroke:var(--qr-scanner-highlight, gold); stroke-width:3; stroke-linecap:round; stroke-linejoin:round; filter:drop-shadow(0 1px 2px rgb(0 0 0 / 35%)); }
[data-qr-camera-overlay] [data-selected="false"] path { opacity:.5; stroke-width:2; }
[data-qr-camera-overlay] [data-visible="false"] { opacity:.45; }
[data-qr-camera-overlay] polygon { fill:transparent; stroke:none; pointer-events:auto; cursor:pointer; }
[data-qr-camera-overlay] .qr-camera-controls { position:absolute; inset-inline:12px; bottom:12px; display:flex; align-items:center; flex-direction:column; gap:8px; pointer-events:none; }
[data-qr-camera-overlay] .qr-camera-targets { display:flex; gap:6px; max-width:100%; overflow:auto; padding:3px; pointer-events:auto; }
[data-qr-camera-overlay] button { font:inherit; border:1px solid currentColor; border-radius:999px; padding:7px 11px; color:var(--qr-scanner-text, CanvasText); background:var(--qr-scanner-surface, Canvas); cursor:pointer; white-space:nowrap; }
[data-qr-camera-overlay] button[aria-pressed="true"] { border-color:var(--qr-scanner-highlight, gold); outline:2px solid var(--qr-scanner-highlight, gold); outline-offset:-2px; }
[data-qr-camera-overlay] .qr-camera-pill { display:block; max-width:100%; min-height:40px; padding:10px 18px; border-radius:999px; background:var(--qr-scanner-highlight, gold); color:var(--qr-scanner-action-text, black); text-decoration:none; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; pointer-events:auto; }
[data-qr-camera-overlay] :is(a,button):focus-visible { outline:3px solid var(--qr-scanner-focus, Highlight); outline-offset:3px; }
@media (prefers-reduced-motion:no-preference) { [data-qr-camera-overlay] path { transition:opacity 120ms,stroke-width 120ms; } }
`;

/** Render measured corner brackets and explicit payload actions without a UI
 * framework. container must contain video and establish a positioned containing
 * block. The host's styles are never changed. Centered object-fit contain/cover
 * and fill are supported; mirror describes the displayed video, not the source.
 * Call update on every tracked frame (including empty frames) to expire holds.
 * Theme with --qr-scanner-highlight, --qr-scanner-surface and --qr-scanner-text. */
export function createCameraOverlay({ video, container, onSelect = () => {}, mirror = false, fit, labels = {} } = {}) {
  if (!video?.ownerDocument || !container?.contains(video))
    throw new TypeError('The overlay container must contain the video.');
  const document = video.ownerDocument, window = document.defaultView;
  if (window.getComputedStyle(container).position === 'static')
    throw new TypeError('The overlay container needs a positioned containing block (for example position: relative).');
  if (fit !== undefined && !['contain', 'cover', 'fill'].includes(fit)) throw new RangeError('Unsupported video fit.');
  const element = (name, className) => {
    const node = document.createElement(name);
    if (className) node.className = className;
    return node;
  };
  const root = element('div'); root.dataset.qrCameraOverlay = '';
  const style = element('style'); style.textContent = styles;
  const svg = document.createElementNS(SVG, 'svg'); svg.setAttribute('aria-hidden', 'true');
  const controls = element('div', 'qr-camera-controls');
  const targets = element('div', 'qr-camera-targets'); targets.setAttribute('role', 'group');
  targets.setAttribute('aria-label', labels.targets ?? 'Choose a detected code');
  const status = element('div'); status.setAttribute('aria-live', 'polite');
  controls.append(targets, status); root.append(style, svg, controls); container.append(root);
  let tracks = [], frame = null, selectedId = null, destroyed = false;
  let geometry = null;
  const buttons = new Map(), paths = new Map();
  const targetLabel = (track, index) => readable(labels.target?.(track, index) ?? `${track.detection.symbology === 'qr_code' ? 'QR' : 'Code'} ${index + 1}`);
  const selected = () => tracks.find(track => track.id === selectedId) ?? null;
  const choose = id => {
    const previous = selectedId;
    selectedId = id;
    render();
    if (selectedId !== previous) onSelect(selected());
  };
  function paint() {
    if (!geometry) return;
    for (const track of tracks) {
      const entry = paths.get(track.id);
      if (!entry) continue;
      entry.group.dataset.selected = String(track.id === selectedId);
      entry.group.dataset.visible = String(track.visible);
      if (!track.corners || track.corners.length !== 4 || track.corners.some(point => !Number.isFinite(point.x) || !Number.isFinite(point.y))) {
        entry.group.hidden = true; entry.group.setAttribute('display', 'none'); continue;
      }
      entry.group.removeAttribute('display');
      const points = track.corners.map(point => geometry.fit === 'fill'
        ? { x: (mirror ? 1 - point.x : point.x) * geometry.viewWidth, y: point.y * geometry.viewHeight }
        : projectVideoPoint(point, { ...geometry, mirror }));
      entry.hit.setAttribute('points', points.map(point => `${point.x},${point.y}`).join(' '));
      entry.path.setAttribute('d', points.map((point, index) => {
        const before = points[(index + 3) % 4], after = points[(index + 1) % 4], fraction = .22;
        return `M ${point.x + (before.x - point.x) * fraction} ${point.y + (before.y - point.y) * fraction} L ${point.x} ${point.y} L ${point.x + (after.x - point.x) * fraction} ${point.y + (after.y - point.y) * fraction}`;
      }).join(' '));
    }
  }
  function layout() {
    if (destroyed || !frame) return;
    const parent = container.getBoundingClientRect(), bounds = video.getBoundingClientRect();
    const sx = container.offsetWidth ? parent.width / container.offsetWidth : 1;
    const sy = container.offsetHeight ? parent.height / container.offsetHeight : 1;
    const computed = window.getComputedStyle(video);
    const number = key => Number.parseFloat(computed[key]) || 0;
    const left = number('borderLeftWidth') + number('paddingLeft'), top = number('borderTopWidth') + number('paddingTop');
    const width = Math.max(0, video.clientWidth - number('paddingLeft') - number('paddingRight'));
    const height = Math.max(0, video.clientHeight - number('paddingTop') - number('paddingBottom'));
    root.style.left = `${(bounds.left - parent.left) / sx + container.scrollLeft - container.clientLeft + left}px`;
    root.style.top = `${(bounds.top - parent.top) / sy + container.scrollTop - container.clientTop + top}px`;
    root.style.width = `${width}px`; root.style.height = `${height}px`;
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    const resolvedFit = fit ?? computed.objectFit;
    geometry = width > 0 && height > 0 ? { sourceWidth: frame.width, sourceHeight: frame.height,
      viewWidth: width, viewHeight: height, fit: ['contain', 'cover'].includes(resolvedFit) ? resolvedFit : 'fill' } : null;
    paint();
  }
  function render() {
    const alive = new Set(tracks.map(track => track.id));
    for (const [id, entry] of paths) if (!alive.has(id)) { entry.group.remove(); paths.delete(id); }
    for (const [id, button] of buttons) if (!alive.has(id)) { button.remove(); buttons.delete(id); }
    tracks.forEach((track, index) => {
      if (!paths.has(track.id)) {
        const group = document.createElementNS(SVG, 'g'); group.dataset.targetId = track.id;
        const hit = document.createElementNS(SVG, 'polygon'), path = document.createElementNS(SVG, 'path');
        hit.addEventListener('click', () => choose(track.id)); path.setAttribute('vector-effect', 'non-scaling-stroke');
        group.append(hit, path); svg.append(group); paths.set(track.id, { group, hit, path });
      }
      if (!buttons.has(track.id)) {
        const button = element('button'); button.type = 'button';
        button.addEventListener('click', () => choose(track.id)); buttons.set(track.id, button);
      }
      const button = buttons.get(track.id);
      button.textContent = targetLabel(track, index); button.setAttribute('aria-pressed', String(track.id === selectedId));
      button.dataset.visible = String(track.visible);
      if (targets.children[index] !== button) targets.insertBefore(button, targets.children[index] ?? null);
    });
    targets.hidden = tracks.length < 2;
    const track = selected();
    let href = null, text = track ? readable(track.detection.content.text) : '';
    const payload = track?.detection.payload;
    if (payload?.kind === 'url' || payload?.kind === 'gs1_digital_link') {
      try {
        const url = new URL(payload.url);
        if (url.protocol === 'https:' || url.protocol === 'http:') { href = url.href; text = url.hostname; }
      } catch { /* Invalid payloads remain text and never become navigable. */ }
    }
    // Avoid replacing the focused action on each camera frame.
    const tag = href ? 'A' : 'SPAN';
    let pill = status.firstElementChild;
    if (!track) status.replaceChildren();
    else {
      if (pill?.tagName !== tag) { pill = element(href ? 'a' : 'span', 'qr-camera-pill'); status.replaceChildren(pill); }
      if (href) { pill.href = href; pill.target = '_blank'; pill.rel = 'noopener noreferrer'; pill.setAttribute('aria-label', `${labels.open ?? 'Open'} ${text}`); }
      if (pill.textContent !== text) pill.textContent = text;
      pill.title = text; pill.dataset.visible = String(track.visible);
    }
    root.hidden = tracks.length === 0;
    layout();
  }
  const observer = typeof window.ResizeObserver === 'function' ? new window.ResizeObserver(layout) : null;
  observer?.observe(container); observer?.observe(video);
  window.addEventListener('resize', layout); window.addEventListener('scroll', layout, true);
  video.addEventListener('loadedmetadata', layout);
  root.hidden = true;
  return {
    get selected() { return selected(); },
    update(next, nextFrame) {
      if (destroyed) throw new Error('The camera overlay has been destroyed.');
      if (![nextFrame?.width, nextFrame?.height].every(value => Number.isFinite(value) && value > 0)) throw new RangeError('A frame needs positive dimensions.');
      tracks = [...next]; frame = nextFrame;
      if (!tracks.some(track => track.id === selectedId)) selectedId = tracks.find(track => track.visible)?.id ?? tracks[0]?.id ?? null;
      render();
    },
    select(id) { if (destroyed) return; if (tracks.some(track => track.id === id)) choose(id); },
    clear() { if (destroyed) return; tracks = []; selectedId = null; render(); },
    destroy() {
      if (destroyed) return;
      destroyed = true; tracks = []; selectedId = null;
      observer?.disconnect(); window.removeEventListener('resize', layout); window.removeEventListener('scroll', layout, true);
      video.removeEventListener('loadedmetadata', layout); root.remove(); buttons.clear(); paths.clear();
    },
  };
}
