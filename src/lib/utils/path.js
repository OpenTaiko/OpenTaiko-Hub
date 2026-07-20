import { resourceDir, documentDir, homeDir, appConfigDir, join } from '@tauri-apps/api/path';
import { type } from '@tauri-apps/plugin-os';

export const GetOS = async () => {
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

export const GetPreferencesPath = async () => {
    return await appConfigDir();
}

export const GetRootPath = async () => {
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
export const GetGlobalSongsPath = async () => {
    return await join(await GetRootPath(), 'Songs');
}

export const GetTmpPath = async (subFolder = null) => {
    const tmp = await join(await GetRootPath(), 'tmp');
    return subFolder ? await join(tmp, subFolder) : tmp;
}

// Pre-0.2 single-instance game folder, adopted as the first instance on upgrade
export const GetLegacyInstancePath = async () => {
    return await join(await GetRootPath(), 'OpenTaiko');
}
