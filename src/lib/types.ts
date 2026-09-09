// Shared domain types of the Hub frontend. The scan / migration / asset shapes mirror
// the Rust structs in src-tauri/src (serde renames every field to camelCase).

import type { download } from '@tauri-apps/plugin-upload';

// ---------------------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------------------

export type HubOS = 'Win' | 'Linux' | 'Mac' | 'Unsupported';

/** Progress payload of the upload plugin's download(), which does not export the type. */
export type ProgressPayload = Parameters<NonNullable<Parameters<typeof download>[2]>>[0];

/** Provided by the root layout through `getContext('toast')`. */
export interface ToastContext {
    TriggerError(message: string): void;
    TriggerWarning(message: string): void;
    TriggerSuccess(message: string): void;
    /** Downloads with retries; resolves to false once every attempt failed. */
    backoffDownload(
        url: string,
        path: string,
        onProgress?: (progress: ProgressPayload) => void,
        attempts?: number,
        delayMs?: number
    ): Promise<boolean>;
    wait(ms: number): Promise<void>;
}

// ---------------------------------------------------------------------------------------
// Song catalog (OpenTaiko-Soundtrack/soundtrack_info.json) and the local library
// ---------------------------------------------------------------------------------------

export type CourseName = 'Easy' | 'Normal' | 'Hard' | 'Oni' | 'Edit' | 'Tower' | 'Dan';

export interface SoundtrackEntry {
    uniqueId: string;
    /** Folder of the chart inside the soundtrack repository, backslash separated. */
    tjaFolderPath: string;
    /** Every file of the chart, as repository paths. */
    tjaFilesPath: string[];
    tjaGenreFolder: string;
    tjaMD5: string;
    /** Size in megabytes. */
    chartSize: number;
    chartTitle: string;
    chartSubtitle?: string;
    chartDifficulties: Partial<Record<CourseName, number>>;
    chartMakers: Partial<Record<CourseName, string>>;
    chartAudioFilePath: string;
    chartJacketFilePath?: string;
    /** Global Hall of Fame rank per difficulty, patched in once hof.db3 is loaded. */
    chartHoFRanks?: Partial<Record<CourseName, number>>;
}

/** A catalog song found in the shared library (uniqueId → this). */
export interface LocalSong {
    chartMD5s: string[];
    chartRelativePath: string;
}

export interface ScannedDifficulty {
    /** A CourseName for the standard courses; custom .tja course names pass through as-is. */
    course: string;
    level: number;
}

/** Side of a Tower chart: "Ex" is Spicy, "Normal" is Sweet. */
export type TowerSide = 'Ex' | 'Normal';

export interface ScannedSong {
    relPath: string;
    uniqueId: string | null;
    title: string | null;
    tjaMd5s: string[];
    difficulties: ScannedDifficulty[];
    /** Set for Tower charts that declare a SIDE. */
    side: TowerSide | null;
}

export interface ScannedGenre {
    relPath: string;
    title: string | null;
    boxDefSha1: string | null;
    preimageSha1: string | null;
}

export interface ScanResult {
    baseExists: boolean;
    songs: ScannedSong[];
    genres: ScannedGenre[];
}

export interface ScanProgressEvent {
    type: 'progress';
    scannedDirs: number;
    songsFound: number;
    batch: ScannedSong[];
}

/** Tree view node: a genre folder holding songs and sub-folders. */
export interface SongTreeFolder {
    name: string;
    title: string | null;
    children: Map<string, SongTreeFolder>;
    songs: ScannedSong[];
    count: number;
}

// ---------------------------------------------------------------------------------------
// Library migration (moving an instance's own Songs folder into the shared library)
// ---------------------------------------------------------------------------------------

export interface MigrationSongEntry {
    relPath: string;
    uniqueId: string | null;
    title: string | null;
    md5s: string[];
    /** Unix timestamp of the newest chart file. */
    modified: number | null;
}

export type MigrationStatus = 'new' | 'identical' | 'conflict';

export interface MigrationItem {
    status: MigrationStatus;
    src: MigrationSongEntry;
    dest: MigrationSongEntry | null;
}

export interface MigrationPlan {
    items: MigrationItem[];
    newCount: number;
    identicalCount: number;
    conflictCount: number;
}

export type MigrationAction = 'move' | 'keep_global' | 'use_instance';

export interface MigrationDecision {
    srcRelPath: string;
    action: MigrationAction;
    destRelPath: string | null;
}

export interface MigrateSummary {
    moved: number;
    replaced: number;
    discarded: number;
}

export interface DuplicateFolder {
    relPath: string;
    fileCount: number;
}

