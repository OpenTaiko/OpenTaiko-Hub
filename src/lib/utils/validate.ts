// The catalogs come from the network, so their shape is checked before the Hub relies on
// it. A malformed entry is dropped and logged instead of breaking the whole list, or of
// sending a download to a path built from a missing field. Only the fields the Hub reads
// are checked, so a catalog that grows new fields keeps working.
import type { AssetCatalog, AssetInfo, AssetPrefix, AssetType, SoundtrackEntry } from '$lib/types';

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isString = (v: unknown): v is string => typeof v === 'string';
const isStringArray = (v: unknown): v is string[] => Array.isArray(v) && v.every(isString);

const keepValid = <T>(label: string, list: unknown[], valid: (entry: Record<string, unknown>) => boolean): T[] => {
    const kept = list.filter((entry) => isRecord(entry) && valid(entry)) as T[];
    if (kept.length < list.length) {
        console.warn(`Ignored ${list.length - kept.length} malformed ${label} entr${list.length - kept.length === 1 ? 'y' : 'ies'}`);
    }
    return kept;
};

export const ValidSoundtrack = (raw: unknown): SoundtrackEntry[] => {
    if (!Array.isArray(raw)) throw new Error('The soundtrack catalog is not a list');
    return keepValid<SoundtrackEntry>('soundtrack', raw, (e) =>
        isString(e.uniqueId) &&
        isString(e.tjaFolderPath) &&
        isStringArray(e.tjaFilesPath) &&
        isString(e.tjaGenreFolder) &&
        isString(e.tjaMD5) &&
        typeof e.chartSize === 'number' &&
        isString(e.chartTitle) &&
        isRecord(e.chartDifficulties) &&
        isRecord(e.chartMakers) &&
        isString(e.chartAudioFilePath)
    );
};

const ASSET_PREFIX: Record<AssetType, AssetPrefix> = { Skins: 'skin', Characters: 'chara', Puchicharas: 'chara' };

export const ValidAssetCatalog = (raw: unknown): AssetCatalog => {
    if (!isRecord(raw)) throw new Error('The asset catalog is not an object');
    const catalog = { Skins: [], Characters: [], Puchicharas: [] } as AssetCatalog;
    for (const type of Object.keys(ASSET_PREFIX) as AssetType[]) {
        const list = raw[type];
        if (!Array.isArray(list)) continue;
        const p = ASSET_PREFIX[type];
        catalog[type] = keepValid<AssetInfo>(type, list, (e) =>
            isString(e[`${p}Name`]) &&
            isString(e[`${p}Folder`]) &&
            isString(e[`${p}Version`]) &&
            typeof e[`${p}Size`] === 'number' &&
            isStringArray(e[`${p}FilesPath`])
        );
    }
    return catalog;
};
