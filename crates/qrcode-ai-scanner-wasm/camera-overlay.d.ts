import type { CameraFrame, TrackedDetection } from './camera';

/** Framework-free measured corner selection. The container must contain the
 * video and use position: relative/absolute/fixed/sticky. Center object-position
 * is required. Set mirror when the displayed video is mirrored. */
export function createCameraOverlay(options: {
  video: HTMLVideoElement;
  container: HTMLElement;
  onSelect?(track: TrackedDetection | null): void;
  fit?: 'contain' | 'cover' | 'fill';
  mirror?: boolean;
  labels?: {
    targets?: string;
    open?: string;
    target?(track: TrackedDetection, index: number): string;
  };
}): {
  readonly selected: TrackedDetection | null;
  /** Supply every frame, including empty tracker output, to expire held codes. */
  update(tracks: readonly TrackedDetection[], frame: CameraFrame): void;
  select(id: string): void;
  clear(): void;
  destroy(): void;
};
