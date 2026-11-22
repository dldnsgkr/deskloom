import type { SceneName } from '../hash';

export interface DesktopPointer { x: number; y: number; down: boolean; overDesktop: boolean }
export interface SceneContext { requestFrames(): void } // called when a paused scene must draw again
export interface Scene {
  wantsFrames(): boolean; // false: stop the loop and draw only on change
  frame(dtMs: number, nowMs: number): void;
  pointer?(p: DesktopPointer, local: { x: number; y: number }): void; // local is in window CSS px
  destroy(): void;
}
export type SceneFactory = (root: HTMLElement, ctx: SceneContext) => Scene;
// Filled in by later tasks (5, 9, 10).
export const sceneFactories: Partial<Record<SceneName, SceneFactory>> = {};
