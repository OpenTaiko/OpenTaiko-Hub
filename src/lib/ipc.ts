// Every backend command the page calls, one typed function each. Argument and result
// shapes mirror the Rust signatures in src-tauri/src (serde renames fields to camelCase),
// so a renamed command or a changed field is a type error here instead of a failure at
// runtime. Call these rather than `invoke` with a command name.
import { invoke, type Channel } from '@tauri-apps/api/core';
import type {
    AssetVersion,
    DuplicateFolder,
    MigrateSummary,
    MigrationDecision,
    MigrationPlan,
    ProgressPayload,
    ScanProgressEvent,
    ScanResult
} from '$lib/types';

// ---------------------------------------------------------------------------------------
// Results that exist only here (library.rs)
// ---------------------------------------------------------------------------------------

export interface BoxDef {
    /** False when the folder itself is not on disk yet. */
    folderExists: boolean;
    fileExists: boolean;
    text: string;
    title: string | null;
}

export interface SongMedia {
    /** Relative to the chart folder, forward-slashed. */
    preimage: string | null;
    audio: string | null;
}

export interface ImportReport {
    charts: { relPath: string; title: string | null; generatedId: boolean }[];
    /** Archives that held no .tja at all. */
    rejected: string[];
}

export interface InstallEntry {
    url: string;
    /** Destination relative to the folder the files are installed into. */
    relPath: string;
    /** Upstream git blob SHA: an identical local file is kept instead of downloaded. */
    gitSha?: string | null;
    /** Upstream size: a local file of another size is not read to be compared. */
    size?: number | null;
    /** May be missing upstream; a 404 lists it in `absent` instead of failing. */
    optional?: boolean;
}

export interface InstallReport {
    downloaded: string[];
    unchanged: string[];
    absent: string[];
}

/** What moving the Hub's data to a folder of its own did at startup (relocate.rs). */
export interface RelocationReport {
    newRoot: string;
    /** Folders moved, as [from, to]. */
    moved: [string, string][];
    /** Steps that failed, in the log's words. */
    problems: string[];
}

export interface ClosePrompt {
    title: string;
    message: string;
    close: string;
    keep: string;
}

// ---------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------

