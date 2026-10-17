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
    /** May be missing upstream; a 404 lists it in `absent` instead of failing. */
    optional?: boolean;
}

export interface InstallReport {
    downloaded: string[];
    unchanged: string[];
    absent: string[];
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
    executeExternalApp: (path: string) => invoke<void>('execute_external_app', { path }),
    getGameVersion: (instanceDir: string) => invoke<string | null>('get_game_version', { instanceDir }),
    ensureConfigTjapath: (instanceDir: string, globalSongsPath: string, includeLocal: boolean) =>
        invoke<string>('ensure_config_tjapath', { instanceDir, globalSongsPath, includeLocal }),
    createInstanceDir: (folderName: string) => invoke<string>('create_instance_dir', { folderName }),
    allowBuildDir: (path: string) => invoke<void>('allow_build_dir', { path }),
    openFolder: (path: string) => invoke<void>('open_folder', { path }),
    logDirectory: () => invoke<string | null>('log_directory'),
    uiHeartbeat: () => invoke<void>('ui_heartbeat'),

    // Files and processes (main.rs, fsops.rs, procs.rs)
    unzip: (zipPath: string, onProgress: Channel<number>, job?: string) =>
        invoke<string>('unzip_and_get_first_folder', { zipPath, job, onProgress }),
    mergeMoveDir: (src: string, dest: string) => invoke<void>('merge_move_dir', { src, dest }),
    runDotnet: (args: string[], cwd: string | null, onOutput: Channel<string[]>, job?: string) =>
        invoke<number>('run_dotnet', { args, cwd, job, onOutput }),

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

    // Song library (scan.rs, library.rs)
    scanSongs: (baseDir: string, onEvent: Channel<ScanProgressEvent>) => invoke<ScanResult>('scan_songs', { baseDir, onEvent }),
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
