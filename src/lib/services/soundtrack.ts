// The official soundtrack: its catalog, the repository's file index, and what has to be
// downloaded for one song. Kept apart from the Songs page so it can be tested on its own.
import { fetchCachedJson } from '$lib/utils/download';
import { ValidSoundtrack } from '$lib/utils/validate';
import type { InstallEntry } from '$lib/ipc';
import type { SoundtrackEntry } from '$lib/types';

const REPO_RAW = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main';
const CATALOG_URL = `${REPO_RAW}/soundtrack_info.json`;
const TREE_URL = 'https://api.github.com/repos/OpenTaiko/OpenTaiko-Soundtrack/git/trees/main?recursive=1';

// Songs not offered in some regions
const REGION_HIDDEN: Record<string, string[]> = {
    'zh-CN': ['losTPEtAlSwANDERRBHLiXoUNdsetSUnaN']
};

export const FetchSoundtrackCatalog = async (language: string = navigator.language): Promise<SoundtrackEntry[]> => {
    const catalog = ValidSoundtrack(await fetchCachedJson<unknown>(CATALOG_URL));
    const hidden = REGION_HIDDEN[language] ?? [];
    return hidden.length ? catalog.filter((song) => !hidden.includes(song.uniqueId)) : catalog;
};

/** repository path → git blob SHA of every file in the soundtrack repository. */
export const FetchRemoteShaMap = async (): Promise<Map<string, string>> => {
    const data = await fetchCachedJson<{ tree?: { type: string; path: string; sha: string }[] }>(TREE_URL);
    return new Map(
        (data.tree ?? [])
            .filter((entry) => entry.type === 'blob')
            .map((entry): [string, string] => [entry.path, entry.sha])
    );
};

/** URL of a repository path, which the catalog writes with backslashes. */
export const SoundtrackFileUrl = (repoPath: string): string => `${REPO_RAW}/${repoPath.replace(/\\/g, '/')}`;

/**
 * The files of a song, placed relative to its folder. A file listed outside the song's
 * own folder keeps only its name, so it can never land elsewhere. With the repository
 * index, each file carries its git blob SHA so an unchanged local copy is kept.
 */
export const SongInstallEntries = (song: SoundtrackEntry, shaMap: Map<string, string> | null): InstallEntry[] =>
    song.tjaFilesPath.map((filePath) => {
        const insideFolder = filePath.startsWith(song.tjaFolderPath + '\\') || filePath.startsWith(song.tjaFolderPath + '/');
        const localFilePath = insideFolder
            ? filePath.slice(song.tjaFolderPath.length + 1)
            : filePath.split('\\').pop() ?? filePath;
        const repoPath = filePath.replace(/\\/g, '/');
        return {
            url: SoundtrackFileUrl(repoPath),
            relPath: localFilePath.replace(/\\/g, '/'),
            gitSha: shaMap?.get(repoPath) ?? null
        };
    });

/** Every genre folder above a song, outermost first ("A", "A/B"). */
export const SongGenrePaths = (song: SoundtrackEntry): string[] =>
    song.tjaFolderPath.split('\\').slice(0, -1).map((_, i, parts) => parts.slice(0, i + 1).join('/'));