/** An instance whose own Songs folder still holds charts. */
export interface MigrationCandidate {
    instance: Instance;
    count: number;
    srcPath: string;
}

export interface DuplicateCandidate {
    instance: Instance;
    srcPath: string;
    globalSongs: string;
    folders: DuplicateFolder[];
}

// ---------------------------------------------------------------------------------------
// Instances and builds
// ---------------------------------------------------------------------------------------

export type BuildChannel = 'stable' | 'experimental';

export interface IndevBuildInfo {
    kind: 'indev';
    branch: string;
    label: string;
    /** Commit the instance was built from. */
    sha?: string;
    builtAt?: string;
}

export interface PrereleaseBuildInfo {
    kind: 'prerelease';
    tag: string;
    label: string;
}

export type ExperimentalBuild = IndevBuildInfo | PrereleaseBuildInfo;

/** What the build picker offers. */
export type BuildOption =
    | { kind: 'stable' }
    | { kind: 'indev'; branch: string; label: string }
    | { kind: 'prerelease'; tag: string; label: string };

export interface Instance {
    id: string;
    name: string;
    path: string;
    /** null until a build has been installed into the folder. */
    channel: BuildChannel | null;
    experimental: ExperimentalBuild | null;
    createdAt: string;
    /** Build chosen when the instance was created; the Home tab installs it, then clears it. */
    pendingInstall?: BuildOption | null;
}

export interface InstanceRegistry {
    version: number;
    activeId: string | null;
    instances: Instance[];
}

export interface GitHubAsset {
    name: string;
    browser_download_url: string;
}

export interface GitHubRelease {
    tag_name: string;
    prerelease: boolean;
    draft: boolean;
    assets: GitHubAsset[];
}

// ---------------------------------------------------------------------------------------
// Skins, characters and puchicharas (OpenTaiko-Skins/assets_info.json)
// ---------------------------------------------------------------------------------------

export type AssetType = 'Skins' | 'Characters' | 'Puchicharas';
export type AssetPrefix = 'skin' | 'chara';

/** Catalog entry; keys are prefixed with "skin" or "chara" depending on the asset type. */
type AssetFields<P extends AssetPrefix> =
    { [K in `${P}Name` | `${P}Folder` | `${P}FolderPath` | `${P}Version` | `${P}Resolution` | `${P}Creator` | `${P}PreviewImage`]: string } &
    { [K in `${P}Size`]: number } &
    { [K in `${P}FilesPath`]: string[] };

export type SkinInfo = AssetFields<'skin'>;
export type CharaInfo = AssetFields<'chara'>;
export type AssetInfo = SkinInfo | CharaInfo;

export type AssetCatalog = Record<AssetType, AssetInfo[]>;

/** An asset found inside the instance (folder name → this). */
export interface LocalAsset {
    assetFolderName: string;
    assetVersion: string;
}

export interface AssetVersion {
    relPath: string;
    version: string;
}

// ---------------------------------------------------------------------------------------
// Hall of Fame and artist databases (SQLite files fetched from opentaiko.github.io)
// ---------------------------------------------------------------------------------------

export interface HoFScore {
    player: string;
    status: string;
    score: number;
    grade: string;
    goodCount: number;
    okCount: number;
    badCount: number;
    videoLink: string | null;
    imageLink: string | null;
    listPoints: number;
    rank: number;
}

export interface ArtistLinks {
    artist: string;
    youtube: string | null;
    soundcloud: string | null;
    spotify: string | null;
    bandcamp: string | null;
    bilibili: string | null;
    other: string | null;
}

export interface SongArtists {
    artists: ArtistLinks[];
    link: string | null;
}

// ---------------------------------------------------------------------------------------
// Saves (Saves.db3)
// ---------------------------------------------------------------------------------------

/** A row of any save table, as returned by sql.js. */
export type SaveRow = Record<string, number | string | null>;

export interface SaveSummary {
    saveId: number;
    name: string;
    /** 0-4 for the playable slots, null for reserve saves. */
    slot: number | null;
    saveUid: string;
}

/** The portable document written by "Export" and read by "Import". */
export interface PortableSave {
    format: 'optk-save';
    formatVersion: number;
    dbVersion: string;
    save: SaveRow;
    bestPlays: SaveRow[];
    activeTriggers: (string | number | null)[];
    globalCounters: SaveRow[];
    danTitles: SaveRow[];
    nameplateTitles: (string | number | null)[];
    unlockedCharacters: (string | number | null)[];
    unlockedPuchicharas: (string | number | null)[];
    unlockedSongs: (string | number | null)[];
}
