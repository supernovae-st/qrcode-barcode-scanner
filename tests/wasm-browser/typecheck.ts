import type { ScanReport } from '@supernovae-st/qrcode-ai-scanner-wasm';
import { loadScanner, createScannerLoader, ScannerInitError, type ScannerVariant } from '@supernovae-st/qrcode-ai-scanner-wasm/auto';
import { createDetectionTracker, createCameraScanner, projectVideoPoint } from '@supernovae-st/qrcode-ai-scanner-wasm/camera';
import { createCameraOverlay } from '@supernovae-st/qrcode-ai-scanner-wasm/camera-overlay';
const scanner = await loadScanner();
const variant: ScannerVariant = scanner.variant;
const report: ScanReport = scanner.scan_image(new Uint8Array(), 'full');
const compatible = createScannerLoader({ variant: 'compatible', wasmUrl: new URL('/scanner.wasm', 'https://example.invalid') });
const error = new ScannerInitError('asset_http_failed', variant, undefined, 404);
void [report, compatible, error.code, error.status];
const tracker = createDetectionTracker();
const frame = { width: 640, height: 480, timestamp: 0 };
const video = document.createElement('video');
const overlay = createCameraOverlay({ video, container: document.createElement('div') });
overlay.update(tracker.update(report.detections, frame), frame);
const session = createCameraScanner({ video, scanFrame: (rgba, width, height) => scanner.scan_frame(new Uint8Array(rgba.buffer), width, height),
  onResult: (result, captured) => overlay.update(tracker.update(result.detections, captured), captured) });
void [session, projectVideoPoint];
