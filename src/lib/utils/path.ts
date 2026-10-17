import { appConfigDir, tempDir, join } from '@tauri-apps/api/path';
import { type, arch } from '@tauri-apps/plugin-os';
import { ipc } from '$lib/ipc';
import type { HubOS } from '$lib/types';
import { get } from 'svelte/store';
import { _ } from 'svelte-i18n';
import { toaster } from '$lib/utils/toaster';
import { DescribeError, ErrorText } from '$lib/utils/errors';

// These folders cannot change while the Hub runs, and each lookup is a call to the
// backend, so every one is resolved once. A failed lookup is forgotten so the next call
// tries again.
const once = <T>(resolve: () => Promise<T>): (() => Promise<T>) => {
    let pending: Promise<T> | null = null;
    return () => {
        pending ??= resolve().catch((error) => {
            pending = null;
            throw error;
        });
        return pending;
    };
};

export const GetOS = once(async (): Promise<HubOS> => {
    const currentPlatform = await type();
    switch (currentPlatform) {
        case "linux":
            return "Linux";
        case "windows":
            return "Win";
        case "macos":
            return "Mac";
        case "ios":
        case "android":
        default:
            return "Unsupported";
    }
});

/** CPU architecture, as game releases and .NET name it. */
export type HubArch = 'x64' | 'arm64' | 'Unsupported';

export const GetArch = once(async (): Promise<HubArch> => {
    switch (await arch()) {
        case 'x86_64':
            return 'x64';
        case 'aarch64':
            return 'arm64';
        default:
            return 'Unsupported';
    }
});

// What a game build is made for: the release asset suffix ("Linux.x64") and the .NET
// runtime identifier of an experimental build ("linux-x64"). Picked from the actual OS
// and CPU: a Mac used to be given a Linux build, and an ARM machine an x64 one, which
// only failed once launched.
export const GetBuildTarget = async (): Promise<{ assetSuffix: string; rid: string; label: string }> => {
    const [os, cpu] = await Promise.all([GetOS(), GetArch()]);
    const ridOs: Record<HubOS, string> = { Win: 'win', Linux: 'linux', Mac: 'osx', Unsupported: 'unsupported' };
    return { assetSuffix: `${os}.${cpu}.zip`, rid: `${ridOs[os]}-${cpu}`, label: `${os} ${cpu}` };
};

export const GetPreferencesPath = async (): Promise<string> => {
    return await appConfigDir();
}

// Decided by the backend (hub_root_dir in main.rs), which also allows the page to use it:
// the install folder on Windows, Documents elsewhere, and a separate folder in dev builds.
export const GetRootPath = once((): Promise<string> => ipc.hubRoot());

// Shared Songs library used by every OpenTaiko instance
export const GetGlobalSongsPath = once(async (): Promise<string> => {
    return await join(await GetRootPath(), 'Songs');
});

export const GetTmpPath = async (subFolder: string | null = null): Promise<string> => {
    const tmp = await join(await GetRootPath(), 'tmp');
    return subFolder ? await join(tmp, subFolder) : tmp;
}

// Deepest path the game's source tree reaches below its own root, with headroom for
// the branch growing (it measured 134 when this was written). Windows refuses paths
// over 259 characters unless the user has enabled long paths in the registry.
const BUILD_TREE_DEPTH = 160;
const WINDOWS_MAX_PATH = 259;

// Folder an experimental build is compiled in. The name is kept as short as possible
// because every character here is taken from the room the game's own deep folders
// need: with the old `indev-<uuid>` name the build could not fit under the Windows
// path limit on any machine. When the Hub itself is installed somewhere deep, the
// system temp folder is used instead if that is shorter.
export const GetBuildPath = async (): Promise<string> => {
    const id = crypto.randomUUID().slice(0, 8);
    const preferred = await GetTmpPath(id);
    if (await GetOS() !== "Win" || preferred.length + 1 + BUILD_TREE_DEPTH <= WINDOWS_MAX_PATH) {
        return preferred;
    }
    try {
        const fallback = await join(await tempDir(), `otk-${id}`);
        if (fallback.length < preferred.length) {
            // The Hub has no access to the temp folder until this one is allowed
            await ipc.allowBuildDir(fallback);
            return fallback;
        }
    } catch (error) {
        console.warn('Temp folder unavailable, building under the Hub folder:', error);
    }
    return preferred;
}

// Leftover temporary folders are removed at startup by the backend (guard.rs)

// A relative path from a remote catalog, checked before it is joined onto a folder on
// disk: an absolute path, a drive letter or a ".." step would place the file elsewhere.
export const IsPlainRelPath = (p: string): boolean =>
    p.length > 0 && !/^([a-zA-Z]:|[\\/])/.test(p) && !p.split(/[\\/]/).some((part) => part === '..');

// Opens a folder in the system file manager. Folders only: the page has no permission to
// open paths itself, since opening a program's path runs it. A failure is reported here,
// the same way from every button that opens a folder (some used to stay silent, one
// called it an error launching the game); the caller learns of it from the result.
export const OpenFolder = async (path: string): Promise<boolean> => {
    try {
        await ipc.openFolder(path);
        return true;
    } catch (error) {
        console.error(`Could not open ${path}:`, error);
        toaster.error({
            description: get(_)('songs.error.open_folder', { values: { error: DescribeError(error) } }),
            duration: Infinity,
            meta: { details: `${path}: ${ErrorText(error)}` }
        });
        return false;
    }
}

// Pre-0.2 single-instance game folder, adopted as the first instance on upgrade
export const GetLegacyInstancePath = async (): Promise<string> => {
    return await join(await GetRootPath(), 'OpenTaiko');
}
