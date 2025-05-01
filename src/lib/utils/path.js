import { resourceDir, documentDir } from '@tauri-apps/api/path';
import { type } from '@tauri-apps/plugin-os';

const GetOS = async () => {
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

export const GetRootPath = async () => {
    const _os = await GetOS();
    let _dir = null;

    switch (_os) {
        case "Win":
            {
                _dir = await resourceDir();
                break;
            }
        default: // Linux
            {
                _dir = await documentDir();
                break;
            }
    }

    return _dir;
}