import { resourceDir, documentDir, homeDir, appConfigDir, tempDir, join } from '@tauri-apps/api/path';
import { type } from '@tauri-apps/plugin-os';
import { invoke } from '@tauri-apps/api/core';
import type { HubOS } from '$lib/types';

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

export const GetPreferencesPath = async (): Promise<string> => {
    return await appConfigDir();
}

export const GetRootPath = once(async (): Promise<string> => {
    const _os = await GetOS();

    if (_os === "Win") {
        return await resourceDir();
    }

    // Linux / Mac: prefer ~/Documents, but fall back to the home directory on minimal
    // installs where the XDG user dirs (XDG_DOCUMENTS_DIR) are not configured — there
    // documentDir() throws, which would otherwise break every path in the app.
    try {
        return await documentDir();
    } catch (error) {
        console.warn('documentDir() unavailable, falling back to the home directory:', error);
        return await homeDir();
    }
});

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
            await invoke('allow_build_dir', { path: fallback });
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
// open paths itself, since opening a program's path runs it.
export const OpenFolder = async (path: string): Promise<void> => {
    await invoke('open_folder', { path });
}

// Pre-0.2 single-instance game folder, adopted as the first instance on upgrade
export const GetLegacyInstancePath = async (): Promise<string> => {
    return await join(await GetRootPath(), 'OpenTaiko');
}
