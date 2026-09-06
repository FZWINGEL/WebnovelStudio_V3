import { invoke } from '@tauri-apps/api/core';

export interface AppCloseStatus {
  startingRequests: number;
  activeJobs: number;
  activeWorkers: number;
  pendingResults: number;
  ready: boolean;
}

export const beginAppClose = (closeId: string): Promise<void> => invoke('begin_app_close', { closeId });
export const appCloseStatus = (closeId: string): Promise<AppCloseStatus> => invoke('app_close_status', { closeId });
export const stopAppJobs = (closeId: string): Promise<AppCloseStatus> => invoke('stop_app_jobs', { closeId });
export const finishAppClose = (closeId: string): Promise<void> => invoke('finish_app_close', { closeId });
export const cancelAppClose = (closeId: string): Promise<void> => invoke('cancel_app_close', { closeId });
