import { Channel } from '@tauri-apps/api/core';
import { ipc, type InstallEntry, type InstallReport } from '$lib/ipc';
import type { ProgressPayload } from '$lib/types';

// Network access goes through the backend (download.rs): it only accepts the hosts the
// Hub uses, writes only inside folders the Hub manages, and retries what can be retried
// (dropped connections, server errors, rate limiting) while failing at once on a missing
// file.

const progressChannel = (onProgress?: (progress: ProgressPayload) => void): Channel<ProgressPayload> => {
    const channel = new Channel<ProgressPayload>();
    if (onProgress) channel.onmessage = onProgress;
    return channel;
};

/** Downloads `url` to `path`. `job` (see startJob) lets the download be cancelled. */
export const download = async (
    url: string,
    path: string,
    onProgress?: (progress: ProgressPayload) => void,
    attempts?: number,
    job?: string
): Promise<void> => {
    await ipc.downloadFile(url, path, progressChannel(onProgress), attempts, job);
};

export type { InstallEntry, InstallReport };

/**
 * Downloads a set of files straight into `destRoot`, a few at a time. Nothing is
 * replaced until every file has arrived, so a failure leaves the previous version whole.
 */
export const installFiles = async (
    destRoot: string,
    entries: InstallEntry[],
    onProgress?: (progress: ProgressPayload) => void,
    job?: string
): Promise<InstallReport> => {
    return await ipc.installFiles(destRoot, entries, progressChannel(onProgress), job);
};

/**
 * Reads a catalog, a database or a GitHub API response through the backend's disk cache.
 * A repeated request is conditional, so an unchanged file is not downloaded again; GitHub
 * still counts it against its hourly limit. Offline or refused, the copy kept from before
 * is returned, unless `current` asks for GitHub's answer of now. Throws
 * `HTTP error status: <code>`.
 */
export const fetchCached = async (url: string, current = false): Promise<ArrayBuffer> => {
    return await ipc.fetchCached(url, current);
};

export const fetchCachedText = async (url: string, current = false): Promise<string> => {
    return new TextDecoder().decode(await fetchCached(url, current));
};

export const fetchCachedJson = async <T>(url: string, current = false): Promise<T> => {
    return JSON.parse(await fetchCachedText(url, current)) as T;
};