export const ipc = {
    // Game instances (main.rs, binver.rs, configini.rs, guard.rs)
    launchGame: (instanceDir: string) => invoke<void>('launch_game', { instanceDir }),
    /** True while a game the Hub started from this instance runs. */
    gameRunning: (instanceDir: string) => invoke<boolean>('game_running', { instanceDir }),
    getGameVersion: (instanceDir: string) => invoke<string | null>('get_game_version', { instanceDir }),
    /** Adds the Hub's Songs folder to the instance's Config.ini. */
    ensureConfigTjapath: (instanceDir: string, includeLocal: boolean) =>
        invoke<string>('ensure_config_tjapath', { instanceDir, includeLocal }),
    createInstanceDir: (folderName: string) => invoke<string>('create_instance_dir', { folderName }),
    /** Shows the folder dialog itself; null when it was closed without a choice. */
    attachInstanceDir: (title: string) => invoke<string | null>('attach_instance_dir', { title }),
    /** Shows a file dialog for one kind of file; empty when it was closed without a choice. */
    pickFiles: (kind: 'image' | 'chart_archive' | 'save', label: string, multiple = false) =>
        invoke<string[]>('pick_files', { kind, label, multiple }),
    allowBuildDir: (path: string) => invoke<void>('allow_build_dir', { path }),
    openFolder: (path: string) => invoke<void>('open_folder', { path }),
    hubRoot: () => invoke<string>('hub_root'),
    logDirectory: () => invoke<string | null>('log_directory'),
    /** Once per start; null when there is nothing to tell. */
    takeRelocationReport: () => invoke<RelocationReport | null>('take_relocation_report'),
    uiHeartbeat: () => invoke<void>('ui_heartbeat'),

    // Files and processes (main.rs, fsops.rs, procs.rs)
    unzip: (zipPath: string, onProgress: Channel<number>, job?: string) =>
        invoke<string>('unzip_and_get_first_folder', { zipPath, job, onProgress }),
    mergeMoveDir: (src: string, dest: string) => invoke<void>('merge_move_dir', { src, dest }),
    listDotnetSdks: (onOutput: Channel<string[]>, job?: string) => invoke<number>('list_dotnet_sdks', { job, onOutput }),
    /** Builds the game in `srcDir` (the `s` folder of a build folder) into `p` beside it. */
    publishGame: (srcDir: string, rid: string, onOutput: Channel<string[]>, job?: string) =>
        invoke<number>('publish_game', { srcDir, rid, job, onOutput }),

    // Writing files (writes.rs): the page has no permission to write files itself
    writeSettingsFile: (name: 'instances.json' | 'settings.json', contents: string) =>
        invoke<void>('write_settings_file', { name, contents }),
    writeVersionJson: (instanceDir: string, version: string | null, build: string) =>
        invoke<void>('write_version_json', { instanceDir, version, build }),
    /** Replaces the instance's Saves.db3, after copying it to SavesBackup/<backupName>. */
    writeSaves: (instanceDir: string, backupName: string, bytes: Uint8Array) =>
        invoke<void>('write_saves', bytes, { headers: { instance: encodeURIComponent(instanceDir), backup: backupName } }),
    /** Shows the save dialog itself; the chosen path, or null when it was closed. */
    exportSave: (fileName: string, filterName: string, contents: string) =>
        invoke<string | null>('export_save', { fileName, filterName, contents }),

    // Network (download.rs)
    downloadFile: (url: string, path: string, onProgress: Channel<ProgressPayload>, attempts?: number, job?: string) =>
        invoke<void>('download_file', { url, path, attempts, job, onProgress }),
    installFiles: (destRoot: string, entries: InstallEntry[], onProgress: Channel<ProgressPayload>, job?: string) =>
        invoke<InstallReport>('install_files', { destRoot, entries, job, onProgress }),
    fetchCached: (url: string) => invoke<ArrayBuffer>('fetch_cached', { url }),

    // Long operations (jobs.rs)
    beginJob: (job: string, prompt: ClosePrompt) => invoke<void>('begin_job', { job, prompt }),
    cancelJob: (job: string) => invoke<void>('cancel_job', { job }),
    endJob: (job: string) => invoke<void>('end_job', { job }),
    /** Once when the page loads: cancels what an earlier page left running. */
    resetJobs: () => invoke<void>('reset_jobs'),

    // Song library (scan.rs, library.rs)
    /** Progress events carry the songs only with `streamSongs` (to show them as they are
     *  found); the result always holds them all. */
    scanSongs: (baseDir: string, onEvent: Channel<ScanProgressEvent>, streamSongs = true) =>
        invoke<ScanResult>('scan_songs', { baseDir, streamSongs, onEvent }),
    countSongs: (baseDir: string) => invoke<number>('count_songs', { baseDir }),
    readBoxDef: (base: string, relPath: string) => invoke<BoxDef>('read_box_def', { base, relPath }),
    writeBoxDef: (base: string, relPath: string, text: string) => invoke<void>('write_box_def', { base, relPath, text }),
    createGenreFolder: (base: string, parentRel: string, name: string) =>
        invoke<string>('create_genre_folder', { base, parentRel, name }),
    deleteLibraryEntry: (base: string, relPath: string) => invoke<void>('delete_library_entry', { base, relPath }),
    setFolderJacket: (base: string, relPath: string, source: string) =>
        invoke<string>('set_folder_jacket', { base, relPath, source }),
    readSongMedia: (base: string, relPath: string) => invoke<SongMedia>('read_song_media', { base, relPath }),
    setSongPreimage: (base: string, relPath: string, source: string) =>
        invoke<string>('set_song_preimage', { base, relPath, source }),
    importChartArchives: (base: string, relPath: string, archives: string[]) =>
        invoke<ImportReport>('import_chart_archives', { base, relPath, archives }),

    // Moving an instance's songs into the shared library (migrate.rs)
    pathsAreSameDir: (a: string, b: string) => invoke<boolean>('paths_are_same_dir', { a, b }),
    planMigration: (srcSongs: string, destSongs: string) => invoke<MigrationPlan>('plan_migration', { srcSongs, destSongs }),
    applyMigration: (srcSongs: string, destSongs: string, decisions: MigrationDecision[]) =>
        invoke<MigrateSummary>('apply_migration', { srcSongs, destSongs, decisions }),
    findDuplicateSongFolders: (instanceSongs: string, globalSongs: string) =>
        invoke<DuplicateFolder[]>('find_duplicate_song_folders', { instanceSongs, globalSongs }),
    removeDuplicateSongFolders: (instanceSongs: string, globalSongs: string, relPaths: string[]) =>
        invoke<number>('remove_duplicate_song_folders', { instanceSongs, globalSongs, relPaths }),
    stripSongContent: (songsDir: string) => invoke<number>('strip_song_content', { songsDir }),

    // Skins and characters (assetscan.rs)
    scanAssetVersions: (baseDir: string, targetFile: string) =>
        invoke<AssetVersion[]>('scan_asset_versions', { baseDir, targetFile })
};
