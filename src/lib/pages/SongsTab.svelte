<script lang="ts">
    import { ipc } from '$lib/ipc';
    import Modal from '$lib/components/Modal.svelte';
    import { ConfirmDiscardEdits } from '$lib/stores/editing';
    import { LoadHallOfFame, ChartScores, HOF_DIFFICULTY_INDEX, type HoFRanks } from '$lib/services/hof';
    import { LoadSongArtists } from '$lib/services/artists';
    import { FetchSoundtrackCatalog, FetchRemoteShaMap, SongInstallEntries, SongGenrePaths, SoundtrackFileUrl } from '$lib/services/soundtrack';
    // Dependencies
    import { onMount, onDestroy } from 'svelte';
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { mkdir, exists, readFile } from '@tauri-apps/plugin-fs';
    import { OpenFolder, IsPlainRelPath } from '$lib/utils/path';
    import { SafeUrl } from '$lib/utils/sanitize';
    import { open, confirm } from '@tauri-apps/plugin-dialog';
    import { getCurrentWebview } from '@tauri-apps/api/webview';
    import { installFiles } from '$lib/utils/download';
    import { startJob, isCancelled, type Job } from '$lib/stores/activity';
    import { DescribeError, ErrorText } from '$lib/utils/errors';
    import { path } from '@tauri-apps/api';
    import { Channel } from '@tauri-apps/api/core';
    import { getContext } from 'svelte';
    import type { Database } from 'sql.js';
    import type {
        CourseName,
        DuplicateCandidate,
        HoFScore,
        LocalSong,
        MigrationCandidate,
        ScanProgressEvent,
        ScannedGenre,
        ScannedSong,
        SongArtists,
        SoundtrackEntry,
        ToastContext
    } from '$lib/types';
    const { TriggerError, TriggerWarning, TriggerSuccess } = getContext<ToastContext>('toast');


    import { _, locale } from 'svelte-i18n';
    import { get } from 'svelte/store';
    import { localizedTitle, titleSearchTerms } from '$lib/utils/titles';

    import { GetGlobalSongsPath } from "../utils/path";
    import { instances } from "../stores/instances";

    // Song management
    import SongMigrationModal from '$lib/components/SongMigrationModal.svelte';
    import NamePromptModal from '$lib/components/songs/NamePromptModal.svelte';

    // The song library browser
    import LibraryHeader from '$lib/components/songs/LibraryHeader.svelte';
    import CollectionBar from '$lib/components/songs/CollectionBar.svelte';
    import SongRow from '$lib/components/songs/SongRow.svelte';
    import SongRowHeader from '$lib/components/songs/SongRowHeader.svelte';
    import SongDetail from '$lib/components/songs/SongDetail.svelte';
    import FolderDetail from '$lib/components/songs/FolderDetail.svelte';
    import { buildLibraryTree, viewOf, pendingUnder, formatSize } from '$lib/utils/library';
    import type { LibraryEntry, LibraryFolder, Scope } from '$lib/utils/library';

    // Soundtrack
    // The big collections are $state.raw: they are always replaced as a whole (never
    // mutated in place), so wrapping thousands of entries in deep proxies would only add
    // a signal per key and per row read.
    let soundtrackInfo = $state.raw<SoundtrackEntry[]>([]);
    let catalogFetchFailed = $state(false);
    let currentSongs = $state.raw<Record<string, LocalSong>>({});          // uniqueId → local chart
    let allScannedSongs = $state.raw<ScannedSong[]>([]);       // every scanned song, catalog or custom
    let scannedGenres = $state.raw<Record<string, ScannedGenre>>({});         // relPath → genre folder
    let scanStats = $state({ dirs: 0, found: 0 });
    let scanning = $state(false);
    // Library browser state
    let scope = $state<Scope>('all');
    let librarySearch = $state('');
    /** Only one top-level collection is open at a time, which is what stops the tab
     *  rendering hundreds of rows the moment it opens. */
    let openCollection = $state<string | null>(null);
    let selectedKey = $state<string | null>(null);
    /** A collection is selected instead of a song: the panel shows its box.def. */
    let selectedFolder = $state<string | null>(null);
    let boxText = $state<string | null>(null);
    let boxFileExists = $state(false);
    let savingBox = $state(false);
    let importing = $state(false);
    let dropActive = $state(false);
    /** Object URL for the selected folder's default.* image, revoked when it changes. */
    let folderJacket = $state<string | null>(null);
    const dropListeners: (() => void)[] = [];
    let globalSongsPath = $state<string | null>(null);
    let songDLProgress = $state<Record<string, number>>({});
    let bulkBusy = $state(false);          // a bulk download is running
    // What the library header shows while a bulk download runs, and how to stop it
    let bulkStatus = $state<{ done: number; total: number; mbLeft: number } | null>(null);
    let bulkJob: Job | null = null;
    // Read by the Cancel button, which says it is cancelling until the run stops
    let bulkCancelRequested = $state(false);
    let activeSingleDownloads = $state(0);  // single-song downloads currently in flight (they run concurrently)
    let songBusy = $derived(activeSingleDownloads > 0);

    // Git blob SHAs of the soundtrack repository (path → sha), fetched once per session
    // and used to detect outdated box.def / default.png files
    let remoteShaMap: Map<string, string> | null = null;
    let remoteShaMapFetch: Promise<Map<string, string> | null> | null = null;   // in-flight fetch, shared by downloads that start together
    // After a failed tree fetch (usually GitHub's hourly API limit), downloads go ahead
    // without it until this time instead of asking again for every song
    let remoteShaMapRetryAt = 0;
    const SHA_MAP_COOLDOWN_MS = 10 * 60 * 1000;

    // Songs found inside attached instances that can be moved to the shared library
    let migrationCandidates = $state<MigrationCandidate[]>([]);
    let migrationCandidate = $state<MigrationCandidate | null>(null);   // the instance whose migration modal is open
    let migrationGlobalPath = $state<string | null>(null);

    // Chart-less folders in an instance that the shared library already provides: the
    // game reads both paths, so each one shows up as a duplicate, empty box
    let duplicateCandidates = $state<DuplicateCandidate[]>([]);
    let duplicateBusy = $state(false);

    let catalogById = $derived(new Map<string, SoundtrackEntry>(soundtrackInfo.map((s) => [s.uniqueId, s])));
    // Lowercased title spellings per song, built once per catalog change rather than per
    // row on every render (the list re-renders on each scan batch)
    let searchTerms = $derived(new Map<string, string[]>(soundtrackInfo.map((s) => [s.uniqueId, titleSearchTerms(s)])));

    // One tree of installed + downloadable charts. Counts come from here and never move
    // with the scope or the search, so "12 of 47" always means the same thing.
    let library = $derived(buildLibraryTree({
        scanned: allScannedSongs,
        genres: scannedGenres,
        catalog: soundtrackInfo,
        catalogById,
        installed: currentSongs,
        labelFor: (entry, fallback) => (entry ? localizedTitle(entry, $locale) : null) ?? fallback,
        subtitleFor: (entry) => entry.chartSubtitle ?? null
    }));

    let view = $derived(viewOf(library, scope, librarySearch,
        (entry: LibraryEntry) => (entry.catalog ? searchTerms.get(entry.catalog.uniqueId) ?? [] : [])));

    let topCollections = $derived(view ? [...view.children.values()].sort((a, b) => a.name.localeCompare(b.name)) : []);
    let libraryEmpty = $derived(!scanning && library.installedCount === 0 && library.totalCount > 0);

    let selectedEntry = $derived.by((): LibraryEntry | null => {
        if (!selectedKey) return null;
        const find = (node: LibraryFolder): LibraryEntry | null => {
            for (const entry of node.entries) if (entry.key === selectedKey) return entry;
            for (const child of node.children.values()) {
                const hit = find(child);
                if (hit) return hit;
            }
            return null;
        };
        return find(library);
    });

    const SOUNDTRACK_RAW = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main';

    // Genre art comes from the soundtrack repo, so a collection looks the same whether or
    // not it is installed. This is the same file EnsureGenreMetadata already downloads.
    const GenreArtUrl = (relPath: string): string | null =>
        relPath ? `${SOUNDTRACK_RAW}/${relPath.split('/').map(encodeURIComponent).join('/')}/default.png` : null;

    const JacketUrl = (entry: SoundtrackEntry): string | null =>
        entry.chartJacketFilePath ? `${SOUNDTRACK_RAW}/${entry.chartJacketFilePath}` : null;

    const OpenHoF = (entry: SoundtrackEntry, course: CourseName) => openHoFModal(entry, course);

    // An installed chart's jacket and preview come off the disk. Streaming them from the
    // soundtrack repository instead would put every browse of the library on the network
    // and on GitHub's bandwidth, and would leave a custom chart — which the repository
    // has never heard of — with no art and no audio at all.
    let songJacket = $state<string | null>(null);
    let songAudio = $state<string | null>(null);
    let mediaFor = $state<string | null>(null);

    const ReleaseSongMedia = () => {
        if (songJacket) URL.revokeObjectURL(songJacket);
        if (songAudio) URL.revokeObjectURL(songAudio);
        songJacket = null;
        songAudio = null;
    };

    const blobFor = async (base: string, relPath: string, name: string): Promise<string | null> => {
        const full = await path.join(base, relPath, ...name.split('/'));
        if (!(await exists(full))) return null;
        const bytes = await readFile(full);
        return URL.createObjectURL(new Blob([bytes as BlobPart]));
    };

    const LoadSongMedia = async (entry: LibraryEntry) => {
        const forPath = entry.relPath;
        ReleaseSongMedia();
        mediaFor = forPath;
        try {
            const base = globalSongsPath ?? await GetGlobalSongsPath();
            const media = await ipc.readSongMedia(base, forPath);
            const [jacket, audio] = await Promise.all([
                media.preimage ? blobFor(base, forPath, media.preimage) : null,
                media.audio ? blobFor(base, forPath, media.audio) : null
            ]);
            // The selection may have moved on while this was in flight.
            if (mediaFor !== forPath) {
                if (jacket) URL.revokeObjectURL(jacket);
                if (audio) URL.revokeObjectURL(audio);
                return;
            }
            songJacket = jacket;
            songAudio = audio;
        } catch (error) {
            console.error('Could not read the chart media:', error);
        }
    };

    $effect(() => {
        const entry = selectedEntry;
        if (!entry?.installed) {
            if (mediaFor !== null) {
                ReleaseSongMedia();
                mediaFor = null;
            }
            return;
        }
        if (entry.relPath === mediaFor) return;
        LoadSongMedia(entry);
    });

    const PickSongPreimage = async (entry: LibraryEntry) => {
        const picked = await open({
            multiple: false,
            filters: [{ name: get(_)('files.images'), extensions: ['png', 'jpg', 'jpeg', 'webp'] }]
        });
        if (!picked || Array.isArray(picked)) return;
        try {
            const base = globalSongsPath ?? await GetGlobalSongsPath();
            const name = await ipc.setSongPreimage(base, entry.relPath, picked);
            TriggerSuccess(get(_)('songs.box.jacket_saved', { values: { name } }));
            mediaFor = null;   // force the effect to reload it
            await LoadSongMedia(entry);
        } catch (error) {
            TriggerError(get(_)('songs.box.jacket_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    let selectedFolderNode = $derived.by((): LibraryFolder | null => {
        if (selectedFolder === null) return null;
        const find = (node: LibraryFolder): LibraryFolder | null => {
            if (node.relPath === selectedFolder) return node;
            for (const child of node.children.values()) {
                const hit = find(child);
                if (hit) return hit;
            }
            return null;
        };
        return find(library);
    });

    let selectedIsOfficial = $derived(selectedFolderNode?.official ?? false);

    onDestroy(() => {
        for (const stop of dropListeners) stop();
        if (folderJacket) URL.revokeObjectURL(folderJacket);
        ReleaseSongMedia();
    });

    // Selecting something else replaces the folder settings form, so unsaved edits there
    // are confirmed first instead of being dropped
    const SelectEntry = async (entry: LibraryEntry) => {
        if (selectedKey === entry.key) return;
        if (!(await ConfirmDiscardEdits())) return;
        selectedKey = entry.key;
        selectedFolder = null;
    };

    const SelectFolder = async (folder: LibraryFolder) => {
        if (selectedFolder === folder.relPath) return;
        if (!(await ConfirmDiscardEdits())) return;
        selectedFolder = folder.relPath;
        selectedKey = null;
        boxText = null;
        boxFileExists = false;
        LoadFolderJacket(folder.relPath);
        if (!folder.onDisk) return;
        try {
            const base = globalSongsPath ?? await GetGlobalSongsPath();
            const info = await ipc.readBoxDef(base, folder.relPath);
            // The selection may have moved on while this was in flight.
            if (selectedFolder !== folder.relPath) return;
            boxFileExists = info.fileExists;
            boxText = info.text;
        } catch (error) {
            console.error('Could not read box.def:', error);
            boxText = '';
        }
    };

    const JACKET_NAMES = ['default.png', 'default.jpg', 'default.webp'];

    // The image is read through the fs plugin and shown as a blob: the asset protocol is
    // not enabled for this app, so a local file cannot be src'd directly.
    const LoadFolderJacket = async (relPath: string) => {
        if (folderJacket) {
            URL.revokeObjectURL(folderJacket);
            folderJacket = null;
        }
        try {
            const base = globalSongsPath ?? await GetGlobalSongsPath();
            for (const name of JACKET_NAMES) {
                const full = await path.join(base, relPath, name);
                if (!(await exists(full))) continue;
                const bytes = await readFile(full);
                // The selection may have moved on while this was in flight.
                if (selectedFolder !== relPath) return;
                folderJacket = URL.createObjectURL(new Blob([bytes as BlobPart]));
                return;
            }
        } catch (error) {
            console.error('Could not read the folder image:', error);
        }
    };

    const PickJacket = async (relPath: string) => {
        const picked = await open({
            multiple: false,
            filters: [{ name: get(_)('files.images'), extensions: ['png', 'jpg', 'jpeg', 'webp'] }]
        });
        if (!picked || Array.isArray(picked)) return;
        try {
            const base = await GetGlobalSongsPath();
            const name = await ipc.setFolderJacket(base, relPath, picked);
            TriggerSuccess(get(_)('songs.box.jacket_saved', { values: { name } }));
            // The panel reloads the image itself; no chart changed, so no rescan.
            await LoadFolderJacket(relPath);
        } catch (error) {
            TriggerError(get(_)('songs.box.jacket_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    const SaveBoxDef = async (text: string) => {
        if (!selectedFolderNode) return;
        savingBox = true;
        try {
            const base = await GetGlobalSongsPath();
            await ipc.writeBoxDef(base, selectedFolderNode.relPath, text);
            boxText = text;
            boxFileExists = true;
            TriggerSuccess(get(_)('songs.folder.box_saved'));
            // Only the folder's own title can have moved, so it is updated in place
            // rather than re-scanning every chart in the library.
            const title = /^#TITLE:(.*)$/m.exec(text)?.[1]?.trim() || null;
            const known = scannedGenres[selectedFolderNode.relPath];
            if (known) {
                scannedGenres = {
                    ...scannedGenres,
                    [selectedFolderNode.relPath]: { ...known, title }
                };
            }
        } catch (error) {
            TriggerError(get(_)('songs.folder.box_save_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        } finally {
            savingBox = false;
        }
    };

    /** Names already used at this level, plus every name the soundtrack owns. */
    const takenUnder = (parentRel: string): Set<string> => {
        const find = (node: LibraryFolder): LibraryFolder | null => {
            if (node.relPath === parentRel) return node;
            for (const child of node.children.values()) {
                const hit = find(child);
                if (hit) return hit;
            }
            return null;
        };
        const parent = parentRel ? find(library) : library;
        const names = new Set<string>();
        for (const child of parent?.children.values() ?? []) {
            names.add(child.rawName.toLowerCase());
            names.add(child.label.toLowerCase());
        }
        return names;
    };

    /** Which folder the new-folder prompt would create into; null while it is closed. */
    let newFolderParent = $state<string | null>(null);

    const NewFolder = (parentRel: string) => { newFolderParent = parentRel; };

    const CreateFolder = async (parentRel: string, wanted: string) => {
        newFolderParent = null;
        try {
            const base = globalSongsPath ?? await GetGlobalSongsPath();
            const created = await ipc.createGenreFolder(base, parentRel, wanted);
            TriggerSuccess(get(_)('songs.folder.created', { values: { name: wanted } }));
            // A new empty folder adds no charts, so the scan has nothing to re-find:
            // registering it directly avoids emptying and rebuilding the whole library.
            scannedGenres = {
                ...scannedGenres,
                [created]: { relPath: created, title: wanted, boxDefSha1: null, preimageSha1: null }
            };
            // The tree has not rebuilt yet, so the node is named rather than looked up.
            SelectFolder({ relPath: created, onDisk: true } as LibraryFolder);
        } catch (error) {
            TriggerError(get(_)('songs.folder.create_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    const DeleteFolder = async (folder: LibraryFolder) => {
        const ok = await confirm(
            get(_)('songs.folder.delete_confirm', { values: { name: folder.label, count: folder.installedCount } })
        );
        if (!ok) return;
        try {
            const base = await GetGlobalSongsPath();
            await ipc.deleteLibraryEntry(base, folder.relPath);
            TriggerSuccess(get(_)('songs.folder.deleted', { values: { name: folder.label } }));
            selectedFolder = null;
            DropUnder(folder.relPath);
        } catch (error) {
            TriggerError(get(_)('songs.folder.delete_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    const DeleteSong = async (entry: LibraryEntry) => {
        const ok = await confirm(get(_)('songs.folder.delete_song_confirm', { values: { name: entry.label } }));
        if (!ok) return;
        try {
            const base = await GetGlobalSongsPath();
            await ipc.deleteLibraryEntry(base, entry.relPath);
            TriggerSuccess(get(_)('songs.folder.deleted', { values: { name: entry.label } }));
            selectedKey = null;
            DropUnder(entry.relPath);
        } catch (error) {
            TriggerError(get(_)('songs.folder.delete_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    /**
     * Forgets everything that lived under a path that was just deleted. Exact, and
     * instant, where a rescan would re-walk the whole library to learn the same thing.
     */
    const DropUnder = (relPath: string) => {
        const under = (p: string) => p === relPath || p.startsWith(relPath + '/');
        allScannedSongs = allScannedSongs.filter((song) => !under(song.relPath));
        currentSongs = Object.fromEntries(
            Object.entries(currentSongs).filter(([, song]) => !under(song.chartRelativePath))
        );
        scannedGenres = Object.fromEntries(
            Object.entries(scannedGenres).filter(([path]) => !under(path))
        );
    };

    /** Extracts dropped or picked archives into `relPath`. */
    const ImportArchives = async (relPath: string, archives: string[]) => {
        if (archives.length === 0 || importing) return;
        importing = true;
        const job = startJob('songs');
        try {
            const base = await GetGlobalSongsPath();
            const report = await ipc.importChartArchives(base, relPath, archives);
            if (report.charts.length > 0) {
                TriggerSuccess(get(_)('songs.folder.imported', { values: { count: report.charts.length } }));
                crawlSongs();
            }
            if (report.rejected.length > 0) {
                TriggerError(get(_)('songs.folder.import_rejected', { values: { names: report.rejected.join(', ') } }));
            }
        } catch (error) {
            TriggerError(get(_)('songs.folder.import_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        } finally {
            importing = false;
            job.end();
        }
    };

    const PickArchives = async (relPath: string) => {
        const picked = await open({
            multiple: true,
            filters: [{ name: get(_)('files.chart_archives'), extensions: ['zip'] }]
        });
        if (!picked) return;
        await ImportArchives(relPath, Array.isArray(picked) ? picked : [picked]);
    };

    // Hall of Fame
    const hofDiffShortMap: Partial<Record<CourseName, string>>     = { "Easy": "EZ", "Normal": "NM", "Hard": "HD", "Oni": "EX", "Edit": "EXEX" };
    let hofDb: Database | null = null;
    let hofMap = $state.raw<HoFRanks>({});

    // Modal state
    let hofModalOpen = $state(false);
    let hofModalSongInfo = $state<SoundtrackEntry | null>(null);
    let hofModalDifficulty = $state<CourseName | null>(null);
    let hofModalScores = $state<HoFScore[]>([]);
    let hofModalMaxListPoints = $state(0);

    const updateHoFInfo = async () => {
        try {
            const { db, ranks } = await LoadHallOfFame();
            hofDb?.close();
            hofDb = db;
            hofMap = ranks;
        } catch (e) {
            console.error('Failed to load HoF data:', e);
        }
    };

    const openHoFModal = (songInfo: SoundtrackEntry, difficulty: CourseName) => {
        if (!hofDb) return;
        const chart = ChartScores(hofDb, hofMap, songInfo.uniqueId, difficulty);
        if (!chart) return;
        hofModalMaxListPoints = chart.maxListPoints;
        hofModalScores = chart.scores;
        hofModalSongInfo = songInfo;
        hofModalDifficulty = difficulty;
        hofModalOpen = true;
    };

    const updateSoundtrackInfo = async () => {
        catalogFetchFailed = false;
        try {
            soundtrackInfo = await FetchSoundtrackCatalog();
        } catch (error) {
            console.error('Failed to fetch the soundtrack catalog:', error);
            // Keep an ARRAY: template #each and .filter/.sort calls expect one
            soundtrackInfo = [];
            catalogFetchFailed = true;
        }
    }
    
    // Scans the shared Songs library natively (one IPC call, live progress batches)
    const crawlSongs = async () => {
        scanning = true;
        scanStats = { dirs: 0, found: 0 };
        let liveSongs: Record<string, LocalSong> = {};
        let liveAll: ScannedSong[] = [];
        // A rescan of a library that is already on screen keeps showing it until the
        // final result replaces it: clearing first empties every bar and refills it row
        // by row, which reads as the whole library reloading after any small change.
        const showLive = allScannedSongs.length === 0;

        const registerSong = (map: Record<string, LocalSong>, list: ScannedSong[], song: ScannedSong) => {
            list.push(song);
            if (song.uniqueId) {
                map[song.uniqueId] = {
                    chartMD5s: song.tjaMd5s,
                    chartRelativePath: song.relPath
                };
            }
        };

        try {
            const baseDirPath = await GetGlobalSongsPath();

            // Batches are merged and pushed to the UI at most once per animation frame:
            // every update re-evaluates the status of each listed song, so applying them
            // as they arrive would hog the UI thread for the duration of the scan.
            let latestStats = { dirs: 0, found: 0 };
            let dirty = false;
            let flushFrame: number | null = null;
            const flush = () => {
                flushFrame = null;
                scanStats = latestStats;
                if (dirty && showLive) {
                    dirty = false;
                    // Raw state compares by reference: hand out fresh containers
                    currentSongs = { ...liveSongs };
                    allScannedSongs = [...liveAll];
                }
            };
            const channel = new Channel<ScanProgressEvent>();
            channel.onmessage = (message) => {
                if (message.type !== 'progress') return;
                latestStats = { dirs: message.scannedDirs, found: message.songsFound };
                if (message.batch?.length) {
                    for (const song of message.batch) registerSong(liveSongs, liveAll, song);
                    dirty = true;
                }
                flushFrame ??= requestAnimationFrame(flush);
            };

            const result = await ipc.scanSongs(baseDirPath, channel);
            if (flushFrame !== null) cancelAnimationFrame(flushFrame);

            // The command result is authoritative; events were only for live display
            const finalSongs: Record<string, LocalSong> = {};
            const finalAll: ScannedSong[] = [];
            for (const song of result.songs) registerSong(finalSongs, finalAll, song);
            currentSongs = finalSongs;
            allScannedSongs = finalAll;
            scannedGenres = Object.fromEntries(result.genres.map((genre) => [genre.relPath, genre]));
            scanStats = { dirs: latestStats.dirs, found: finalAll.length };
        } catch (error) {
            console.error('Song scan failed:', error);
            TriggerError(get(_)('songs.error.scan_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
        scanning = false;
        CheckMigrations();
    }

    // Looks inside every attached (non-experimental) instance for songs that could be
    // moved to the shared library
    const CheckMigrations = async () => {
        const candidates: MigrationCandidate[] = [];
        try {
            const globalSongs = await GetGlobalSongsPath();
            for (const inst of get(instances)) {
                const instSongs = await path.join(inst.path, 'Songs');
                // Asked of the backend, which resolves symlinks. A string compare
                // cannot see that an instance's Songs folder is a link to the shared
                // library, and would keep offering a migration the backend refuses.
                if (await ipc.pathsAreSameDir(instSongs, globalSongs)) continue;
                try {
                    const probe = new Channel<ScanProgressEvent>();
                    const result = await ipc.scanSongs(instSongs, probe);
                    if (result.baseExists && result.songs.length > 0) {
                        candidates.push({ instance: inst, count: result.songs.length, srcPath: instSongs });
                    }
                } catch (error) {
                    console.error(`Migration check failed for ${inst.name}:`, error);
                }
            }
        } catch (error) {
            console.error('Migration check failed:', error);
        }
        migrationCandidates = candidates;
        CheckDuplicates();
    }

    // Looks for chart-less folders an instance duplicates from the shared library
    const CheckDuplicates = async () => {
        const found: DuplicateCandidate[] = [];
        try {
            const globalSongs = await GetGlobalSongsPath();
            for (const inst of get(instances)) {
                const instSongs = await path.join(inst.path, 'Songs');
                try {
                    const folders = await ipc.findDuplicateSongFolders(instSongs, globalSongs);
                    if (folders.length > 0) {
                        found.push({ instance: inst, srcPath: instSongs, globalSongs, folders });
                    }
                } catch (error) {
                    console.error(`Duplicate check failed for ${inst.name}:`, error);
                }
            }
        } catch (error) {
            console.error('Duplicate check failed:', error);
        }
        duplicateCandidates = found;
    }

    const CleanDuplicates = async (candidate: DuplicateCandidate) => {
        if (duplicateBusy) return;
        duplicateBusy = true;
        try {
            const removed = await ipc.removeDuplicateSongFolders(candidate.srcPath, candidate.globalSongs, candidate.folders.map((f) => f.relPath));
            TriggerSuccess(get(_)('songs.duplicates.success', { values: { count: removed } }));
            duplicateCandidates = duplicateCandidates.filter((c) => c !== candidate);
        } catch (error) {
            TriggerError(get(_)('songs.duplicates.error', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
        duplicateBusy = false;
    }

    // Opens the resolve/transfer modal for one instance (conflicts are decided there)
    const OpenMigration = async (candidate: MigrationCandidate) => {
        migrationGlobalPath = await GetGlobalSongsPath();
        migrationCandidate = candidate;
    }

    // Called by the modal after a plan was applied; the resolved instance is emptied,
    // so it drops out of the candidate list and won't prompt again.
    const OnMigrationApplied = (candidate: MigrationCandidate) => {
        migrationCandidates = migrationCandidates.filter((c) => c !== candidate);
        migrationCandidate = null;
        crawlSongs();
    }

    /** Opens one collection, or one song's folder, inside the shared library. */
    const OpenGenreFolder = async (relPath: string) => {
        try {
            const songsDir = await GetGlobalSongsPath();
            await OpenFolder(relPath ? await path.join(songsDir, relPath) : songsDir);
        } catch (error) {
            TriggerError(get(_)('songs.error.open_folder', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    const BULK_SONG_CONCURRENCY = 3;

    /** Downloads everything a folder is still missing, updates included. */
    const DownloadFolder = async (folder: LibraryFolder) => {
        if (scanning) {
            TriggerWarning(get(_)('songs.error.scanning'));
            return;
        }
        if (bulkBusy || songBusy) return;

        const pending = pendingUnder(folder);
        if (pending.length === 0) return;

        bulkBusy = true;
        bulkCancelRequested = false;
        const job = startJob('songs');
        bulkJob = job;
        // Progress lives in the header instead of a toast per song, and the run ends with
        // a single summary: hundreds of stacked toasts buried the one that mattered
        let done = 0;
        let failed = 0;
        let mbLeft = pending.reduce((sum, song) => sum + (song.chartSize || 0), 0);
        bulkStatus = { done, total: pending.length, mbLeft };
        // Genre folders of the downloaded songs, whose box.def and art are refreshed once at
        // the end rather than after each song
        const genres = new Set<string>();
        try {
            await EnsureRemoteShaMap();
            // A few songs at a time (each downloads several files at once in the
            // backend): one after the other, a whole library took a request round trip
            // per file in sequence. Kept small so GitHub does not start refusing requests.
            let next = 0;
            const worker = async () => {
                while (next < pending.length && !bulkCancelRequested) {
                    const index = next++;
                    const SInfo = pending[index];
                    const outcome = await DownloadSong(SInfo, currentSongs[SInfo.uniqueId] ?? null, index + 1, pending.length, job.id);
                    if (outcome === 'done') {
                        done++;
                        for (const genrePath of SongGenrePaths(SInfo)) genres.add(genrePath);
                    }
                    else if (outcome === 'failed') failed++;
                    mbLeft = Math.max(0, mbLeft - (SInfo.chartSize || 0));
                    bulkStatus = { done, total: pending.length, mbLeft };
                }
            };
            await Promise.all(Array.from({ length: Math.min(BULK_SONG_CONCURRENCY, pending.length) }, worker));

            const values = { count: done, total: pending.length, failed };
            if (bulkCancelRequested) {
                TriggerWarning(get(_)('bulk.cancelled', { values }));
                return;
            }
            if (failed > 0) TriggerError(get(_)('bulk.failed', { values }));
            else TriggerSuccess(get(_)('bulk.done', { values }));

            const updated = await RefreshAllGenreMetadata(genres, job.id);
            if (updated > 0) TriggerSuccess(get(_)('songs.metadata.updated', { values: { count: updated } }));
        } finally {
            job.end();
            bulkJob = null;
            bulkStatus = null;
            bulkBusy = false;
        }
    };

    const CancelBulk = () => {
        bulkCancelRequested = true;
        bulkJob?.cancel();
    };

    const OpenSongsFolder = async () => {
        try {
            const songsDir = await GetGlobalSongsPath();
            await mkdir(songsDir, { recursive: true });
            await OpenFolder(songsDir);
        } catch (error) {
            // OpenFolder reports its own failures; this is the folder creation failing
            TriggerError(get(_)('songs.error.open_folder', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    }

    // Fetches the soundtrack repository's git tree once so local box.def / default.png
    // files can be compared against their upstream version by git blob SHA
    const EnsureRemoteShaMap = async (): Promise<Map<string, string> | null> => {
        if (remoteShaMap) return remoteShaMap;
        if (Date.now() < remoteShaMapRetryAt) return null;
        // Downloads started back to back share one request instead of each hitting the
        // GitHub API (rate limited) for the same tree
        if (!remoteShaMapFetch) {
            remoteShaMapFetch = (async () => {
                try {
                    remoteShaMap = await FetchRemoteShaMap();
                } catch (error) {
                    console.error('Failed to fetch the soundtrack repository tree:', error);
                    remoteShaMapRetryAt = Date.now() + SHA_MAP_COOLDOWN_MS;
                } finally {
                    remoteShaMapFetch = null;
                }
                return remoteShaMap;
            })();
        }
        return remoteShaMapFetch;
    }

    // Downloads box.def / default.png for a genre folder when it is missing or its git
    // blob SHA differs from the soundtrack repository. Without the remote index, box.def
    // is refreshed once per session and the preimage only when missing; a file the
    // repository does not have is simply skipped (the backend reports a 404 as absent
    // instead of the page probing for it first).
    //
    // Calls are serialized: songs download concurrently, and two songs of the same
    // genre would otherwise write the same box.def on top of each other. Once the first
    // one has written it, the next sees the updated local SHA and skips the file.
    let genreMetadataLock: Promise<unknown> = Promise.resolve();
    const EnsureGenreMetadata = (genrePath: string, job?: string): Promise<boolean> => {
        const run = genreMetadataLock.then(() => EnsureGenreMetadataUnlocked(genrePath, job));
        genreMetadataLock = run.catch(() => {});
        return run;
    }

    // Genres whose box.def was already refreshed without the remote index this session
    const boxDefRefreshedBlind = new Set<string>();

    const EnsureGenreMetadataUnlocked = async (genrePath: string, job?: string): Promise<boolean> => {
        const baseDirPath = await GetGlobalSongsPath();
        const genreFullPath = await path.join(baseDirPath, genrePath);
        let changed = false;

        for (const fileName of ['box.def', 'default.png']) {
            const localGenre = scannedGenres[genrePath];
            const localSha = fileName === 'box.def' ? localGenre?.boxDefSha1 : localGenre?.preimageSha1;
            const remoteSha = remoteShaMap?.get(`${genrePath}/${fileName}`);

            let needsDownload: boolean;
            if (remoteShaMap) {
                needsDownload = !!remoteSha && localSha !== remoteSha;
            } else if (fileName === 'box.def') {
                needsDownload = !boxDefRefreshedBlind.has(genrePath);
                boxDefRefreshedBlind.add(genrePath);
            } else {
                needsDownload = !(await exists(await path.join(genreFullPath, fileName)));
            }
            if (!needsDownload) continue;

            const _url = SoundtrackFileUrl(`${genrePath}/${fileName}`);
            try {
                const report = await installFiles(genreFullPath, [{ url: _url, relPath: fileName, optional: !remoteSha }], undefined, job);
                if (report.downloaded.length === 0) continue;
                changed = true;
                if (remoteSha) {
                    scannedGenres = {
                        ...scannedGenres,
                        [genrePath]: {
                            ...(scannedGenres[genrePath] ?? { relPath: genrePath, title: null, boxDefSha1: null, preimageSha1: null }),
                            [fileName === 'box.def' ? 'boxDefSha1' : 'preimageSha1']: remoteSha
                        }
                    };
                }
            } catch (error) {
                console.error(`Failed to update ${genrePath}/${fileName}:`, error);
            }
        }
        return changed;
    }

    // The genres of the songs a bulk run downloaded, which the last scan may not know yet,
    // and with the repository index every genre the scan found. Stops once cancelled.
    const RefreshAllGenreMetadata = async (downloaded: Set<string>, job: string) => {
        const genres = new Set([...(remoteShaMap ? Object.keys(scannedGenres) : []), ...downloaded]);
        let updated = 0;
        for (const genrePath of genres) {
            if (bulkCancelRequested) break;
            if (await EnsureGenreMetadata(genrePath, job)) updated++;
        }
        return updated;
    }

    type DownloadOutcome = 'done' | 'failed' | 'cancelled' | 'skipped';

    // songNb is set when a bulk run drives this: the run reports one summary at the end,
    // so the song itself stays quiet and only returns how it went.
    const DownloadSong = async (songObj: SoundtrackEntry, currentObj: LocalSong | null, songNb?: number, songTotal?: number, job?: string): Promise<DownloadOutcome> => {
        // Never write into the library while it is still being scanned, the scan's
        // final result would otherwise clobber this download's bookkeeping.
        if (scanning) {
            TriggerWarning(get(_)('songs.error.scanning'));
            return 'skipped';
        }
        // songNb is set when the bulk loop drives this, which owns the busy flag itself
        const standalone = songNb === undefined;
        if (standalone) {
            // Single songs run in parallel. Only refuse while a bulk run owns every
            // displayed row, or when this exact song is already in flight (it would
            // write into the same folder twice).
            if (bulkBusy || songDLProgress[songObj.uniqueId] !== undefined) return 'skipped';
            activeSingleDownloads++;
        }
        // A song of its own is a job too: the Songs tile shows it is running, and
        // closing the window asks first instead of cutting it off
        const ownJob = standalone ? startJob('songs') : null;
        // Claim the row right away (nothing awaited above) so a second click on the
        // same song cannot slip through before the download shows its progress bar
        songDLProgress[songObj.uniqueId] = 0;
        try {
            return await RunDownloadSong(songObj, currentObj, songNb !== undefined, ownJob?.id ?? job);
        } finally {
            // Also releases the row when the download throws, otherwise the button
            // would stay replaced by a stuck progress bar
            delete songDLProgress[songObj.uniqueId];
            if (standalone) activeSingleDownloads--;
            ownJob?.end();
        }
    }

    const RunDownloadSong = async (songObj: SoundtrackEntry, currentObj: LocalSong | null, quiet: boolean, job?: string): Promise<DownloadOutcome> => {
        await EnsureRemoteShaMap();

        // Catalog paths decide where files are written, so a malformed one is refused
        // before anything is downloaded
        if (![songObj.tjaFolderPath, ...songObj.tjaFilesPath].every(IsPlainRelPath)) {
            console.error(`Refusing catalog paths of ${songObj.uniqueId}:`, songObj.tjaFolderPath, songObj.tjaFilesPath);
            if (!quiet) TriggerError(get(_)('dl.error.failed_abort'));
            return 'failed';
        }

        const baseDirPath = await GetGlobalSongsPath();
        const localRelPath = ((currentObj !== null) ? currentObj.chartRelativePath : songObj.tjaFolderPath).replace(/\\/g, '/');
        const tjaFullPath = await path.join(baseDirPath, localRelPath);

        // Files go straight into the song folder: the backend downloads them side by
        // side and only replaces anything once every file has arrived, so there is no
        // temporary copy to write and then copy again. A file whose git blob SHA matches
        // the repository is kept, so updating a chart does not download its audio again.
        const entries = SongInstallEntries(songObj, remoteShaMap);

        let totbyts = 0;
        try {
            await installFiles(tjaFullPath, entries, (pr) => {
                totbyts += pr.progress;
                songDLProgress[songObj.uniqueId] = 100 * (totbyts / (songObj.chartSize * 1024 * 1024));
            }, job);
        } catch (error) {
            // The caller's finally releases the row
            if (isCancelled(error)) return 'cancelled';
            console.error(`Download of ${songObj.uniqueId} failed:`, error);
            if (!quiet) TriggerError(`${get(_)('dl.error.failed_abort')}<br>${DescribeError(error)}`, { details: ErrorText(error) });
            return 'failed';
        }

        // Download / refresh box.def and default.png of every ancestor genre folder. A bulk
        // run does this once at the end: here, one slow box.def held up every song after it
        if (!quiet) {
            for (const genrePath of SongGenrePaths(songObj)) {
                await EnsureGenreMetadata(genrePath, job);
            }
        }

        if (!quiet) TriggerSuccess(get(_)('songs.success.download_complete'));

        //crawlSongs();
        currentSongs = {
            ...currentSongs,
            [songObj.uniqueId]: {
                chartMD5s: [songObj.tjaMD5],
                // Keep the actual install location when the song was relocated by the user
                chartRelativePath: (currentObj !== null) ? currentObj.chartRelativePath : songObj.tjaFolderPath
            }
        };
        return 'done';
    }

    // Artists info
    let songArtistsMap = $state.raw<Record<string, SongArtists>>({}); // songUid → artists and links

    const updateArtistInfo = async () => {
        try {
            songArtistsMap = await LoadSongArtists();
        } catch (e) {
            console.error('Failed to load artist info:', e);
        }
    };

    onMount(async () => {
        GetGlobalSongsPath().then((dir) => globalSongsPath = dir).catch(() => {});

        const stopDrop = await getCurrentWebview().onDragDropEvent(async (event) => {
            // Archives land in the collection that is currently selected, which is the
            // only unambiguous target: the panel showing the drop zone is its panel.
            const target = selectedFolderNode;
            const usable = target !== null && target.onDisk && target.canAddCharts;
            if (event.payload.type === 'over') {
                dropActive = usable;
            } else if (event.payload.type === 'drop') {
                dropActive = false;
                if (!usable || !target) {
                    TriggerWarning(get(_)('songs.folder.drop_needs_folder'));
                    return;
                }
                await ImportArchives(target.relPath, event.payload.paths);
            } else {
                dropActive = false;
            }
        });
        dropListeners.push(stopDrop);
        crawlSongs();         // native scan: no longer depends on the catalog fetch
        await updateSoundtrackInfo();
        updateHoFInfo();      // fire-and-forget: patches soundtrackInfo when DB is ready
        updateArtistInfo();   // fire-and-forget
    });

</script>

{#if catalogFetchFailed}
<aside class="card p-3 mb-2 flex items-center gap-3 flex-wrap">
	<i class="fa-solid fa-triangle-exclamation text-red-500"></i>
	<span class="flex-1"><b>{$_('common.fetch_error')}</b></span>
	<button type="button" class="button-blue button-main" onclick={updateSoundtrackInfo}>
		<i class="fa-solid fa-rotate"></i> {$_('common.retry')}
	</button>
</aside>
{/if}

{#each duplicateCandidates as candidate (candidate.instance.id)}
<aside class="card p-3 mb-2 flex items-center gap-3 flex-wrap">
	<i class="fa-solid fa-clone text-yellow-500"></i>
	<span class="flex-1">
		{$_('songs.duplicates.banner', { values: { count: candidate.folders.length, instance: candidate.instance.name } })}
		<br />
		<span class="text-sm opacity-70">{candidate.folders.map((f) => f.relPath).join(', ')}</span>
	</span>
	<button type="button" class="button-green button-main" disabled={duplicateBusy} onclick={() => CleanDuplicates(candidate)}>
		<i class="fa-solid fa-broom"></i> {$_('songs.duplicates.button')}
	</button>
	<button type="button" class="button-gray button-main" onclick={() => duplicateCandidates = duplicateCandidates.filter((c) => c !== candidate)}>
		{$_('songs.migrate.later')}
	</button>
</aside>
{/each}

{#each migrationCandidates as candidate (candidate.instance.id)}
<aside class="card p-3 mb-2 flex items-center gap-3 flex-wrap">
	<i class="fa-solid fa-boxes-packing"></i>
	<span class="flex-1">{$_('songs.migrate.banner', { values: { count: candidate.count, instance: candidate.instance.name } })}</span>
	<button type="button" class="button-green button-main" disabled={scanning || bulkBusy || songBusy || migrationCandidate !== null} onclick={() => OpenMigration(candidate)}>
		<i class="fa-solid fa-right-left"></i> {$_('songs.migrate.button')}
	</button>
	<button type="button" class="button-gray button-main" onclick={() => migrationCandidates = migrationCandidates.filter((c) => c !== candidate)}>
		{$_('songs.migrate.later')}
	</button>
</aside>
{/each}

<LibraryHeader
	LibraryPath={globalSongsPath}
	Installed={library.installedCount}
	Total={library.totalCount}
	PendingMb={library.pendingMb}
	Outdated={library.outdatedCount}
	Scanning={scanning}
	Found={scanStats.found}
	Busy={bulkBusy || songBusy}
	Bulk={bulkStatus}
	Cancelling={bulkCancelRequested}
	OnCancelBulk={CancelBulk}
	CurrentScope={scope}
	Search={librarySearch}
	OnScope={(s) => scope = s}
	OnSearch={(v) => librarySearch = v}
	OnReload={crawlSongs}
	OnOpenLibrary={OpenSongsFolder}
	OnGetAll={() => DownloadFolder(library)}
/>

<div class="library-body">
	<div class="collections">
		{#if libraryEmpty}
			<!-- First run: one sentence and one action, not a wall of ghosts. -->
			<div class="card start">
				<h3>{$_('songs.start.title')}</h3>
				<p>{$_('songs.start.body')}</p>
				<div class="start-actions">
					<button type="button" class="button-green button-main" disabled={bulkBusy || songBusy || scanning} onclick={() => DownloadFolder(library)}>
						<i class="fa-solid fa-download"></i>
						{$_('songs.library.get_everything', { values: { count: library.totalCount } })}
						<span class="dim">{formatSize(library.pendingMb)}</span>
					</button>
					<button type="button" class="button-blue button-main" onclick={OpenSongsFolder}>
						<i class="fa-solid fa-folder-open"></i> {$_('songs.button.open_folder')}
					</button>
					<button type="button" class="button-blue button-main" onclick={() => NewFolder('')}>
						<i class="fa-solid fa-folder-plus"></i> {$_('songs.folder.new_folder')}
					</button>
				</div>
			</div>
		{:else if view === null}
			<!-- Says why the list is empty: a search that found nothing reads differently
			     from a library with nothing to update -->
			<p class="nomatch">
				{#if librarySearch.trim()}
					{$_('songs.tree.no_match')}
				{:else if scope === 'updates'}
					<i class="fa-solid fa-check"></i> {$_('songs.tree.all_updated')}
				{:else if scope === 'installed'}
					{$_('songs.tree.none_installed')}
				{:else}
					{$_('songs.tree.nothing_here')}
				{/if}
			</p>
		{:else}
			{#each topCollections as node (node.relPath)}
				<CollectionBar
					Node={node}
					Depth={0}
					Open={openCollection === node.relPath}
					SelectedKey={selectedKey}
					SelectedFolder={selectedFolder}
					Downloading={songDLProgress}
					Disabled={bulkBusy || scanning}
					BulkDisabled={bulkBusy || songBusy || scanning}
					ArtUrl={GenreArtUrl}
					OnToggle={(p) => openCollection = openCollection === p ? null : p}
					OnSelect={SelectEntry}
					OnSelectFolder={SelectFolder}
					OnGet={(entry) => DownloadSong(entry, currentSongs[entry.uniqueId] ?? null)}
					OnGetFolder={DownloadFolder}
					OnOpenFolder={OpenGenreFolder}
				/>
			{/each}

			<button type="button" class="addfolder" onclick={() => NewFolder('')}>
				<i class="fa-solid fa-plus"></i> {$_('songs.folder.new_folder')}
			</button>

			{#if view.entries.length > 0}<SongRowHeader />{/if}
			{#each view.entries as entry (entry.key)}
				<SongRow
					Entry={entry}
					Selected={selectedKey === entry.key}
					Progress={entry.catalog ? songDLProgress[entry.catalog.uniqueId] : undefined}
					Disabled={bulkBusy || scanning}
					OnSelect={SelectEntry}
					OnGet={(e) => DownloadSong(e, currentSongs[e.uniqueId] ?? null)}
				/>
			{/each}
		{/if}
	</div>

	{#if selectedFolderNode}
		<FolderDetail
			Node={selectedFolderNode}
			BoxText={boxText}
			BoxFileExists={boxFileExists}
			Saving={savingBox}
			Busy={bulkBusy || songBusy || scanning}
			DropActive={dropActive}
			Importing={importing}
			Official={selectedIsOfficial}
			Scanning={scanning}
			OnSaveBox={SaveBoxDef}
			OnOpenFolder={OpenGenreFolder}
			OnGetFolder={DownloadFolder}
			OnNewFolder={NewFolder}
			OnDelete={DeleteFolder}
			OnPickArchives={PickArchives}
			OnPickJacket={PickJacket}
			JacketUrl={folderJacket}
		/>
	{:else}
	<SongDetail
		Entry={selectedEntry}
		Artists={selectedEntry?.catalog ? songArtistsMap[selectedEntry.catalog.uniqueId] ?? null : null}
		HoFRanks={selectedEntry?.catalog ? hofMap[selectedEntry.catalog.uniqueId] ?? {} : {}}
		Progress={selectedEntry?.catalog ? songDLProgress[selectedEntry.catalog.uniqueId] : undefined}
		Disabled={bulkBusy || scanning}
		JacketUrl={JacketUrl}
		LocalJacket={songJacket}
		LocalAudio={songAudio}
		OnPickPreimage={PickSongPreimage}
		OnGet={(entry) => DownloadSong(entry, currentSongs[entry.uniqueId] ?? null)}
		OnOpenFolder={OpenGenreFolder}
		OnHoF={OpenHoF}
		OnDelete={DeleteSong}
	/>
	{/if}
</div>

{#if newFolderParent !== null}
    {@const parent = newFolderParent}
    <NamePromptModal
        Title={$_('songs.folder.new_folder')}
        Label={$_('songs.folder.new_prompt')}
        Placeholder={$_('songs.folder.new_placeholder')}
        Confirm={$_('songs.folder.create')}
        Taken={takenUnder(parent)}
        OnConfirm={(name) => CreateFolder(parent, name)}
        OnCancel={() => newFolderParent = null}
    />
{/if}

{#if hofModalOpen && hofModalSongInfo && hofModalDifficulty}
{@const hofTitle = $_('hof.modal_title', { values: { title: localizedTitle(hofModalSongInfo, $locale), course: hofDiffShortMap[hofModalDifficulty] ?? '', rank: hofMap[hofModalSongInfo.uniqueId]?.[hofModalDifficulty] ?? '' } })}
<Modal Label={hofTitle} OnClose={() => hofModalOpen = false}>
    <div class="p-6 space-y-4 hof-modal">
        <div class="flex justify-between items-center">
            <h2 class="h3">{hofTitle}</h2>
            <button type="button" class="btn-icon btn-icon-sm preset-tonal" onclick={() => hofModalOpen = false} aria-label={$_('hof.close')} title={$_('hof.close')}><i class="fa-solid fa-xmark"></i></button>
        </div>
        <div class="flex flex-col gap-1">
            <a href="https://opentaiko.github.io/songinfo/{hofModalSongInfo.uniqueId}?d={HOF_DIFFICULTY_INDEX[hofModalDifficulty]}" target="_blank" class="underline">
                {$_('hof.website_link')} <i class="fa-solid fa-arrow-up-right-from-square"></i>
            </a>
            <span class="text-sm opacity-70">{$_('hof.max_list_points')}: <b>{hofModalMaxListPoints}</b></span>
        </div>
        {#if hofModalScores.length === 0}
            <p>{$_('hof.no_scores')}</p>
        {:else}
        <div class="table-wrap">
            <table class="table table-hover">
                <thead>
                    <tr>
                        <th>{$_('hof.col.rank')}</th>
                        <th>{$_('hof.col.player')}</th>
                        <th>{$_('hof.col.score')}</th>
                        <th>{$_('hof.col.grade')}</th>
                        <th>{$_('hof.col.status')}</th>
                        <th>{$_('hof.col.good')}</th>
                        <th>{$_('hof.col.ok')}</th>
                        <th>{$_('hof.col.bad')}</th>
                        <th>{$_('hof.col.list_points')}</th>
                        <th>{$_('hof.col.video')}</th>
                    </tr>
                </thead>
                <tbody>
                    {#each hofModalScores as s}
                        {@const gradeDisplay = s.grade?.toUpperCase()}
                        {@const gradeClass = gradeDisplay === 'Ω' ? 'omega' : gradeDisplay}
                    <tr>
                        <td>{s.rank}</td>
                        <td>{s.player ?? '—'}</td>
                        <td>{s.score?.toLocaleString($locale ?? undefined)}</td>
                        <td class="grade grade-{gradeClass}">{gradeDisplay}</td>
                        <td class="status status-{s.status?.replace(' ', '-')}">{s.status === 'Perfect' ? $_('hof.status.perfect') : s.status === 'Full Combo' ? $_('hof.status.full_combo') : s.status === 'Clear' ? $_('hof.status.clear') : (s.status ?? '—')}</td>
                        <td>{s.goodCount}</td>
                        <td>{s.okCount}</td>
                        <td>{s.badCount}</td>
                        <td>{s.listPoints?.toLocaleString($locale ?? undefined)}</td>
                        <td>
                            {#if SafeUrl(s.videoLink)}
                                <a href={SafeUrl(s.videoLink)} target="_blank" rel="noreferrer" class="underline">{$_('hof.video_link')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a>
                            {:else}—{/if}
                        </td>
                    </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        {/if}
    </div>
</Modal>
{/if}

{#if migrationCandidate && migrationGlobalPath}
{@const candidate = migrationCandidate}
<SongMigrationModal
	Candidate={candidate}
	GlobalPath={migrationGlobalPath}
	CatalogById={catalogById}
	OnClose={() => migrationCandidate = null}
	OnApplied={() => OnMigrationApplied(candidate)}
/>
{/if}

<style>
    /* Collections scroll; the detail panel stays put beside them. */
    .library-body {
        display: grid;
        grid-template-columns: minmax(0, 1fr) 20rem;
        gap: 0.6rem;
        align-items: start;
        margin-top: 0.5rem;
    }
    .collections {
        display: flex;
        flex-direction: column;
        gap: 0.35rem;
        min-width: 0;
        padding: 0.5rem;
        border-radius: var(--radius-container);
        background-color: light-dark(var(--color-surface-100), var(--color-surface-800));
        box-shadow: inset 0 0 0 1px light-dark(rgb(23 23 23 / 0.05), rgb(250 250 250 / 0.05));
    }

    .start {padding: 1.25rem; display: flex; flex-direction: column; gap: 0.6rem; align-items: flex-start;}
    .start-actions {display: flex; gap: 0.5rem; flex-wrap: wrap;}
    .dim {opacity: 0.85; font-weight: 400;}
    .nomatch {opacity: 0.7; padding: 1rem 0.5rem;}

    .addfolder {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 0.45rem;
        width: 100%;
        font: inherit;
        font-size: 0.82rem;
        font-weight: 700;
        padding: 0.5rem;
        border-radius: 0.5rem;
        border: 2px dashed color-mix(in oklab, var(--color-surface-500) 40%, transparent);
        background: none;
        color: inherit;
        opacity: 0.8;
        cursor: pointer;
    }
    .addfolder:hover {
        opacity: 1;
        border-color: var(--color-primary-500);
        background-color: color-mix(in oklab, var(--color-primary-500) 10%, transparent);
    }

    /* Below ~1000px the panel would squeeze the list, so it moves under it. */
    @media (max-width: 1000px) {
        .library-body {grid-template-columns: minmax(0, 1fr);}
    }

    .hof-modal {
        width: min(800px, calc(100vw - 2rem));
    }

    /* Grade letter colors */
    .grade { font-weight: bold; }
    .grade-E { color: light-dark(#6b7280, #ffffff); }
    .grade-D { color: light-dark(#b91c1c, #ff4444); }
    .grade-C { color: light-dark(#c2410c, #ff9933); }
    .grade-B { color: light-dark(#a16207, #ffdd00); }
    .grade-A { color: light-dark(#15803d, #44cc44); }
    .grade-S {
        background: linear-gradient(90deg, #56ccf2, #2f80ed, #56ccf2);
        background-size: 200% auto;
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        animation: shimmer-blue 2s linear infinite;
    }
    .grade-omega {
        font-family: 'Segoe UI', Arial, sans-serif;
        background: linear-gradient(90deg, light-dark(#1a237e, #8c9eff), light-dark(#3949ab, #b3bcff), light-dark(#5c6bc0, #dfe3ff), light-dark(#3949ab, #b3bcff), light-dark(#1a237e, #8c9eff));
        background-size: 200% auto;
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        animation: shimmer-blue 2s linear infinite;
    }

    /* Status colors */
    .status-Perfect {
        background: linear-gradient(90deg, #ff0000, #ff8800, #ffff00, #00cc00, #0088ff, #8800ff, #ff0000);
        background-size: 300% auto;
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        font-weight: bold;
        animation: rainbow 3s linear infinite;
    }
    .status-Full-Combo {
        background: linear-gradient(90deg, #b8860b, #ffd700, #fffacd, #ffd700, #b8860b);
        background-size: 200% auto;
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        font-weight: bold;
        animation: shimmer-gold 2s linear infinite;
    }

    @keyframes rainbow {
        0%   { background-position: 0% center; }
        100% { background-position: 300% center; }
    }
    @keyframes shimmer-gold {
        0%   { background-position: 0% center; }
        100% { background-position: 200% center; }
    }
    @keyframes shimmer-blue {
        0%   { background-position: 0% center; }
        100% { background-position: 200% center; }
    }

    /* Expand button */

    /* Expanded detail row */


    /* Jacket */

    /* Artist info */
</style>