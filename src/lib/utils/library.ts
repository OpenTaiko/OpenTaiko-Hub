// The song library's data model.
//
// One tree merges two sources: the charts scanned on disk, and the charts the OpenTaiko
// soundtrack offers. They share a tree because a catalog entry's `tjaFolderPath` IS the
// path it installs to, relative to the shared Songs folder — RunDownloadSong joins
// exactly that onto GetGlobalSongsPath().
//
// The counts here are deliberately independent of any filter the UI applies: "12 of 47"
// has to mean the same thing whatever is on screen, or the meter is a lie.

import type {
    CourseName,
    LocalSong,
    ScannedGenre,
    ScannedSong,
    SoundtrackEntry,
    TowerSide
} from '$lib/types';

/** The five courses a normal chart has, in the order the game lists them. */
export const MAIN_COURSES: CourseName[] = ['Easy', 'Normal', 'Hard', 'Oni', 'Edit'];
export const ALL_COURSES: CourseName[] = [...MAIN_COURSES, 'Tower', 'Dan'];

export type EntryStatus = 'available' | 'installed' | 'outdated' | 'custom';

/** What the scope switch above the list is showing. Never changes the counts. */
export type Scope = 'all' | 'installed' | 'updates';

export interface LibraryEntry {
    /** Stable per row: the install path on disk, else the catalog id. */
    key: string;
    label: string;
    subtitle: string | null;
    /** Folder of this chart relative to the shared Songs folder, "/" separated. */
    relPath: string;
    status: EntryStatus;
    installed: boolean;
    /** Levels per course; a course the chart lacks is simply absent. */
    levels: Partial<Record<CourseName, number>>;
    side: TowerSide | null;
    catalog: SoundtrackEntry | null;
    local: LocalSong | null;
    /** Download size in MB. Only meaningful for something not yet on disk: the scan
     *  reports no byte sizes, so this must never be shown as "size on disk". */
    downloadMb: number;
}

export interface LibraryFolder {
    name: string;
    /** box.def title when there is one, else the folder name with its sort prefix cut. */
    label: string;
    /** The raw on-disk name, kept so it can be shown and copied. */
    rawName: string;
    /** What the sort prefix says this collection is. */
    kind: CollectionKind;
    relPath: string;
    children: Map<string, LibraryFolder>;
    entries: LibraryEntry[];
    /** Totals over every descendant. Never affected by a scope or a search. */
    installedCount: number;
    totalCount: number;
    outdatedCount: number;
    /** MB still to download under here, updates included. */
    pendingMb: number;
    /** Charts still to download under here, updates included. Computed once with the
     *  totals so a bar does not have to walk its subtree on every render. */
    pendingCount: number;
    existsLocally: boolean;
    /** The scanner found this exact folder on disk — true even when it holds no charts,
     *  which is how a folder the user just made stays visible. */
    onDisk: boolean;
    /** The soundtrack owns this folder, at any depth. Marked from every ancestor of every
     *  catalog path, so "L1 Collaborations" and "E02 Events" count even when no chart
     *  sits directly in them. The Hub must never restructure or delete one. */
    official: boolean;
    /** Dan and Tower collections take user content even though the soundtrack owns them.
     *  Derived from the courses their charts actually use rather than from folder names,
     *  which differ between installs. Inherited by everything underneath. */
    special: 'dan' | 'tower' | null;
    /** A subfolder may be created here. */
    canAddFolder: boolean;
    /** A chart may be dropped here. Dan charts live in per-dan subfolders, so the Dan
     *  root itself takes folders but not charts. */
    canAddCharts: boolean;
}

export type CollectionKind = 'chapter' | 'dojo' | 'collab' | 'event' | 'extra' | 'plain';

/**
 * Titles carry the game's inline colour markup, e.g. `<c.#d3d3d3>名人</c>`. The Hub shows
 * names as plain text, so the tags are removed for display only — the file keeps them,
 * and the box.def editor still shows and edits the raw value.
 */
export const stripColorTags = (text: string): string =>
    text.replace(/<\/?c(?:\.[^>]*)?>/gi, '').trim();

