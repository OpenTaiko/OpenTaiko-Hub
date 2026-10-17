import { Channel, invoke } from '@tauri-apps/api/core';
import type { ProgressPayload } from '$lib/types';

// Downloads `url` to `path` (download.rs). Only the hosts the Hub uses are accepted, and
// `path` must be inside a folder the Hub manages.
export const download = async (
    url: string,
    path: string,
    onProgress?: (progress: ProgressPayload) => void
): Promise<void> => {
    const channel = new Channel<ProgressPayload>();
    if (onProgress) channel.onmessage = onProgress;
    await invoke('download_file', { url, path, onProgress: channel });
};
