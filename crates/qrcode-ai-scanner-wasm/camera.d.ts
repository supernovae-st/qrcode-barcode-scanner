import type { Detection, Point, ScanReport } from './report-types';

export interface TrackedDetection {
  id: string;
  detection: Detection;
  /** Normalized measured geometry; null when the engine only has a scan line. */
  corners: [Point, Point, Point, Point] | null;
  visible: boolean;
  lastSeenAt: number;
}
export interface CameraFrame { width: number; height: number; timestamp: number }
export function createDetectionTracker(options?: { holdMs?: number; smoothing?: number }): {
  update(detections: readonly Detection[], frame: CameraFrame): TrackedDetection[];
  clear(): void;
};
export function projectVideoPoint(point: Point, geometry: {
  sourceWidth: number; sourceHeight: number; viewWidth: number; viewHeight: number;
  fit?: 'contain' | 'cover'; mirror?: boolean;
}): Point;
export function createCameraScanner(options: {
  video: HTMLVideoElement;
  /** Prefer a module worker to keep the main thread responsive. */
  scanFrame(rgba: Uint8ClampedArray, width: number, height: number): ScanReport | Promise<ScanReport>;
  onResult(report: ScanReport, frame: CameraFrame): void;
  onError?(error: unknown): void;
  maxDimension?: number;
  intervalMs?: number;
  constraints?: MediaTrackConstraints;
  mediaDevices?: Pick<MediaDevices, 'getUserMedia'>;
}): { start(): Promise<void>; stop(): void };