/**
 * The library's folders are named with the game's sort prefixes ("01 OpenTaiko Chapter I",
 * "S1 Dan-i Dojo", "L1 Collaborations"). They order the list in-game and mean nothing to a
 * newcomer, so they are parsed off for display and kept as a kind.
 */
export const readPrefix = (name: string): { kind: CollectionKind; label: string } => {
    const m = /^([0-9]{1,2}|[SLEC][0-9]{0,2})[\s._-]+(.*)$/.exec(name);
    if (!m) return { kind: 'plain', label: name };
    const [, token, rest] = m;
    if (!rest.trim()) return { kind: 'plain', label: name };
    const kind: CollectionKind =
        /^[0-9]+$/.test(token) ? 'chapter'
        : token.startsWith('S') ? 'dojo'
        : token.startsWith('L') ? 'collab'
        : token.startsWith('E') ? 'event'
        : 'extra';
    return { kind, label: rest.trim() };
};

const makeFolder = (name: string, relPath: string): LibraryFolder => {
    const { kind, label } = readPrefix(name);
    return {
        name,
        label: stripColorTags(label),
        rawName: name,
        kind,
        relPath,
        children: new Map(),
        entries: [],
        installedCount: 0,
        totalCount: 0,
        outdatedCount: 0,
        pendingMb: 0,
        pendingCount: 0,
        existsLocally: false,
        onDisk: false,
        official: false,
        special: null,
        canAddFolder: false,
        canAddCharts: false
    };
};

const folderAt = (root: LibraryFolder, relPath: string): LibraryFolder => {
    if (!relPath) return root;
    let node = root;
    let walked = '';
    for (const part of relPath.split('/')) {
        if (!part) continue;
        walked = walked ? `${walked}/${part}` : part;
        let child = node.children.get(part);
        if (!child) {
            child = makeFolder(part, walked);
            node.children.set(part, child);
        }
        node = child;
    }
    return node;
};

const parentOf = (relPath: string): string =>
    relPath.includes('/') ? relPath.slice(0, relPath.lastIndexOf('/')) : '';

/** The soundtrack stores paths with backslashes; on disk the Hub uses "/". */
export const catalogRelPath = (entry: SoundtrackEntry): string =>
    entry.tjaFolderPath.replace(/\\/g, '/');

const scannedLevels = (song: ScannedSong): Partial<Record<CourseName, number>> => {
    const levels: Partial<Record<CourseName, number>> = {};
    for (const d of song.difficulties ?? []) levels[d.course as CourseName] = d.level;
    return levels;
};

export interface BuildInput {
    scanned: ScannedSong[];
    genres: Record<string, ScannedGenre>;
    catalog: SoundtrackEntry[];
    catalogById: Map<string, SoundtrackEntry>;
    /** uniqueId → the chart as installed. Written the moment a download finishes, so it
     *  is what makes a freshly downloaded row stop reading as "available" before the
     *  next full rescan. */
    installed: Record<string, LocalSong>;
    /** Resolves a display name, so the caller owns localisation. */
    labelFor: (entry: SoundtrackEntry | null, fallback: string) => string;
    subtitleFor?: (entry: SoundtrackEntry) => string | null;
}

