<script>
    // Dependencies
    import { onMount } from 'svelte';
    import { ProgressBar } from '@skeletonlabs/skeleton';
    import { readFile, readTextFile, mkdir, readDir, exists, copyFile, remove } from '@tauri-apps/plugin-fs';
    import { fetch } from "@tauri-apps/plugin-http";
    import { download } from "@tauri-apps/plugin-upload";
    import { path } from '@tauri-apps/api';
    import { getContext } from 'svelte';
    const { TriggerError, TriggerWarning, TriggerSuccess, backoffDownload } = getContext('toast');

    import { md5 } from 'js-md5';

    import { GetRootPath } from "../utils/path.js";

    // Song management
    import AudioPlayer from '$lib/components/AudioPlayer.svelte';
    import SongDifficultyChip from '$lib/components/SongDifficultyChip.svelte';

    // Soundtrack
    const soundtrackInfoUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/soundtrack_info.json';
    let soundtrackInfo = [];
    let currentSongs = {};
    let scanning = false;
    let searchSong = "";
    let searchGenre = "";
    let songPreviousSort = "none";
    let songDLProgress = {};
    let songCountProgress = 0;
    let songCountProgressBar = null;

    const updateSoundtrackInfo = async () => {
        try {
            const response = await fetch(soundtrackInfoUrl);
        if (response.ok) {
            const text = await response.text();
            soundtrackInfo = JSON.parse(text);
        } else {
            soundtrackInfo = {};
        }
        } catch (error) {
            soundtrackInfo = {};
        }
    }
    
    const crawlSongs = async () => {
        const res = await GetRootPath();
        const baseDir = './OpenTaiko/Songs';
        scanning = true;
        
        async function folderExists(folderPath) {
            try {
                const fullPath = await path.join(res, folderPath);
                const entries = await readDir(fullPath);
                return true;
            } catch (error) {
                // Directory does not exist
                console.log(error)
                return false;
            }
        }

        async function processFolder(folderPath) {
            try {
                const fullPath = await path.join(res, folderPath);
                const entries = await readDir(fullPath, { recursive: true });

                for (const entry of entries) {
                    //console.log(entry)
                    if (entry.isDirectory) {
                        // If it's a folder, process it recursively
                        await processFolder([folderPath, entry.name].join("/"));
                    } else if (entry.name.endsWith('.tja')) {
                        // If it's a .tja file, process it
                        const tjaPath = [folderPath, entry.name].join("/");
                        //const folderPath = tjaPath.substring(0, tjaPath.lastIndexOf('/'));
                        const relativePath = folderPath.replace(`${baseDir}/`, '');

                        // Read the uniqueID.json file in the same folder
                        const uniqueIdFile = `${folderPath}/uniqueID.json`;
                        try {
                            let re = /[\0-\x1F\x7F-\x9F\xAD\u0378\u0379\u037F-\u0383\u038B\u038D\u03A2\u0528-\u0530\u0557\u0558\u0560\u0588\u058B-\u058E\u0590\u05C8-\u05CF\u05EB-\u05EF\u05F5-\u0605\u061C\u061D\u06DD\u070E\u070F\u074B\u074C\u07B2-\u07BF\u07FB-\u07FF\u082E\u082F\u083F\u085C\u085D\u085F-\u089F\u08A1\u08AD-\u08E3\u08FF\u0978\u0980\u0984\u098D\u098E\u0991\u0992\u09A9\u09B1\u09B3-\u09B5\u09BA\u09BB\u09C5\u09C6\u09C9\u09CA\u09CF-\u09D6\u09D8-\u09DB\u09DE\u09E4\u09E5\u09FC-\u0A00\u0A04\u0A0B-\u0A0E\u0A11\u0A12\u0A29\u0A31\u0A34\u0A37\u0A3A\u0A3B\u0A3D\u0A43-\u0A46\u0A49\u0A4A\u0A4E-\u0A50\u0A52-\u0A58\u0A5D\u0A5F-\u0A65\u0A76-\u0A80\u0A84\u0A8E\u0A92\u0AA9\u0AB1\u0AB4\u0ABA\u0ABB\u0AC6\u0ACA\u0ACE\u0ACF\u0AD1-\u0ADF\u0AE4\u0AE5\u0AF2-\u0B00\u0B04\u0B0D\u0B0E\u0B11\u0B12\u0B29\u0B31\u0B34\u0B3A\u0B3B\u0B45\u0B46\u0B49\u0B4A\u0B4E-\u0B55\u0B58-\u0B5B\u0B5E\u0B64\u0B65\u0B78-\u0B81\u0B84\u0B8B-\u0B8D\u0B91\u0B96-\u0B98\u0B9B\u0B9D\u0BA0-\u0BA2\u0BA5-\u0BA7\u0BAB-\u0BAD\u0BBA-\u0BBD\u0BC3-\u0BC5\u0BC9\u0BCE\u0BCF\u0BD1-\u0BD6\u0BD8-\u0BE5\u0BFB-\u0C00\u0C04\u0C0D\u0C11\u0C29\u0C34\u0C3A-\u0C3C\u0C45\u0C49\u0C4E-\u0C54\u0C57\u0C5A-\u0C5F\u0C64\u0C65\u0C70-\u0C77\u0C80\u0C81\u0C84\u0C8D\u0C91\u0CA9\u0CB4\u0CBA\u0CBB\u0CC5\u0CC9\u0CCE-\u0CD4\u0CD7-\u0CDD\u0CDF\u0CE4\u0CE5\u0CF0\u0CF3-\u0D01\u0D04\u0D0D\u0D11\u0D3B\u0D3C\u0D45\u0D49\u0D4F-\u0D56\u0D58-\u0D5F\u0D64\u0D65\u0D76-\u0D78\u0D80\u0D81\u0D84\u0D97-\u0D99\u0DB2\u0DBC\u0DBE\u0DBF\u0DC7-\u0DC9\u0DCB-\u0DCE\u0DD5\u0DD7\u0DE0-\u0DF1\u0DF5-\u0E00\u0E3B-\u0E3E\u0E5C-\u0E80\u0E83\u0E85\u0E86\u0E89\u0E8B\u0E8C\u0E8E-\u0E93\u0E98\u0EA0\u0EA4\u0EA6\u0EA8\u0EA9\u0EAC\u0EBA\u0EBE\u0EBF\u0EC5\u0EC7\u0ECE\u0ECF\u0EDA\u0EDB\u0EE0-\u0EFF\u0F48\u0F6D-\u0F70\u0F98\u0FBD\u0FCD\u0FDB-\u0FFF\u10C6\u10C8-\u10CC\u10CE\u10CF\u1249\u124E\u124F\u1257\u1259\u125E\u125F\u1289\u128E\u128F\u12B1\u12B6\u12B7\u12BF\u12C1\u12C6\u12C7\u12D7\u1311\u1316\u1317\u135B\u135C\u137D-\u137F\u139A-\u139F\u13F5-\u13FF\u169D-\u169F\u16F1-\u16FF\u170D\u1715-\u171F\u1737-\u173F\u1754-\u175F\u176D\u1771\u1774-\u177F\u17DE\u17DF\u17EA-\u17EF\u17FA-\u17FF\u180F\u181A-\u181F\u1878-\u187F\u18AB-\u18AF\u18F6-\u18FF\u191D-\u191F\u192C-\u192F\u193C-\u193F\u1941-\u1943\u196E\u196F\u1975-\u197F\u19AC-\u19AF\u19CA-\u19CF\u19DB-\u19DD\u1A1C\u1A1D\u1A5F\u1A7D\u1A7E\u1A8A-\u1A8F\u1A9A-\u1A9F\u1AAE-\u1AFF\u1B4C-\u1B4F\u1B7D-\u1B7F\u1BF4-\u1BFB\u1C38-\u1C3A\u1C4A-\u1C4C\u1C80-\u1CBF\u1CC8-\u1CCF\u1CF7-\u1CFF\u1DE7-\u1DFB\u1F16\u1F17\u1F1E\u1F1F\u1F46\u1F47\u1F4E\u1F4F\u1F58\u1F5A\u1F5C\u1F5E\u1F7E\u1F7F\u1FB5\u1FC5\u1FD4\u1FD5\u1FDC\u1FF0\u1FF1\u1FF5\u1FFF\u200B-\u200F\u202A-\u202E\u2060-\u206F\u2072\u2073\u208F\u209D-\u209F\u20BB-\u20CF\u20F1-\u20FF\u218A-\u218F\u23F4-\u23FF\u2427-\u243F\u244B-\u245F\u2700\u2B4D-\u2B4F\u2B5A-\u2BFF\u2C2F\u2C5F\u2CF4-\u2CF8\u2D26\u2D28-\u2D2C\u2D2E\u2D2F\u2D68-\u2D6E\u2D71-\u2D7E\u2D97-\u2D9F\u2DA7\u2DAF\u2DB7\u2DBF\u2DC7\u2DCF\u2DD7\u2DDF\u2E3C-\u2E7F\u2E9A\u2EF4-\u2EFF\u2FD6-\u2FEF\u2FFC-\u2FFF\u3040\u3097\u3098\u3100-\u3104\u312E-\u3130\u318F\u31BB-\u31BF\u31E4-\u31EF\u321F\u32FF\u4DB6-\u4DBF\u9FCD-\u9FFF\uA48D-\uA48F\uA4C7-\uA4CF\uA62C-\uA63F\uA698-\uA69E\uA6F8-\uA6FF\uA78F\uA794-\uA79F\uA7AB-\uA7F7\uA82C-\uA82F\uA83A-\uA83F\uA878-\uA87F\uA8C5-\uA8CD\uA8DA-\uA8DF\uA8FC-\uA8FF\uA954-\uA95E\uA97D-\uA97F\uA9CE\uA9DA-\uA9DD\uA9E0-\uA9FF\uAA37-\uAA3F\uAA4E\uAA4F\uAA5A\uAA5B\uAA7C-\uAA7F\uAAC3-\uAADA\uAAF7-\uAB00\uAB07\uAB08\uAB0F\uAB10\uAB17-\uAB1F\uAB27\uAB2F-\uABBF\uABEE\uABEF\uABFA-\uABFF\uD7A4-\uD7AF\uD7C7-\uD7CA\uD7FC-\uF8FF\uFA6E\uFA6F\uFADA-\uFAFF\uFB07-\uFB12\uFB18-\uFB1C\uFB37\uFB3D\uFB3F\uFB42\uFB45\uFBC2-\uFBD2\uFD40-\uFD4F\uFD90\uFD91\uFDC8-\uFDEF\uFDFE\uFDFF\uFE1A-\uFE1F\uFE27-\uFE2F\uFE53\uFE67\uFE6C-\uFE6F\uFE75\uFEFD-\uFF00\uFFBF-\uFFC1\uFFC8\uFFC9\uFFD0\uFFD1\uFFD8\uFFD9\uFFDD-\uFFDF\uFFE7\uFFEF-\uFFFB\uFFFE\uFFFF]/g;
                            
                            const uidFullPath = await path.join(res, uniqueIdFile);
                            const uniqueIdData = (await readTextFile(uidFullPath)).replace(re, "");
                            const uniqueId = (JSON.parse(uniqueIdData)).id;

                            // Compute the MD5 hash of the .tja file
                            const tjaFullPath = await path.join(res, tjaPath);
                            const tjaContent = await readFile(tjaFullPath);
                            const chartMD5 = md5(tjaContent);

                            // Update the currentSongs object
                            currentSongs[uniqueId] = {
                                chartMD5,
                                chartRelativePath: relativePath
                            };
                        } catch (error) {
                            console.error(`Failed to read uniqueID.json in ${folderPath}:`, error);
                            // Skip this folder if uniqueID.json is not found or there's an error reading it
                        }
                    }
                }
            } catch (error) {
                console.error(`Error processing folder ${folderPath}:`, error);
            }
        }

        try {
            // Check if base directory exists
            if (await folderExists(baseDir)) {
                // Start the process with the base directory
                await processFolder(baseDir);
            } else {
                console.warn(`The directory "${baseDir}" does not exist.`);
            }

            scanning = false;
        } catch (error) {
            console.error(`Error scanning songs:`, error);
            scanning = false;
        }
    }

    $: GetFilteredSInfo = (SInfo) => {
        const bInNameFilter = SInfo.chartTitle.toLowerCase().includes(searchSong.toLowerCase()) || SInfo.chartSubtitle?.toLowerCase().includes(searchSong.toLowerCase());
        const bInGenreFilter = SInfo.tjaGenreFolder.toLowerCase().includes(searchGenre.toLowerCase());

        return bInGenreFilter && bInNameFilter;
    }

    const GetFilteredAvailableSInfo = (SInfo) => {
        const bNotUpToDate = !(Object.keys(currentSongs).includes(SInfo.uniqueId) && currentSongs[SInfo.uniqueId].chartMD5 === SInfo.tjaMD5);

        return bNotUpToDate && GetFilteredSInfo(SInfo);
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
            TriggerError("Cannot download songs while local folders are being scanned");
            return ;
        }

        const filteredSInfo = soundtrackInfo.filter((SInfo) => GetFilteredAvailableSInfo(SInfo));

        const songCount = filteredSInfo.length;

        if (songCount === 0) {
            TriggerSuccess("All songs are already up-to-date");
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
        songCountProgressBar = null
    }

    const DownloadSong = async (songObj, currentObj, songNb = undefined, songTotal = undefined) => {
        songDLProgress[songObj.uniqueId] = 0;
        //console.log(songDLProgress);

        const res = await GetRootPath();
        const baseDir = './OpenTaiko/Songs';
        const baseDirPath = await path.join(res, baseDir);
        const localPath = `${baseDir}/${(currentObj !== null) ? currentObj.chartRelativePath : songObj.tjaFolderPath}`.replace(/\\/g, '/');
        const tjaFullPath = await path.join(res, localPath);

        let fold_exists = await exists(tjaFullPath);
        if (!fold_exists)
            await mkdir(tjaFullPath, {recursive: true});

        const tmpFolder = await path.join(res, "./tmp");
        const uuid = crypto.randomUUID();
        const chartDownloadFolder = await path.join(tmpFolder, `${uuid}/`);

        fold_exists = await exists(chartDownloadFolder);
        if (!fold_exists)
            await mkdir(chartDownloadFolder, {recursive: true});

        let fileNames = [];

        let totbyts = 0;
        for (const filePath of songObj.tjaFilesPath) {
            const localFileName = filePath.split("\\").pop();
            const tjaFileUrl = `https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/${filePath}`;
            const dlPath = await path.join(chartDownloadFolder, localFileName.replace(/\\/g, '/'));

            
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

            fileNames.push(localFileName);
        };

        songDLProgress[songObj.uniqueId] = 0;
        await Promise.all(fileNames.map(async (fn, idx) => {
            const strPath = await path.join(chartDownloadFolder, fn.replace(/\\/g, '/'));
            const destPath = await path.join(tjaFullPath, fn.replace(/\\/g, '/'));

            await copyFile(strPath, destPath);
            songDLProgress[songObj.uniqueId] = (idx + 1) * (100 / fileNames.length);
            //console.log(songDLProgress);
        }));

        // Download box.def and default.png if missing
        const genrePaths = songObj.tjaFolderPath.split('\\').slice(0, -1).map((_, i, arr) => arr.slice(0, i + 1).join('/'));

        for (const genrePath of genrePaths) {
            const genreFullPath = await path.join(baseDirPath, genrePath);
            const boxdefPath = await path.join(genreFullPath, "./box.def");
            const preimgPath = await path.join(genreFullPath, "./default.png");

            // Always download the box.def file to be sure it remains up-to-date, not a good implementation but would get the job done for now
            const deffile_exists = false; // await exists(boxdefPath);
            if (!deffile_exists) {
                const localFileName = 'box.def';
                const _url = `https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/${genrePath}/box.def`;
                const dlPath = await path.join(chartDownloadFolder, localFileName);

                const resourceExists = await fetch(_url).then(res => res.ok).catch(() => false);

                if (resourceExists) {
                    await download(_url, dlPath);
                    await copyFile(dlPath, boxdefPath);
                }
            }

            const preimage_exists = await exists(preimgPath);
            if (!preimage_exists) {
                const localFileName = 'default.png';
                const _url = `https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/${genrePath}/default.png`;
                const dlPath = await path.join(chartDownloadFolder, localFileName);

                const resourceExists = await fetch(_url).then(res => res.ok).catch(() => false);

                if (resourceExists) {
                    await download(_url, dlPath);
                    await copyFile(dlPath, preimgPath);
                }
            }
        }

        // Clean after pooping
        await remove(chartDownloadFolder, { recursive: true });

        if (songNb === undefined)
            TriggerSuccess('Download complete');
        else
            TriggerSuccess(`Downloaded song ${songNb} out of ${songTotal}`);

        //crawlSongs();
        currentSongs[songObj.uniqueId] = {
            chartMD5: songObj.tjaMD5,
            chartRelativePath: songObj.tjaFolderPath
        };

        delete songDLProgress[songObj.uniqueId];
        //console.log(songDLProgress);
    }

    onMount(async () => {
        updateSoundtrackInfo();
        crawlSongs();
    });

