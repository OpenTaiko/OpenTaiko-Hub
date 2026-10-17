import { resourceDir, documentDir, homeDir, appConfigDir, tempDir, join } from '@tauri-apps/api/path';
import { type } from '@tauri-apps/plugin-os';
import { exists, readDir, remove, stat } from '@tauri-apps/plugin-fs';
import type { HubOS } from '$lib/types';

export const GetOS = async (): Promise<HubOS> => {
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
}

export const GetPreferencesPath = async (): Promise<string> => {
    return await appConfigDir();
}

export const GetRootPath = async (): Promise<string> => {
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
}

// Shared Songs library used by every OpenTaiko instance
export const GetGlobalSongsPath = async (): Promise<string> => {
    return await join(await GetRootPath(), 'Songs');
}

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
        if (fallback.length < preferred.length) return fallback;
    } catch (error) {
        console.warn('tempDir() unavailable, building under the Hub folder:', error);
    }
    return preferred;
}

// A download or build stopped by a crash, a forced quit or a power cut never reaches the
// cleanup that removes its folder, and one such folder can hold a whole game build or a
// song archive. Anything older than the longest operation the Hub runs is one of those.
// Checking the age rather than wiping everything leaves alone a folder that a second Hub
// window may still be writing to.
const STALE_TMP_MS = 6 * 60 * 60 * 1000;

const removeStale = async (dir: string, match: (name: string) => boolean): Promise<number> => {
    if (!(await exists(dir))) return 0;
    let removed = 0;
    for (const entry of await readDir(dir)) {
        if (!match(entry.name)) continue;
        const full = await join(dir, entry.name);
        try {
            const mtime = (await stat(full)).mtime;
            if (!mtime || Date.now() - mtime.getTime() < STALE_TMP_MS) continue;
            await remove(full, { recursive: true });
            removed++;
        } catch (error) {
            console.warn(`Could not remove leftover temporary folder ${full}:`, error);
        }
    }
    return removed;
}

// Names the Hub gives its temporary folders: a UUID, its first 8 digits (GetBuildPath),
// or `indev-<uuid>` from older versions. On Linux and macOS the tmp folder sits in
// ~/Documents, where the user may keep a tmp folder of their own, so nothing else is touched.
const HUB_TMP_NAME = /^(indev-)?[0-9a-f]{8}(-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})?$/i;

// Called once at startup, in the background
export const CleanStaleTmp = async (): Promise<void> => {
    try {
        let removed = await removeStale(await GetTmpPath(), (name) => HUB_TMP_NAME.test(name));
        // Experimental builds fall back to the system temp folder (see GetBuildPath)
        removed += await removeStale(await tempDir(), (name) => /^otk-[0-9a-f]{8}$/.test(name));
        if (removed > 0) console.info(`Removed ${removed} leftover temporary folder(s)`);
    } catch (error) {
        console.warn('Temporary folder cleanup failed:', error);
    }
}

// Pre-0.2 single-instance game folder, adopted as the first instance on upgrade
export const GetLegacyInstancePath = async (): Promise<string> => {
    return await join(await GetRootPath(), 'OpenTaiko');
}
