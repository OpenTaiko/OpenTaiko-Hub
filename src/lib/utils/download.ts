import { Channel, invoke } from '@tauri-apps/api/core';
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

/** Downloads `url` to `path`. */
export const download = async (
    url: string,
    path: string,
    onProgress?: (progress: ProgressPayload) => void,
    attempts?: number
): Promise<void> => {
    await invoke('download_file', { url, path, attempts, onProgress: progressChannel(onProgress) });
};

export interface InstallEntry {
    url: string;
    /** Destination relative to the folder the files are installed into. */
    relPath: string;
    /** Upstream git blob SHA: an identical local file is kept instead of downloaded. */
    gitSha?: string | null;
    /** May be missing upstream; a 404 lists it in `absent` instead of failing. */
    optional?: boolean;
}

export interface InstallReport {
    downloaded: string[];
    unchanged: string[];
    absent: string[];
}

/**
 * Downloads a set of files straight into `destRoot`, a few at a time. Nothing is
 * replaced until every file has arrived, so a failure leaves the previous version whole.
 */
export const installFiles = async (
    destRoot: string,
    entries: InstallEntry[],
    onProgress?: (progress: ProgressPayload) => void
): Promise<InstallReport> => {
    return await invoke<InstallReport>('install_files', { destRoot, entries, onProgress: progressChannel(onProgress) });
};

/**
 * Reads a catalog, a database or a GitHub API response through the backend's disk cache.
 * A repeated request is conditional, so an unchanged file is not downloaded again and
 * does not count against GitHub's hourly API limit. Throws `HTTP error status: <code>`.
 */
export const fetchCached = async (url: string): Promise<ArrayBuffer> => {
    return await invoke<ArrayBuffer>('fetch_cached', { url });
};

export const fetchCachedText = async (url: string): Promise<string> => {
    return new TextDecoder().decode(await fetchCached(url));
};

export const fetchCachedJson = async <T>(url: string): Promise<T> => {
    return JSON.parse(await fetchCachedText(url)) as T;
};
