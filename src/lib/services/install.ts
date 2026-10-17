// Installing a game build into an instance folder. The Home page drives this and shows
// its progress through the hooks; the steps themselves live here.
import { mkdir, remove, exists, writeTextFile } from '@tauri-apps/plugin-fs';
import { path } from '@tauri-apps/api';
import { Channel } from '@tauri-apps/api/core';
import { ipc } from '$lib/ipc';
import { GetTmpPath } from '$lib/utils/path';
import type { HubOS } from '$lib/types';

export type InstallStage = 'download' | 'extract' | 'install';

export interface InstallHooks {
    /** Percent, or undefined while no percentage is known. */
    onProgress: (percent: number | undefined) => void;
    onStage: (stage: InstallStage) => void;
    /** Downloads with retries; resolves false once it gave up (and said so). */
    download: (url: string, dest: string, onProgress: (percent: number | undefined) => void) => Promise<boolean>;
}

/** The game executable's file name on this system. */
export const GameExecutable = (os: HubOS): string => (os === 'Win' ? 'OpenTaiko.exe' : 'OpenTaiko');

/**
 * A finished install must at least contain the game executable; a missing one means the
 * install was interrupted or the build output is broken.
 */
export const IsInstallComplete = async (instancePath: string, os: HubOS): Promise<boolean> =>
    await exists(await path.join(instancePath, GameExecutable(os)));

/** version.json, read by pre-0.2 Hub versions. Best-effort. */
export const WriteVersionJson = async (instancePath: string, version: string | null, os: HubOS): Promise<void> => {
    try {
        await writeTextFile(await path.join(instancePath, 'version.json'), JSON.stringify({ version, build: os }));
    } catch (error) {
        console.error('Failed to write version.json:', error);
    }
};

/**
 * Downloads a release zip and merges its content into the instance folder. Returns false
 * when the download gave up. Throws on any other failure, or with "Cancelled" when the
 * job was cancelled before the files started moving into the instance.
 */
export const InstallReleaseZip = async (url: string, instancePath: string, job: string, hooks: InstallHooks): Promise<boolean> => {
    const tmpDir = await GetTmpPath(crypto.randomUUID());
    await mkdir(tmpDir, { recursive: true });
    const zipPath = await path.join(tmpDir, 'OpenTaiko.zip');
    try {
        hooks.onStage('download');
        if (!(await hooks.download(url, zipPath, hooks.onProgress))) return false;

        hooks.onStage('extract');
        hooks.onProgress(undefined);
        const extractProgress = new Channel<number>();
        extractProgress.onmessage = (percent) => hooks.onProgress(percent);
        const sourceFolder = await ipc.unzip(zipPath, extractProgress, job);

        hooks.onStage('install');
        hooks.onProgress(undefined);
        await remove(zipPath);
        // Songs live in the shared library, so never install a build's own copies
        await ipc.stripSongContent(await path.join(sourceFolder, 'Songs'));
        await ipc.mergeMoveDir(sourceFolder, instancePath);
        return true;
    } finally {
        try { await remove(tmpDir, { recursive: true }); } catch {}
    }
};