export const buildLibraryTree = (input: BuildInput): LibraryFolder => {
    const { scanned, genres, catalog, catalogById, installed, labelFor, subtitleFor } = input;
    const root = makeFolder('', '');

    for (const [relPath, genre] of Object.entries(genres)) {
        const node = folderAt(root, relPath);
        node.onDisk = true;
        if (genre.title) node.label = stripColorTags(genre.title);
    }

    // Mark every ancestor of every catalog chart as the soundtrack's. Deriving this from
    // tjaGenreFolder alone missed folders whose charts all live one level deeper.
    for (const entry of catalog) {
        const parts = catalogRelPath(entry).split('/');
        parts.pop();
        let walked = '';
        for (const part of parts) {
            walked = walked ? `${walked}/${part}` : part;
            folderAt(root, walked).official = true;
        }
    }

    // Charts on disk first: a chart that exists always wins over its catalog entry.
    const seen = new Set<string>();
    for (const song of scanned) {
        const entry = song.uniqueId ? catalogById.get(song.uniqueId) ?? null : null;
        if (song.uniqueId) seen.add(song.uniqueId);

        // A finished download writes the new hash into `installed` but the scan list still
        // holds the old one until the next rescan, so the freshest source wins. Without
        // this a chart stays on "Update" forever after being updated.
        const local = song.uniqueId ? installed[song.uniqueId] ?? null : null;
        const hashes = local?.chartMD5s?.length ? local.chartMD5s : song.tjaMd5s ?? [];

        const status: EntryStatus = !entry
            ? 'custom'
            : hashes.includes(entry.tjaMD5) ? 'installed' : 'outdated';

        folderAt(root, parentOf(song.relPath)).entries.push({
            key: song.relPath,
            label: stripColorTags(labelFor(entry, song.title ?? song.relPath.split('/').pop() ?? song.relPath)),
            subtitle: entry && subtitleFor ? stripColorTags(subtitleFor(entry) ?? '') || null : null,
            relPath: song.relPath,
            status,
            installed: true,
            levels: scannedLevels(song),
            side: song.side,
            catalog: entry,
            local,
            downloadMb: status === 'outdated' ? entry?.chartSize ?? 0 : 0
        });
    }

    for (const entry of catalog) {
        if (seen.has(entry.uniqueId)) continue;

        // A download that has just finished is in `installed` but not yet in the scan.
        // Without this the row the user just clicked stays faded until a full rescan.
        const justInstalled = installed[entry.uniqueId] !== undefined;
        const relPath = justInstalled
            ? installed[entry.uniqueId].chartRelativePath.replace(/\\/g, '/')
            : catalogRelPath(entry);

        folderAt(root, parentOf(relPath)).entries.push({
            key: entry.uniqueId,
            label: stripColorTags(labelFor(entry, entry.chartTitle)),
            subtitle: subtitleFor ? stripColorTags(subtitleFor(entry) ?? '') || null : null,
            relPath,
            status: justInstalled ? 'installed' : 'available',
            installed: justInstalled,
            levels: entry.chartDifficulties,
            side: null,
            catalog: entry,
            local: installed[entry.uniqueId] ?? null,
            downloadMb: justInstalled ? 0 : entry.chartSize ?? 0
        });
    }

    tally(root);
    grantEdits(root, null, 0);
    return root;
};

/** True when any chart under here uses a course only that collection type has. */
const usesCourse = (node: LibraryFolder, course: CourseName): boolean => {
    for (const entry of node.entries) {
        if (entry.levels[course] !== undefined) return true;
    }
    for (const child of node.children.values()) {
        if (usesCourse(child, course)) return true;
    }
    return false;
};

/**
 * Works out what may be edited where.
 *
 * The soundtrack's collections are off limits: the Hub keeps them in sync and
 * restructuring one would fight the next download. Two named exceptions, and only at the
 * top of each collection:
 *   - the Dan collection takes new subfolders (a dan is a folder), but no charts of its
 *     own, and its existing dan folders are left alone;
 *   - the Tower collection takes new subfolders too, and songs go into the tower folders
 *     that already exist — never loose at the collection root.
 * Anything the user made is theirs entirely.
 */
const grantEdits = (node: LibraryFolder, inherited: 'dan' | 'tower' | null, depth: number): void => {
    let special = inherited;
    if (special === null && node.official && depth === 1) {
        if (usesCourse(node, 'Dan')) special = 'dan';
        else if (usesCourse(node, 'Tower')) special = 'tower';
    }
    node.special = special;

    if (!node.official) {
        // A folder the user made, wherever it sits.
        node.canAddFolder = true;
        node.canAddCharts = true;
    } else if (depth === 1 && (special === 'dan' || special === 'tower')) {
        // The top of each collection takes new folders — a dan is a folder and so is a
        // tower — but never loose charts of its own.
        node.canAddFolder = true;
        node.canAddCharts = false;
    } else if (special === 'tower') {
        // Songs go into the tower folders that are already there.
        node.canAddFolder = false;
        node.canAddCharts = true;
    } else {
        // Every other soundtrack folder, the existing dan folders included: untouched.
        node.canAddFolder = false;
        node.canAddCharts = false;
    }

    for (const child of node.children.values()) grantEdits(child, special, depth + 1);
};

