// Installing a game build into an instance folder. The Home page drives this and shows
// its progress through the hooks; the steps themselves live here.
import { mkdir, remove, exists } from '@tauri-apps/plugin-fs';
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
    /** Downloads (the backend retries); throws once it gave up. */
    download: (url: string, dest: string, onProgress: (percent: number | undefined) => void) => Promise<void>;
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
        await ipc.writeVersionJson(instancePath, version, os);
    } catch (error) {
        console.error('Failed to write version.json:', error);
    }
};

export interface ZipOptions {
    /** The archive holds its files at its root (a skin package), not in one top folder
     *  (a game release). */
    flat?: boolean;
    /** Drop the archive's Songs folder: songs live in the shared library. */
    stripSongs?: boolean;
}

/**
 * Downloads a zip and merges its content into `dest`, through a temporary folder of the
 * Hub's. Throws on any failure, the download giving up included, or with "Cancelled"
 * when the job was cancelled before the files started moving. The caller reports it,
 * once. A game release and a skin package both install through here: the skin install
 * used to repeat every step of it.
 */
export const InstallZip = async (url: string, dest: string, job: string, hooks: InstallHooks, options: ZipOptions = {}): Promise<void> => {
    const tmpDir = await GetTmpPath(crypto.randomUUID());
    await mkdir(tmpDir, { recursive: true });
    const zipPath = await path.join(tmpDir, 'package.zip');
    try {
        hooks.onStage('download');
        await hooks.download(url, zipPath, hooks.onProgress);

        hooks.onStage('extract');
        hooks.onProgress(undefined);
        const extractProgress = new Channel<number>();
        extractProgress.onmessage = (percent) => hooks.onProgress(percent);
        const topFolder = await ipc.unzip(zipPath, extractProgress, job);

        hooks.onStage('install');
        hooks.onProgress(undefined);
        await remove(zipPath);
        // A flat archive's files are the extraction folder's own content
        const source = options.flat ? tmpDir : topFolder;
        if (options.stripSongs) await ipc.stripSongContent(await path.join(source, 'Songs'));
        await ipc.mergeMoveDir(source, dest);
    } finally {
        // Already gone after a flat archive's move
        try { await remove(tmpDir, { recursive: true }); } catch {}
    }
};

/** A game release into an instance folder: a build's own songs are never installed. */
export const InstallReleaseZip = (url: string, instancePath: string, job: string, hooks: InstallHooks): Promise<void> =>
    InstallZip(url, instancePath, job, hooks, { stripSongs: true });
