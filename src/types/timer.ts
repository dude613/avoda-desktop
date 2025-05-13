/**
 * Timer status enum for timer state.
 */
export enum TimerStatus {
  Stopped = "Stopped",
  Running = "Running",
  Paused = "Paused",
}

/**
 * Interface for activity data from backend.
 */
export interface ActivityData {
  key_presses: number;
  mouse_clicks: number;
}
