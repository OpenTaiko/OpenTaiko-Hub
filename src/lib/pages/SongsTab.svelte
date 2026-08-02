<script>
    // Dependencies
    import { onMount } from 'svelte';
    import { ProgressBar } from '@skeletonlabs/skeleton';
    import { mkdir, exists, copyFile, remove } from '@tauri-apps/plugin-fs';
    import { openPath } from '@tauri-apps/plugin-opener';
    import { fetch } from "@tauri-apps/plugin-http";
    import { download } from "@tauri-apps/plugin-upload";
    import { path } from '@tauri-apps/api';
    import { invoke, Channel } from '@tauri-apps/api/core';
    import { getContext } from 'svelte';
    const { TriggerError, TriggerWarning, TriggerSuccess, backoffDownload } = getContext('toast');

    import { getSQL } from '$lib/utils/sqljs.js';

    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import { GetGlobalSongsPath, GetTmpPath } from "../utils/path.js";
    import { instances } from "../stores/instances.js";

    // Song management
    import AudioPlayer from '$lib/components/AudioPlayer.svelte';
    import SongDifficultyChip from '$lib/components/SongDifficultyChip.svelte';
    import SongTree from '$lib/components/SongTree.svelte';
    import SongMigrationModal from '$lib/components/SongMigrationModal.svelte';

    // Soundtrack
    const soundtrackInfoUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/soundtrack_info.json';
    let soundtrackInfo = [];
    let catalogFetchFailed = false;
    let currentSongs = {};          // uniqueId → { chartMD5s: string[], chartRelativePath }
    let allScannedSongs = [];       // every scanned song, catalog or custom
    let scannedGenres = {};         // relPath → { title, boxDefSha1, preimageSha1 }
    let scanStats = { dirs: 0, found: 0 };
    let scanning = false;
    let viewMode = 'list';          // 'list' | 'tree'
    let searchSong = "";
    let searchGenre = "";
    let songPreviousSort = "none";
    let songDLProgress = {};
    let songCountProgress = 0;
    let songCountProgressBar = null;
    let bulkBusy = false;          // a bulk download is running
    let songBusy = false;          // any single-song download is running

    // Git blob SHAs of the soundtrack repository (path → sha), fetched once per session
    // and used to detect outdated box.def / default.png files
    let remoteShaMap = null;

    // Songs found inside attached instances that can be moved to the shared library
    let migrationCandidates = [];
    let migrationCandidate = null;   // the instance whose migration modal is open
    let migrationGlobalPath = null;

    // Chart-less folders in an instance that the shared library already provides: the
    // game reads both paths, so each one shows up as a duplicate, empty box
    let duplicateCandidates = [];
    let duplicateBusy = false;

    $: catalogById = new Map(Array.isArray(soundtrackInfo) ? soundtrackInfo.map((s) => [s.uniqueId, s]) : []);

    // Hall of Fame
    const hofDbUrl = 'https://opentaiko.github.io/hof.db3';
    const hofDifficultyMap    = { 0: "Easy", 1: "Normal", 2: "Hard", 3: "Oni", 4: "Edit" };
    const hofDifficultyRevMap = { "Easy": 0, "Normal": 1, "Hard": 2, "Oni": 3, "Edit": 4 };
    const hofDiffShortMap     = { "Easy": "EZ", "Normal": "NM", "Hard": "HD", "Oni": "EX", "Edit": "EXEX" };
    let hofDb = null;
    // uniqueId → { difficultyString → globalRank }
    let hofMap = {};

    // Modal state
    let hofModalOpen = false;
    let hofModalSongInfo = null;
    let hofModalDifficulty = null;
    let hofModalScores = [];
    let hofModalMaxListPoints = 0;

    const ComputeMaxListPoints = (rank) => {
        return parseInt(1000 * Math.pow(0.95, rank - 1));
    };

    const ScoreToListPointsRatio = (score) => {
        const total = score.goodCount + score.okCount + score.badCount;
        if (total === 0) return 0;
        const accuracy = (score.goodCount + score.okCount * 0.5) / total;
        const badRatio = score.badCount / total;
        let ratio = Math.pow(accuracy, 6) * Math.pow(1 - badRatio, 18);
        switch (score.status) {
            case "Perfect":    break;
            case "Full Combo": ratio *= 0.9; break;
            case "Clear":      ratio *= 0.7; break;
            default:           ratio = 0;    break;
        }
        return ratio;
    };

    const updateHoFInfo = async () => {
        try {
            const SQL = await getSQL();

            const response = await fetch(hofDbUrl);
            const buffer = await response.arrayBuffer();
            hofDb?.close();
            hofDb = new SQL.Database(new Uint8Array(buffer));

            // Global rank: all entries sorted by internalDifficultyIndex DESC regardless of difficulty
            const result = hofDb.exec(
                'SELECT uniqueId, difficulty, internalDifficultyIndex FROM entries ORDER BY internalDifficultyIndex DESC'
            );

            if (result.length > 0) {
                let globalRank = 0;
                for (const [uniqueId, difficulty, _idx] of result[0].values) {
                    globalRank++;
                    const diffStr = hofDifficultyMap[difficulty];
                    if (diffStr && uniqueId) {
                        if (!hofMap[uniqueId]) hofMap[uniqueId] = {};
                        hofMap[uniqueId][diffStr] = globalRank;
                    }
                }
            }

            // Patch soundtrackInfo with chartHoFRanks derived from the DB
            soundtrackInfo = soundtrackInfo.map(s => ({
                ...s,
                chartHoFRanks: hofMap[s.uniqueId] ?? {}
            }));
        } catch (e) {
            console.error('Failed to load HoF data:', e);
        }
    };

    const openHoFModal = (songInfo, difficulty) => {
        if (!hofDb) return;
        const rank = hofMap[songInfo.uniqueId]?.[difficulty];
        if (rank === undefined) return;

        const diffInt = hofDifficultyRevMap[difficulty];
        const result = hofDb.exec(
            `SELECT player, status, score, grade, goodCount, okCount, badCount, videoLink, imageLink
             FROM scores WHERE entryId = ? AND difficulty = ?`,
            [songInfo.uniqueId, diffInt]
        );

        const maxListPoints = ComputeMaxListPoints(rank);
        hofModalMaxListPoints = maxListPoints;

        hofModalScores = result.length > 0
            ? result[0].values
                .map((row) => {
                    const s = {
                        player: row[0], status: row[1], score: row[2], grade: row[3],
                        goodCount: row[4], okCount: row[5], badCount: row[6],
                        videoLink: row[7], imageLink: row[8]
                    };
                    s.listPoints = Math.round(maxListPoints * ScoreToListPointsRatio(s));
                    return s;
                })
                .sort((a, b) => b.listPoints - a.listPoints || b.score - a.score)
                .map((s, i) => ({ ...s, rank: i + 1 }))
            : [];

        hofModalSongInfo = songInfo;
        hofModalDifficulty = difficulty;
        hofModalOpen = true;
    };

    const filter1 = (sInfo) => {
        const uids = ["losTPEtAlSwANDERRBHLiXoUNdsetSUnaN"];
        return sInfo.filter(obj => !uids.includes(obj.uniqueId));
    }

    const filter2 = (sInfo) => {
        return sInfo;
    }

    const updateSoundtrackInfo = async () => {
        catalogFetchFailed = false;
        try {
            const response = await fetch(soundtrackInfoUrl);
        if (response.ok) {
            const text = await response.text();
            soundtrackInfo = JSON.parse(text);

            if (navigator.language === "zh-CN") {
                soundtrackInfo = filter1(soundtrackInfo);
            }
            else {
                soundtrackInfo = filter2(soundtrackInfo);
            }
        } else {
            // Keep an ARRAY: template #each and .filter/.sort calls expect one
            soundtrackInfo = [];
            catalogFetchFailed = true;
        }
        } catch (error) {
            soundtrackInfo = [];
            catalogFetchFailed = true;
        }
    }
    
    // Scans the shared Songs library natively (one IPC call, live progress batches)
    const crawlSongs = async () => {
        scanning = true;
        scanStats = { dirs: 0, found: 0 };
        let liveSongs = {};
        let liveAll = [];
        currentSongs = {};
        allScannedSongs = [];

        const registerSong = (map, list, song) => {
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

            const channel = new Channel();
            channel.onmessage = (message) => {
                if (message.type !== 'progress') return;
                scanStats = { dirs: message.scannedDirs, found: message.songsFound };
                if (message.batch?.length) {
                    for (const song of message.batch) registerSong(liveSongs, liveAll, song);
                    currentSongs = liveSongs;
                    allScannedSongs = liveAll;
                }
            };

            const result = await invoke('scan_songs', { baseDir: baseDirPath, onEvent: channel });

            // The command result is authoritative; events were only for live display
            const finalSongs = {};
            const finalAll = [];
            for (const song of result.songs) registerSong(finalSongs, finalAll, song);
            currentSongs = finalSongs;
            allScannedSongs = finalAll;
            scannedGenres = Object.fromEntries(result.genres.map((genre) => [genre.relPath, genre]));
            scanStats = { dirs: scanStats.dirs, found: finalAll.length };
        } catch (error) {
            console.error('Song scan failed:', error);
            TriggerError(get(_)('songs.error.scan_failed', { values: { error } }));
        }
        scanning = false;
        CheckMigrations();
    }

    // Looks inside every attached (non-experimental) instance for songs that could be
    // moved to the shared library
    const CheckMigrations = async () => {
        const candidates = [];
        try {
            const globalSongs = await GetGlobalSongsPath();
            const norm = (p) => p.replace(/\//g, '\\').replace(/[\\]+$/, '').toLowerCase();
            for (const inst of get(instances)) {
                const instSongs = await path.join(inst.path, 'Songs');
                if (norm(instSongs) === norm(globalSongs)) continue;
                try {
                    const probe = new Channel();
                    const result = await invoke('scan_songs', { baseDir: instSongs, onEvent: probe });
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
        const found = [];
        try {
            const globalSongs = await GetGlobalSongsPath();
            for (const inst of get(instances)) {
                const instSongs = await path.join(inst.path, 'Songs');
                try {
                    const folders = await invoke('find_duplicate_song_folders', {
                        instanceSongs: instSongs,
                        globalSongs
                    });
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

    const CleanDuplicates = async (candidate) => {
        if (duplicateBusy) return;
        duplicateBusy = true;
        try {
            const removed = await invoke('remove_duplicate_song_folders', {
                instanceSongs: candidate.srcPath,
                globalSongs: candidate.globalSongs,
                relPaths: candidate.folders.map((f) => f.relPath)
            });
            TriggerSuccess(get(_)('songs.duplicates.success', { values: { count: removed } }));
            duplicateCandidates = duplicateCandidates.filter((c) => c !== candidate);
        } catch (error) {
            TriggerError(get(_)('songs.duplicates.error', { values: { error } }));
        }
        duplicateBusy = false;
    }

    // Opens the resolve/transfer modal for one instance (conflicts are decided there)
    const OpenMigration = async (candidate) => {
        migrationGlobalPath = await GetGlobalSongsPath();
        migrationCandidate = candidate;
    }

    // Called by the modal after a plan was applied; the resolved instance is emptied,
    // so it drops out of the candidate list and won't prompt again.
    const OnMigrationApplied = (candidate) => {
        migrationCandidates = migrationCandidates.filter((c) => c !== candidate);
        migrationCandidate = null;
        crawlSongs();
    }

    const OpenSongsFolder = async () => {
        try {
            const songsDir = await GetGlobalSongsPath();
            await mkdir(songsDir, { recursive: true });
            await openPath(songsDir);
        } catch (error) {
            TriggerError(get(_)('home.error.launch', { values: { error } }));
        }
    }

    // Fetches the soundtrack repository's git tree once so local box.def / default.png
    // files can be compared against their upstream version by git blob SHA
    const EnsureRemoteShaMap = async () => {
        if (remoteShaMap) return remoteShaMap;
        try {
            const response = await fetch('https://api.github.com/repos/OpenTaiko/OpenTaiko-Soundtrack/git/trees/main?recursive=1');
            if (response.ok) {
                const data = await response.json();
                remoteShaMap = new Map(
                    (data.tree ?? [])
                        .filter((entry) => entry.type === 'blob')
                        .map((entry) => [entry.path, entry.sha])
                );
            }
        } catch (error) {
            console.error('Failed to fetch the soundtrack repository tree:', error);
        }
        return remoteShaMap;
    }


    $: GetFilteredSInfo = (SInfo) => {
        const bInNameFilter = SInfo.chartTitle.toLowerCase().includes(searchSong.toLowerCase()) || SInfo.chartSubtitle?.toLowerCase().includes(searchSong.toLowerCase());
        const bInGenreFilter = SInfo.tjaGenreFolder.toLowerCase().includes(searchGenre.toLowerCase());

        return bInGenreFilter && bInNameFilter;
    }

    // Reactive so status cells re-render as scan results stream in
    $: IsSongUpToDate = (SInfo) => {
        const localSong = currentSongs[SInfo.uniqueId];
        return !!localSong && (localSong.chartMD5s ?? []).includes(SInfo.tjaMD5);
    }

    const GetFilteredAvailableSInfo = (SInfo) => {
        return !IsSongUpToDate(SInfo) && GetFilteredSInfo(SInfo);
    }

    const UndefinedToMinusOne = (val) => {
        return (val === undefined || val === null) ? -1 : val;
    }

    const AlterValueTowerDan = (a, b, og) => {
        const aTD = (a.chartDifficulties.Tower !== undefined || a.chartDifficulties.Dan !== undefined);
        const bTD = (b.chartDifficulties.Tower !== undefined || b.chartDifficulties.Dan !== undefined);
        if (aTD) return 2147483647;
        else if (bTD) return -2147483648;
        return og;
    }

    const SortSongsByColumn = (column) => {
        const wasClickedPreviously = `${column} asc` === songPreviousSort;
        const mult = (wasClickedPreviously) ? -1 : 1;
        songPreviousSort = (wasClickedPreviously) ? `${column} desc` : `${column} asc`;

        switch (column) {
            default:
            case ("name"): 
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => mult * a.chartTitle.localeCompare(b.chartTitle));
                break;
            }
            case ("genre"): 
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => mult * a.tjaGenreFolder.localeCompare(b.tjaGenreFolder));
                break;
            }
            case ("size"):
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => mult * (a.chartSize - b.chartSize));
                break;
            }
            case ("ez"):
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => AlterValueTowerDan(a, b, mult * (UndefinedToMinusOne(a.chartDifficulties.Easy) - UndefinedToMinusOne(b.chartDifficulties.Easy))));
                break;
            }
            case ("nm"):
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => AlterValueTowerDan(a, b, mult * (UndefinedToMinusOne(a.chartDifficulties.Normal) - UndefinedToMinusOne(b.chartDifficulties.Normal))));
                break;
            }
            case ("hd"):
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => AlterValueTowerDan(a, b, mult * (UndefinedToMinusOne(a.chartDifficulties.Hard) - UndefinedToMinusOne(b.chartDifficulties.Hard))));
                break;
            }
            case ("ex"):
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => AlterValueTowerDan(a, b, mult * (UndefinedToMinusOne(a.chartDifficulties.Oni) - UndefinedToMinusOne(b.chartDifficulties.Oni))));
                break;
            }
            case ("exex"):
            {
                soundtrackInfo = soundtrackInfo.sort((a, b) => AlterValueTowerDan(a, b, mult * (UndefinedToMinusOne(a.chartDifficulties.Edit) - UndefinedToMinusOne(b.chartDifficulties.Edit))));
                break;
            }
        }
    }

    const DownloadDisplayedSongs = async () => {
        if (scanning === true) {
            TriggerError(get(_)('songs.error.scanning'));
            return ;
        }
        // Guard the whole run, not just the progress-bar phase: the first steps are
        // async, so without this a second click would start a parallel bulk download
        if (bulkBusy) return;
        bulkBusy = true;

        try {
            const filteredSInfo = soundtrackInfo.filter((SInfo) => GetFilteredAvailableSInfo(SInfo));

            const songCount = filteredSInfo.length;

            // Load the remote file index once so genre metadata can be checked for updates
            await EnsureRemoteShaMap();

            if (songCount === 0) {
                const updated = await RefreshAllGenreMetadata();
                if (updated > 0) {
                    TriggerSuccess(get(_)('songs.metadata.updated', { values: { count: updated } }));
                } else {
                    TriggerSuccess(get(_)('songs.success.all_up_to_date'));
                }
                return ;
            }

            songCountProgress = 0;
            for (const SInfo of filteredSInfo) {
                songCountProgressBar = 100 * (songCountProgress / songCount);

                console.log(`Downloading song ${songCountProgress + 1} out of ${songCount}...`);
                console.log(SInfo);

                let curObj = null;
                if (Object.keys(currentSongs).includes(SInfo.uniqueId)) curObj = currentSongs[SInfo.uniqueId];

                await DownloadSong(SInfo, curObj, songCountProgress + 1, songCount);
                songCountProgress++;
            }

            // Refresh outdated box.def / default.png files across the whole library
            const updated = await RefreshAllGenreMetadata();
            if (updated > 0) {
                TriggerSuccess(get(_)('songs.metadata.updated', { values: { count: updated } }));
            }
        } finally {
            songCountProgressBar = null;
            bulkBusy = false;
        }
    }

    // Downloads box.def / default.png for a genre folder when it is missing or its git
    // blob SHA differs from the soundtrack repository. Without the remote index the
    // old behavior is kept (box.def always refreshed, preimage only when missing).
    const EnsureGenreMetadata = async (genrePath, tmpFolder) => {
        const baseDirPath = await GetGlobalSongsPath();
        const genreFullPath = await path.join(baseDirPath, genrePath);
        let changed = false;

        for (const fileName of ['box.def', 'default.png']) {
            const destPath = await path.join(genreFullPath, fileName);
            const localGenre = scannedGenres[genrePath];
            const localSha = fileName === 'box.def' ? localGenre?.boxDefSha1 : localGenre?.preimageSha1;
            const remoteSha = remoteShaMap?.get(`${genrePath}/${fileName}`);

            let needsDownload;
            if (remoteShaMap) {
                needsDownload = !!remoteSha && localSha !== remoteSha;
            } else {
                needsDownload = fileName === 'box.def' ? true : !(await exists(destPath));
            }
            if (!needsDownload) continue;

            const _url = `https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/${genrePath}/${fileName}`;
            const dlPath = await path.join(tmpFolder, fileName);
            const resourceExists = remoteShaMap ? true : await fetch(_url).then(res => res.ok).catch(() => false);
            if (!resourceExists) continue;

            try {
                await download(_url, dlPath);
                await copyFile(dlPath, destPath);
                changed = true;
                if (remoteSha) {
                    scannedGenres[genrePath] = {
                        ...(scannedGenres[genrePath] ?? { relPath: genrePath, title: null, boxDefSha1: null, preimageSha1: null }),
                        [fileName === 'box.def' ? 'boxDefSha1' : 'preimageSha1']: remoteSha
                    };
                }
            } catch (error) {
                console.error(`Failed to update ${genrePath}/${fileName}:`, error);
            }
        }
        return changed;
    }

    const RefreshAllGenreMetadata = async () => {
        if (!remoteShaMap) return 0;
        const tmpFolder = await GetTmpPath(crypto.randomUUID());
        await mkdir(tmpFolder, { recursive: true });
        let updated = 0;
        for (const genrePath of Object.keys(scannedGenres)) {
            if (await EnsureGenreMetadata(genrePath, tmpFolder)) updated++;
        }
        try { await remove(tmpFolder, { recursive: true }); } catch {}
        return updated;
    }

    const DownloadSong = async (songObj, currentObj, songNb = undefined, songTotal = undefined) => {
        // Never write into the library while it is still being scanned, the scan's
        // final result would otherwise clobber this download's bookkeeping.
        if (scanning) {
            TriggerError(get(_)('songs.error.scanning'));
            return;
        }
        // songNb is set when the bulk loop drives this, which owns the busy flag itself
        const standalone = songNb === undefined;
        if (standalone) {
            if (songBusy || bulkBusy) return;
            songBusy = true;
        }
        try {
            await RunDownloadSong(songObj, currentObj, songNb, songTotal);
        } finally {
            if (standalone) songBusy = false;
        }
    }

    const RunDownloadSong = async (songObj, currentObj, songNb, songTotal) => {
        songDLProgress[songObj.uniqueId] = 0;
        //console.log(songDLProgress);

        await EnsureRemoteShaMap();

        const baseDirPath = await GetGlobalSongsPath();
        const localRelPath = ((currentObj !== null) ? currentObj.chartRelativePath : songObj.tjaFolderPath).replace(/\\/g, '/');
        const tjaFullPath = await path.join(baseDirPath, localRelPath);

        let fold_exists = await exists(tjaFullPath);
        if (!fold_exists)
            await mkdir(tjaFullPath, {recursive: true});

        const chartDownloadFolder = await GetTmpPath(crypto.randomUUID());

        fold_exists = await exists(chartDownloadFolder);
        if (!fold_exists)
            await mkdir(chartDownloadFolder, {recursive: true});

        let fileNames = [];

        let totbyts = 0;
        for (const filePath of songObj.tjaFilesPath) {
            // forbid non-children paths
            let localFilePath = (filePath.startsWith(songObj.tjaFolderPath + '\\') || filePath.startsWith(songObj.tjaFolderPath + '/')) ?
                filePath.slice(songObj.tjaFolderPath.length + 1)
                : filePath.split("\\").pop();

            const tjaFileUrl = `https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/${filePath}`;
            const dlPath = await path.join(chartDownloadFolder, localFilePath.replace(/\\/g, '/'));

            // ensure subdirectory exists
            const dlPathFold = await path.dirname(dlPath);
            if (!await exists(dlPathFold))
                await mkdir(dlPathFold, {recursive: true});

            const success = await backoffDownload(
                tjaFileUrl,
                dlPath,
                (pr) => {
                    totbyts += pr.progress;
                    songDLProgress[songObj.uniqueId] = 100 * (totbyts / (songObj.chartSize * 1024 * 1024));
                    //console.log(songDLProgress);
                }
            );

            if (!success) {
                delete songDLProgress[songObj.uniqueId];
                return ;
            }

            fileNames.push(localFilePath);
        };

        songDLProgress[songObj.uniqueId] = 0;
        await Promise.all(fileNames.map(async (fn, idx) => {
            const strPath = await path.join(chartDownloadFolder, fn.replace(/\\/g, '/'));
            const destPath = await path.join(tjaFullPath, fn.replace(/\\/g, '/'));

            // ensure subdirectory exists
            const destPathDir = await path.dirname(destPath);
            if (!await exists(destPathDir))
                await mkdir(destPathDir, {recursive: true});

            await copyFile(strPath, destPath);
            songDLProgress[songObj.uniqueId] = (idx + 1) * (100 / fileNames.length);
            //console.log(songDLProgress);
        }));

        // Download / refresh box.def and default.png of every ancestor genre folder
        const genrePaths = songObj.tjaFolderPath.split('\\').slice(0, -1).map((_, i, arr) => arr.slice(0, i + 1).join('/'));

        for (const genrePath of genrePaths) {
            const genreFullPath = await path.join(baseDirPath, genrePath);
            if (!await exists(genreFullPath))
                await mkdir(genreFullPath, {recursive: true});
            await EnsureGenreMetadata(genrePath, chartDownloadFolder);
        }

        // Clean after pooping
        await remove(chartDownloadFolder, { recursive: true });

        if (songNb === undefined)
            TriggerSuccess(get(_)('songs.success.download_complete'));
        else
            TriggerSuccess(get(_)('songs.success.download_nb', { values: { nb: songNb, total: songTotal } }));

        //crawlSongs();
        currentSongs[songObj.uniqueId] = {
            chartMD5s: [songObj.tjaMD5],
            // Keep the actual install location when the song was relocated by the user
            chartRelativePath: (currentObj !== null) ? currentObj.chartRelativePath : songObj.tjaFolderPath
        };

        delete songDLProgress[songObj.uniqueId];
        //console.log(songDLProgress);
    }

    // Artists info
    const artistsDbUrl = 'https://opentaiko.github.io/artists_info.db3';
    let songArtistsMap = {}; // songUid → { artists: ArtistObj[], link }
    let expandedSongUid = null;

    const updateArtistInfo = async () => {
        let db = null;
        try {
            const SQL = await getSQL();
            const response = await fetch(artistsDbUrl);
            const buffer = await response.arrayBuffer();
            db = new SQL.Database(new Uint8Array(buffer));

            const artistsResult = db.exec('SELECT entryId, artist, youtube, soundcloud, spotify, bandcamp, bilibili, other FROM artists');
            const artistsById = {};
            if (artistsResult.length > 0) {
                for (const [entryId, artist, youtube, soundcloud, spotify, bandcamp, bilibili, other] of artistsResult[0].values) {
                    artistsById[entryId] = { artist, youtube, soundcloud, spotify, bandcamp, bilibili, other };
                }
            }

            const songsResult = db.exec('SELECT songUid, artists, link FROM songs');
            if (songsResult.length > 0) {
                for (const [songUid, artistsJson, link] of songsResult[0].values) {
                    const artistIds = JSON.parse(artistsJson || '[]');
                    const artists = artistIds.map(id => artistsById[id]).filter(Boolean);
                    songArtistsMap[songUid] = { artists, link };
                }
            }
            songArtistsMap = songArtistsMap;
        } catch (e) {
            console.error('Failed to load artist info:', e);
        } finally {
            db?.close();
        }
    };

    const toggleExpand = (uid) => {
        expandedSongUid = expandedSongUid === uid ? null : uid;
    };

    onMount(async () => {
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
	<button type="button" class="button-red button-main" on:click={updateSoundtrackInfo}>
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
	<button type="button" class="button-green button-main" disabled={duplicateBusy} on:click={() => CleanDuplicates(candidate)}>
		<i class="fa-solid fa-broom"></i> {$_('songs.duplicates.button')}
	</button>
	<button type="button" class="button-gray button-main" on:click={() => duplicateCandidates = duplicateCandidates.filter((c) => c !== candidate)}>
		{$_('songs.migrate.later')}
	</button>
</aside>
{/each}

{#each migrationCandidates as candidate (candidate.instance.id)}
<aside class="card p-3 mb-2 flex items-center gap-3 flex-wrap">
	<i class="fa-solid fa-boxes-packing"></i>
	<span class="flex-1">{$_('songs.migrate.banner', { values: { count: candidate.count, instance: candidate.instance.name } })}</span>
	<button type="button" class="button-green button-main" disabled={scanning || bulkBusy || songBusy || migrationCandidate !== null} on:click={() => OpenMigration(candidate)}>
		<i class="fa-solid fa-right-left"></i> {$_('songs.migrate.button')}
	</button>
	<button type="button" class="button-gray button-main" on:click={() => migrationCandidates = migrationCandidates.filter((c) => c !== candidate)}>
		{$_('songs.migrate.later')}
	</button>
</aside>
{/each}

<div class="card bg-surface-100-800-token p-3 mb-2 flex items-center gap-3 flex-wrap">
	{#if scanning}
		<div class="flex-1 flex items-center gap-3 min-w-[16rem]">
			<ProgressBar />
			<span class="whitespace-nowrap text-sm">{$_('songs.scan.progress', { values: { count: scanStats.found } })}</span>
		</div>
	{:else}
		<span class="text-sm">{$_('songs.scan.done', { values: { count: allScannedSongs.length } })}</span>
		<button type="button" class="button-blue button-main" on:click={crawlSongs}><i class="fa-solid fa-rotate"></i> {$_('common.reload')}</button>
		<button type="button" class="button-blue button-main" on:click={OpenSongsFolder}><i class="fa-solid fa-folder-open"></i> {$_('songs.button.open_folder')}</button>
		<span class="flex-1"></span>
	{/if}
	<button type="button" class="button-{viewMode === 'list' ? 'gray' : 'blue'} button-main" on:click={() => viewMode = 'list'}>
		<i class="fa-solid fa-list"></i> {$_('songs.view.list')}
	</button>
	<button type="button" class="button-{viewMode === 'tree' ? 'gray' : 'blue'} button-main" on:click={() => viewMode = 'tree'}>
		<i class="fa-solid fa-folder-tree"></i> {$_('songs.view.tree')}
	</button>
</div>

{#if viewMode === 'tree'}
<SongTree Songs={allScannedSongs} Genres={scannedGenres} CatalogById={catalogById} />
{:else}
<div class="table-container text-token">
	<table class="table table-hover">
		<thead>
			<tr>
				<th><button on:click={() => SortSongsByColumn("name")}>{$_('songs.col.name')}</button></th>
				<th><button on:click={() => SortSongsByColumn("genre")}>{$_('songs.col.folder')}</button></th>
				<th colspan="5" class="w-1/5">{$_('songs.col.difficulties')}</th>
				<th><button on:click={() => SortSongsByColumn("size")}>{$_('songs.col.size')}</button></th>
				<th class="w-1/6">{$_('songs.col.status')}</th>
			</tr>
			<tr>
				<th><input class="w-full rounded-md px-3 py-2 text-blue-950" placeholder={$_('songs.filter.song')} bind:value={searchSong}></th>
				<th><input class="w-full rounded-md px-3 py-2 text-blue-950" placeholder={$_('songs.filter.folder')} bind:value={searchGenre}></th>
				<th><button on:click={() => SortSongsByColumn("ez")}>EZ</button></th>
				<th><button on:click={() => SortSongsByColumn("nm")}>NM</button></th>
				<th><button on:click={() => SortSongsByColumn("hd")}>HD</button></th>
				<th><button on:click={() => SortSongsByColumn("ex")}>EX</button></th>
				<th><button on:click={() => SortSongsByColumn("exex")}>EXEX</button></th>
				<th></th>
				<th>
					{#if songCountProgressBar !== null}
					<ProgressBar bind:value={songCountProgressBar} max={100} />
					{:else}
					<button type="button" disabled={bulkBusy || songBusy} on:click={DownloadDisplayedSongs} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('songs.button.bulk_download')}</button>
					{/if}
				</th>
			</tr>
		</thead>
		<tbody>
			{#each soundtrackInfo as songInfo}
			{#if GetFilteredSInfo(songInfo)}
			<tr class:row-expanded={expandedSongUid === songInfo.uniqueId}>
				<td>
					<div class="flex items-center gap-2">
						<button class="expand-btn" on:click={() => toggleExpand(songInfo.uniqueId)} aria-label="expand">
							<i class="fa-solid fa-chevron-{expandedSongUid === songInfo.uniqueId ? 'down' : 'right'}"></i>
						</button>
						<div class="flex-1"><AudioPlayer {songInfo} /></div>
					</div>
				</td>
				<td>{songInfo.tjaGenreFolder}</td>
				{#if songInfo.chartDifficulties.Dan !== undefined}
				<td colspan="5">
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Dan"/>
				</td>
				{:else if songInfo.chartDifficulties.Tower !== undefined}
				<td colspan="5">
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Tower"/>
				</td>
				{:else}
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Easy" OnCrownClick={openHoFModal}/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Normal" OnCrownClick={openHoFModal}/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Hard" OnCrownClick={openHoFModal}/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Oni" OnCrownClick={openHoFModal}/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Edit" OnCrownClick={openHoFModal}/>
				</td>
				{/if}
				<td>{songInfo.chartSize}Mb</td>
				<!-- songDLProgress[songObj.uniqueId] -->
				{#if scanning === true && !Object.keys(currentSongs).includes(songInfo.uniqueId)}
				<td>
					<p>{$_('songs.status.scanning')}</p>
				</td>
				{:else if !Object.keys(currentSongs).includes(songInfo.uniqueId)}
				<td>
					<p>{$_('songs.status.not_downloaded')}</p>
					<br />
					{#if songDLProgress[songInfo.uniqueId] === undefined}
					<button type="button" disabled={bulkBusy || songBusy} on:click={() => DownloadSong(songInfo, null)} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('songs.button.download')}</button>
					{:else}
					<ProgressBar bind:value={songDLProgress[songInfo.uniqueId]} max={100} />
					{/if}
				</td>
				{:else if IsSongUpToDate(songInfo)}
				<td>
					<p>{$_('songs.status.up_to_date')}</p>
                    <br />
                    {#if songDLProgress[songInfo.uniqueId] === undefined}
					<button type="button" disabled={bulkBusy || songBusy} on:click={() => DownloadSong(songInfo, currentSongs[songInfo.uniqueId])} class="button-gray button-main"><i class="fa-solid fa-download"></i> {$_('songs.button.redownload')}</button>
					{:else}
					<ProgressBar bind:value={songDLProgress[songInfo.uniqueId]} max={100} />
					{/if}
				</td>
				{:else}
				<td>
					<p>{$_('songs.status.outdated')}</p>
					<br />
					{#if songDLProgress[songInfo.uniqueId] === undefined}
					<button type="button" disabled={bulkBusy || songBusy} on:click={() => DownloadSong(songInfo, currentSongs[songInfo.uniqueId])} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('songs.button.update')}</button>
					{:else}
					<ProgressBar bind:value={songDLProgress[songInfo.uniqueId]} max={100} />
					{/if}
				</td>
				{/if}
			</tr>
			{#if expandedSongUid === songInfo.uniqueId}
				{@const artistData = songArtistsMap[songInfo.uniqueId]}
			<tr class="expanded-detail-row">
				<td colspan="9">
					<div class="song-detail-box">
						<!-- Jacket -->
						<div class="song-jacket">
							{#if songInfo.chartJacketFilePath}
								<img
									src="https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/{songInfo.chartJacketFilePath}"
									alt="jacket"
								/>
							{:else}
								<div class="no-jacket"><span>No jacket</span></div>
							{/if}
						</div>
						<!-- Artist info -->
						<div class="song-artist-info">
							{#if artistData && artistData.artists.length > 0}
								{#each artistData.artists as artist}
									<div class="artist-entry">
										<span class="artist-name">{artist.artist}</span>
										<div class="artist-links">
											{#if artist.youtube}   <a href={artist.youtube}   target="_blank" class="artist-link link-youtube">  <i class="fa-brands fa-youtube"></i>    YouTube</a>   {/if}
											{#if artist.soundcloud}<a href={artist.soundcloud} target="_blank" class="artist-link link-soundcloud"><i class="fa-brands fa-soundcloud"></i> SoundCloud</a>{/if}
											{#if artist.spotify}   <a href={artist.spotify}   target="_blank" class="artist-link link-spotify">  <i class="fa-brands fa-spotify"></i>    Spotify</a>   {/if}
											{#if artist.bandcamp}  <a href={artist.bandcamp}  target="_blank" class="artist-link link-bandcamp"> <i class="fa-brands fa-bandcamp"></i>   Bandcamp</a>  {/if}
											{#if artist.bilibili}  <a href={artist.bilibili}  target="_blank" class="artist-link link-bilibili"> <i class="fa-solid fa-tv"></i>          Bilibili</a>  {/if}
											{#if artist.other}     <a href={artist.other}     target="_blank" class="artist-link link-other">    <i class="fa-solid fa-link"></i>        Website</a>   {/if}
										</div>
									</div>
								{/each}
								{#if artistData.link}
									<a href={artistData.link} target="_blank" class="artist-link link-other mt-1"><i class="fa-solid fa-music"></i> Song link</a>
								{/if}
							{:else}
								<span class="opacity-50 italic">No artist information available.</span>
							{/if}
						</div>
					</div>
				</td>
			</tr>
			{/if}
			{/if}
			{/each}
		</tbody>
	</table>
</div>
{/if}

{#if hofModalOpen && hofModalSongInfo}
<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="modal-backdrop" on:click={() => hofModalOpen = false}>
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
    <div class="card p-6 space-y-4 modal-card" on:click|stopPropagation>
        <div class="flex justify-between items-center">
            <h2 class="h3">Hall of Fame — {hofModalSongInfo.chartTitle} ({hofDiffShortMap[hofModalDifficulty]} #{hofMap[hofModalSongInfo.uniqueId]?.[hofModalDifficulty]})</h2>
            <button class="btn-icon btn-icon-sm variant-filled" on:click={() => hofModalOpen = false} aria-label={$_('hof.close')}>✕</button>
        </div>
        <div class="flex flex-col gap-1">
            <a href="https://opentaiko.github.io/songinfo/{hofModalSongInfo.uniqueId}?d={hofDifficultyRevMap[hofModalDifficulty]}" target="_blank" class="text-blue-600 underline">
                {$_('hof.website_link')} <i class="fa-solid fa-arrow-up-right-from-square"></i>
            </a>
            <span class="text-sm opacity-70">{$_('hof.max_list_points')}: <b>{hofModalMaxListPoints}</b></span>
        </div>
        {#if hofModalScores.length === 0}
            <p>{$_('hof.no_scores')}</p>
        {:else}
        <div class="table-container">
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
                        <th>LP</th>
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
                        <td>{s.score?.toLocaleString()}</td>
                        <td class="grade grade-{gradeClass}">{gradeDisplay}</td>
                        <td class="status status-{s.status?.replace(' ', '-')}">{s.status === 'Perfect' ? $_('hof.status.perfect') : s.status === 'Full Combo' ? $_('hof.status.full_combo') : s.status === 'Clear' ? $_('hof.status.clear') : (s.status ?? '—')}</td>
                        <td>{s.goodCount}</td>
                        <td>{s.okCount}</td>
                        <td>{s.badCount}</td>
                        <td>{s.listPoints?.toLocaleString()}</td>
                        <td>
                            {#if s.videoLink}
                                <a href={s.videoLink} target="_blank" class="text-blue-600 underline">{$_('hof.video_link')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a>
                            {:else}—{/if}
                        </td>
                    </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        {/if}
    </div>
</div>
{/if}

{#if migrationCandidate}
<SongMigrationModal
	Candidate={migrationCandidate}
	GlobalPath={migrationGlobalPath}
	CatalogById={catalogById}
	OnClose={() => migrationCandidate = null}
	OnApplied={() => OnMigrationApplied(migrationCandidate)}
/>
{/if}

<style>
    .modal-backdrop {
        position: fixed;
        inset: 0;
        background: rgba(0,0,0,0.6);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
    }
    .modal-card {
        max-width: 800px;
        width: 90%;
        max-height: 80vh;
        overflow-y: auto;
    }

    /* Grade letter colors */
    .grade { font-weight: bold; }
    .grade-E { color: #ffffff; }
    .grade-D { color: #ff4444; }
    .grade-C { color: #ff9933; }
    .grade-B { color: #ffdd00; }
    .grade-A { color: #44cc44; }
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
        background: linear-gradient(90deg, #1a237e, #3949ab, #5c6bc0, #3949ab, #1a237e);
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
    .expand-btn {
        flex-shrink: 0;
        width: 1.6rem;
        height: 1.6rem;
        border-radius: 0.3rem;
        opacity: 0.6;
        transition: opacity 0.15s;
    }
    .expand-btn:hover { opacity: 1; }

    /* Expanded detail row */
    .expanded-detail-row td { padding: 0 !important; }

    .song-detail-box {
        display: flex;
        flex-direction: row;
        gap: 1rem;
        padding: 0.75rem 1rem;
        border-top: 1px solid rgba(128,128,128,0.2);
    }

    /* Jacket */
    .song-jacket {
        flex-shrink: 0;
        width: 96px;
        height: 96px;
        border-radius: 0.4rem;
        overflow: hidden;
        border: 1px solid rgba(128,128,128,0.3);
    }
    .song-jacket img {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }
    .no-jacket {
        width: 100%;
        height: 100%;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(128,128,128,0.15);
        font-size: 0.7rem;
        opacity: 0.5;
        font-style: italic;
    }

    /* Artist info */
    .song-artist-info {
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
        justify-content: center;
    }
    .artist-entry {
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
    }
    .artist-name {
        font-weight: 600;
        font-size: 0.95rem;
    }
    .artist-links {
        display: flex;
        flex-wrap: wrap;
        gap: 0.3rem;
    }
    .artist-link {
        display: inline-flex;
        align-items: center;
        gap: 0.3rem;
        padding: 0.15rem 0.5rem;
        border-radius: 0.3rem;
        font-size: 0.78rem;
        font-weight: 500;
        color: #fff;
        text-decoration: none;
        transition: opacity 0.15s;
    }
    .artist-link:hover { opacity: 0.8; }
    .link-youtube   { background: #ff0000; }
    .link-soundcloud{ background: #ff5500; }
    .link-spotify   { background: #1db954; }
    .link-bandcamp  { background: #1da0c3; }
    .link-bilibili  { background: #00a1d6; }
    .link-other     { background: #555; }
</style>