const tally = (node: LibraryFolder): void => {
    node.installedCount = 0;
    node.totalCount = 0;
    node.outdatedCount = 0;
    node.pendingMb = 0;
    node.pendingCount = 0;
    node.existsLocally = node.onDisk;

    for (const entry of node.entries) {
        node.totalCount++;
        if (entry.installed) {
            node.installedCount++;
            node.existsLocally = true;
        }
        if (entry.status === 'outdated') node.outdatedCount++;
        if (entry.catalog && (!entry.installed || entry.status === 'outdated')) node.pendingCount++;
        node.pendingMb += entry.downloadMb;
    }
    for (const child of node.children.values()) {
        tally(child);
        node.totalCount += child.totalCount;
        node.installedCount += child.installedCount;
        node.outdatedCount += child.outdatedCount;
        node.pendingMb += child.pendingMb;
        node.pendingCount += child.pendingCount;
        if (child.existsLocally) node.existsLocally = true;
    }
};

/**
 * Everything under a folder that a download would fetch: charts not installed AND charts
 * whose upstream copy changed. Counting only the missing ones understates a folder whose
 * charts are all present but stale.
 */
export const pendingUnder = (node: LibraryFolder): SoundtrackEntry[] => {
    const out: SoundtrackEntry[] = [];
    const walk = (folder: LibraryFolder) => {
        for (const entry of folder.entries) {
            if (entry.catalog && (!entry.installed || entry.status === 'outdated')) out.push(entry.catalog);
        }
        for (const child of folder.children.values()) walk(child);
    };
    walk(node);
    return out;
};

const matchesScope = (entry: LibraryEntry, scope: Scope): boolean => {
    if (scope === 'installed') return entry.installed;
    if (scope === 'updates') return entry.status === 'outdated';
    return true;
};

/**
 * Produces the rows to display. It never touches the counts: a folder keeps the totals
 * computed over the whole library, and only its `entries` are narrowed. Returns null when
 * nothing under this folder survives.
 */
export const viewOf = (
    node: LibraryFolder,
    scope: Scope,
    needle: string,
    extraTerms: (entry: LibraryEntry) => string[] = () => []
): LibraryFolder | null => {
    const term = needle.trim().toLowerCase();
    const folderMatches = term.length > 0
        && (node.label.toLowerCase().includes(term) || node.rawName.toLowerCase().includes(term));

    const entries = node.entries.filter((entry) => {
        if (!matchesScope(entry, scope)) return false;
        if (!term || folderMatches) return true;
        return entry.label.toLowerCase().includes(term)
            || (entry.subtitle?.toLowerCase().includes(term) ?? false)
            || extraTerms(entry).some((t) => t.includes(term));
    });

    const children = new Map<string, LibraryFolder>();
    for (const [name, child] of node.children) {
        const kept = viewOf(child, scope, folderMatches ? '' : needle, extraTerms);
        if (kept) children.set(name, kept);
    }

    // An empty folder on disk is a real thing the user made and must stay visible — but
    // not under Updates, where it has nothing to offer. An empty folder that exists only
    // in the catalog has nothing to show at all.
    const emptyButReal =
        node.onDisk && node.totalCount === 0 && scope !== 'updates' && (!term || folderMatches);
    if (!entries.length && !children.size && !emptyButReal) return null;

    // Counts are carried over untouched; only what is rendered changes.
    return { ...node, entries, children };
};

/** How many rows this folder would show, so a bar can say "3 matches" while collapsed. */
export const shownUnder = (node: LibraryFolder): number => {
    let n = node.entries.length;
    for (const child of node.children.values()) n += shownUnder(child);
    return n;
};

export const formatSize = (mb: number): string =>
    mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.max(1, Math.round(mb))} MB`;