</script>

<div class="table-container text-token">
	<table class="table table-hover">
		<thead>
			<tr>
				<th><button on:click={() => SortSongsByColumn("name")}>Song name</button></th>
				<th><button on:click={() => SortSongsByColumn("genre")}>Folder</button></th>
				<th colspan="5" class="w-1/5">Difficulties</th>
				<th><button on:click={() => SortSongsByColumn("size")}>Size</button></th>
				<th class="w-1/6">Status</th>
			</tr>
			<tr>
				<th><input class="w-full rounded-md px-3 py-2 text-blue-950" placeholder="Song search..." bind:value={searchSong}></th>
				<th><input class="w-full rounded-md px-3 py-2 text-blue-950" placeholder="Folder search..." bind:value={searchGenre}></th>
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
					<button type="button" on:click={DownloadDisplayedSongs} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700">Bulk download</button>
					{/if}
				</th>
			</tr>
		</thead>
		<tbody>
			{#each soundtrackInfo as songInfo}
			{#if GetFilteredSInfo(songInfo)}
			<tr>
				<td>
					<AudioPlayer {songInfo} />
					<!-- <button on:click={() => navigator.clipboard.writeText(songInfo.uniqueId)}>Copy uniqueID</button> -->
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
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Easy"/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Normal"/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Hard"/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Oni"/>
				</td>
				<td>
					<SongDifficultyChip SongInfo={songInfo} Difficulty="Edit"/>
				</td>
				{/if}
				<td>{songInfo.chartSize}Mb</td>
				<!-- songDLProgress[songObj.uniqueId] -->
				{#if scanning === true}
				<td>
					<p>Scanning...</p>
				</td>
				{:else if !Object.keys(currentSongs).includes(songInfo.uniqueId)}
				<td>
					<p>Not downloaded</p>
					<br />
					{#if songDLProgress[songInfo.uniqueId] === undefined}
					<button type="button" on:click={DownloadSong(songInfo, null)} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700">Download</button>
					{:else}
					<ProgressBar bind:value={songDLProgress[songInfo.uniqueId]} max={100} />
					{/if}
				</td>
				{:else if currentSongs[songInfo.uniqueId].chartMD5 === songInfo.tjaMD5}
				<td>
					<p>Up-to-date</p>
                    <br />
                    {#if songDLProgress[songInfo.uniqueId] === undefined}
					<button type="button" on:click={DownloadSong(songInfo, currentSongs[songInfo.uniqueId])} class="text-white bg-gray-700 hover:bg-gray-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-gray-600 dark:hover:bg-gray-700">Redownload</button>
					{:else}
					<ProgressBar bind:value={songDLProgress[songInfo.uniqueId]} max={100} />
					{/if}
				</td>
				{:else}
				<td>
					<p>Outdated</p>
					<br />
					{#if songDLProgress[songInfo.uniqueId] === undefined}
					<button type="button" on:click={DownloadSong(songInfo, currentSongs[songInfo.uniqueId])} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700">Update</button>
					{:else}
					<ProgressBar bind:value={songDLProgress[songInfo.uniqueId]} max={100} />
					{/if}
				</td>
				{/if}
			</tr>
			{/if}
			{/each}
		</tbody>
	</table>
</div>

<style>


</style>