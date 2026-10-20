// The official soundtrack: its catalog, the repository's file index, and what has to be
// downloaded for one song. Kept apart from the Songs page so it can be tested on its own.
import { fetchCachedJson } from '$lib/utils/download';
import { ValidSoundtrack } from '$lib/utils/validate';
import { SOUNDTRACK_REPO, apiUrl, rawUrl } from '$lib/repos';
import type { InstallEntry } from '$lib/ipc';
import type { SoundtrackEntry } from '$lib/types';

const REPO_RAW = rawUrl(SOUNDTRACK_REPO);
const CATALOG_URL = `${REPO_RAW}/soundtrack_info.json`;
const BRANCH_URL = `${apiUrl(SOUNDTRACK_REPO)}/branches/${SOUNDTRACK_REPO.branch}`;

// Songs not offered in some regions
const REGION_HIDDEN: Record<string, string[]> = {
    'zh-CN': ['losTPEtAlSwANDERRBHLiXoUNdsetSUnaN']
};

export const FetchSoundtrackCatalog = async (language: string = navigator.language): Promise<SoundtrackEntry[]> => {
    const catalog = ValidSoundtrack(await fetchCachedJson<unknown>(CATALOG_URL));
    const hidden = REGION_HIDDEN[language] ?? [];
    return hidden.length ? catalog.filter((song) => !hidden.includes(song.uniqueId)) : catalog;
};

/** The soundtrack repository's files at one commit. */
export interface SoundtrackIndex {
    /** The commit read. A file it lists is downloaded from that same commit: from the
     *  branch, a push made since the index was read gave files that matched no SHA. */
    commit: string;
    /** repository path → git blob SHA and size */
    files: Map<string, { sha: string; size: number | null }>;
}

/** Every file of the soundtrack repository at the head of its branch. */
export const FetchSoundtrackIndex = async (): Promise<SoundtrackIndex> => {
    const branch = await fetchCachedJson<{ commit?: { sha?: string } }>(BRANCH_URL);
    const commit = branch.commit?.sha;
    if (!commit || !/^[0-9a-f]{40}$/i.test(commit)) throw new Error(`No commit in ${BRANCH_URL}`);
    const data = await fetchCachedJson<{ tree?: { type: string; path: string; sha: string; size?: number }[] }>(
        `${apiUrl(SOUNDTRACK_REPO)}/git/trees/${commit}?recursive=1`
    );
    const files = new Map(
        (data.tree ?? [])
            .filter((entry) => entry.type === 'blob')
            .map((entry): [string, { sha: string; size: number | null }] => [
                entry.path,
                { sha: entry.sha, size: typeof entry.size === 'number' ? entry.size : null }
            ])
    );
    return { commit, files };
};

/** URL of a repository path, which the catalog writes with backslashes, on the branch or
 *  at one commit. */
export const SoundtrackFileUrl = (repoPath: string, commit: string | null = null): string =>
    `${commit ? rawUrl({ ...SOUNDTRACK_REPO, branch: commit }) : REPO_RAW}/${repoPath.replace(/\\/g, '/')}`;

/** The git blob SHA and size of a repository path, and the URL to download that very file
 *  from. A path the index does not list (a song newer than it) comes from the branch,
 *  unchecked. */
export const IndexedFile = (repoPath: string, index: SoundtrackIndex | null): { url: string; gitSha: string | null; size: number | null } => {
    const file = index?.files.get(repoPath);
    return { url: SoundtrackFileUrl(repoPath, file ? index!.commit : null), gitSha: file?.sha ?? null, size: file?.size ?? null };
};

/**
 * The files of a song, placed relative to its folder. A file listed outside the song's
 * own folder keeps only its name, so it can never land elsewhere. With the repository
 * index, each file carries its git blob SHA so an unchanged local copy is kept.
 */
export const SongInstallEntries = (song: SoundtrackEntry, index: SoundtrackIndex | null): InstallEntry[] =>
    song.tjaFilesPath.map((filePath) => {
        const insideFolder = filePath.startsWith(song.tjaFolderPath + '\\') || filePath.startsWith(song.tjaFolderPath + '/');
        const localFilePath = insideFolder
            ? filePath.slice(song.tjaFolderPath.length + 1)
            : filePath.split('\\').pop() ?? filePath;
        return {
            ...IndexedFile(filePath.replace(/\\/g, '/'), index),
            relPath: localFilePath.replace(/\\/g, '/')
        };
    });

/** Every genre folder above a song, outermost first ("A", "A/B"). */
export const SongGenrePaths = (song: SoundtrackEntry): string[] =>
    song.tjaFolderPath.split('\\').slice(0, -1).map((_, i, parts) => parts.slice(0, i + 1).join('/'));
