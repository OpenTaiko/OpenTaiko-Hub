// The Hub's preferences, in one file (settings.json in the app config folder). They used
// to be three files, one per setting; those are read once, when settings.json does not
// exist yet, and left as they are.
import { readTextFile } from '@tauri-apps/plugin-fs';
import { join } from '@tauri-apps/api/path';
import { ipc } from '$lib/ipc';
import { GetPreferencesPath } from '$lib/utils/path';

export interface HubSettings {
    theme?: string;
    /** 'dark' or 'light' */
    mode?: string;
    locale?: string;
}

const readJson = async (name: string): Promise<Record<string, unknown> | null> => {
    try {
        return JSON.parse(await readTextFile(await join(await GetPreferencesPath(), name)));
    } catch {
        return null;
    }
};

const asString = (value: unknown): string | undefined => (typeof value === 'string' ? value : undefined);

/** The three files of Hub 0.2 and earlier, as one object. */
export const FromLegacyFiles = (theme: Record<string, unknown> | null, mode: Record<string, unknown> | null, language: Record<string, unknown> | null): HubSettings => ({
    theme: asString(theme?.theme),
    mode: asString(mode?.thememode),
    locale: asString(language?.locale)
});

let loaded: Promise<HubSettings> | null = null;
let current: HubSettings = {};

export const LoadSettings = (): Promise<HubSettings> => {
    loaded ??= (async () => {
        const stored = await readJson('settings.json');
        current = stored
            ? { theme: asString(stored.theme), mode: asString(stored.mode), locale: asString(stored.locale) }
            : FromLegacyFiles(await readJson('theme.json'), await readJson('thememode.json'), await readJson('language.json'));
        // Moved over once, so the old files are not read at every start
        if (!stored && Object.values(current).some((value) => value !== undefined)) {
            ipc.writeSettingsFile('settings.json', JSON.stringify(current)).catch(() => {});
        }
        return current;
    })();
    return loaded;
};

// Saves run one after the other, each with everything known by then: two changes made in
// quick succession could otherwise land in the wrong order and lose the first
let saving: Promise<void> = Promise.resolve();

export const SaveSettings = async (change: HubSettings): Promise<void> => {
    await LoadSettings();
    current = { ...current, ...change };
    const snapshot = JSON.stringify(current);
    saving = saving.then(() => ipc.writeSettingsFile('settings.json', snapshot)).catch((error) => {
        console.error('Could not save the settings:', error);
    });
    await saving;
};
