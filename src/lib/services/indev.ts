// Building the game from source: the experimental ("InDev") builds of a branch not yet
// released. The Home page drives this and shows its progress through the hooks; the steps
// themselves live here, as install.ts holds a release install.
import { mkdir, remove, exists } from '@tauri-apps/plugin-fs';
import { path } from '@tauri-apps/api';
import { Channel } from '@tauri-apps/api/core';
import { get } from 'svelte/store';
import { _ } from 'svelte-i18n';
import { ipc } from '$lib/ipc';
import { download } from '$lib/utils/download';
import { GetBuildPath, GetBuildTarget } from '$lib/utils/path';
import { fetchBranchHeadSha } from '$lib/utils/builds';
import { GAME_REPO, sourceZipUrl } from '$lib/repos';
import { isCancelled } from '$lib/stores/activity';
import { IsInstallComplete } from './install';
import type { HubOS } from '$lib/types';

export type BuildStage = 'check_sdk' | 'fetch' | 'download' | 'extract' | 'build' | 'install';

/** There is no .NET SDK 8 or later to build with. */
export class MissingDotnetSdk extends Error {}

export interface BuildHooks {
    onStage: (stage: BuildStage) => void;
    /** Percent, or undefined while no percentage is known. */
    onProgress: (percent: number | undefined) => void;
    /** Bytes of the source archive received so far, and the rate per second. */
    onTransfer: (bytes: number, perSecond: number) => void;
    /** Compiler output, a batch of lines at a time. */
    onOutput: (lines: string[]) => void;
}

/** True when `dotnet --list-sdks` lists an SDK the game builds with: it targets net8.0. */
export const HasDotnetSdk = (lines: string[]): boolean =>
    lines.some((line) => Number(line.trim().match(/^(\d+)\./)?.[1] ?? 0) >= 8);

/**
 * Builds `branch` at its current head commit and installs the result into the instance
 * folder. Returns the commit built. Throws MissingDotnetSdk without an SDK to build with,
 * "Cancelled" when the job was cancelled, and any other failure as it came. From the
 * "install" stage on, the files are being moved into the instance and the job can no
 * longer be cancelled.
 */
export const BuildAndInstall = async (instancePath: string, branch: string, os: HubOS, job: string, hooks: BuildHooks): Promise<string> => {
    let tmpDir: string | null = null;
    const output = new Channel<string[]>();
    try {
        // 1. The build requires the .NET SDK 8 or later
        hooks.onStage('check_sdk');
        hooks.onProgress(undefined);
        const sdkLines: string[] = [];
        // Output arrives in batches of lines
        const sdkChannel = new Channel<string[]>();
        sdkChannel.onmessage = (lines) => sdkLines.push(...lines);
        let sdkExitCode = -1;
        try {
            sdkExitCode = await ipc.listDotnetSdks(sdkChannel, job);
        } catch (error) {
            if (isCancelled(error)) throw error;
        }
        if (sdkExitCode !== 0 || !HasDotnetSdk(sdkLines)) throw new MissingDotnetSdk();

        // 2. Resolve the branch head so the source and the update check are consistent
        hooks.onStage('fetch');
        const sha = await fetchBranchHeadSha(branch);
        if (!sha) throw new Error(get(_)('home.error.branch_head'));

        // 3. Download the source archive of that exact commit
        hooks.onStage('download');
        hooks.onProgress(undefined);
        hooks.onTransfer(0, 0);
        tmpDir = await GetBuildPath();
        await mkdir(tmpDir, { recursive: true });
        const srcZip = await path.join(tmpDir, 'src.zip');
        await download(
            sourceZipUrl(GAME_REPO, sha),
            srcZip,
            (pr) => {
                // codeload builds the archive on the fly and streams it chunked, so it
                // sends no Content-Length and `total` is 0. A percentage is impossible;
                // bytes and rate are the only way to tell this stage from a hang.
                // progressTotal is the backend's own running count, so it also resets
                // correctly when a retry restarts the transfer.
                hooks.onTransfer(pr.progressTotal, pr.transferSpeed);
                hooks.onProgress(pr.total ? 100 * (pr.progressTotal / pr.total) : undefined);
            },
            3,
            job
        );

        // 4. Extract
        hooks.onStage('extract');
        hooks.onProgress(0);
        const extractProgress = new Channel<number>();
        extractProgress.onmessage = (percent) => hooks.onProgress(percent);
        const extracted = await ipc.unzip(srcZip, extractProgress, job);
        await remove(srcZip);
        // The archive unpacks into `OpenTaiko-<40 character commit>`. Every character
        // of that name counts against the Windows path limit for the whole build, so
        // it is moved to a one-letter folder before the compiler sees it.
        const srcFolder = await path.join(tmpDir, 's');
        await ipc.mergeMoveDir(extracted, srcFolder);

        // 5. Build. The backend decides the command (procs.rs), and puts the game in `p`
        // beside the source folder
        hooks.onStage('build');
        hooks.onProgress(undefined);
        const rid = (await GetBuildTarget()).rid;
        output.onmessage = (lines) => hooks.onOutput(lines);
        const publishDir = await path.join(tmpDir, 'p');
        const exitCode = await ipc.publishGame(srcFolder, rid, output, job);
        if (exitCode !== 0) {
            throw new Error(get(_)('home.error.build_exit', { values: { code: exitCode } }));
        }

        // 6. Install the publish output (and local docs when the branch ships them)
        hooks.onStage('install');
        // A local publish carries the repository's whole Songs tree; drop those charts so
        // the instance uses only the shared library
        await ipc.stripSongContent(await path.join(publishDir, 'Songs'));
        await ipc.mergeMoveDir(publishDir, instancePath);
        if (!(await IsInstallComplete(instancePath, os))) {
            throw new Error(get(_)('home.error.incomplete_build'));
        }
        const docsDir = await path.join(srcFolder, 'OpenTaiko', 'docs');
        if (await exists(docsDir)) {
            await ipc.mergeMoveDir(docsDir, await path.join(instancePath, 'docs'));
        }
        return sha;
    } finally {
        // Lines a cancelled build still sends are dropped rather than written into the
        // log after the run, or into the next run's
        output.onmessage = () => {};
        if (tmpDir) {
            try { await remove(tmpDir, { recursive: true }); } catch {}
        }
    }
};
