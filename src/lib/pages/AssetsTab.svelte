<script>
    // Dependencies
    import { onMount } from 'svelte';
    import { Tabs } from '@skeletonlabs/skeleton-svelte';
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { mkdir, readDir, exists, copyFile, remove } from '@tauri-apps/plugin-fs';
    import { fetch } from "@tauri-apps/plugin-http";
    import { path } from '@tauri-apps/api';
    import { invoke } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';

    const copyAllFilesRecursive = async (src, dst) => {
        let files;
        try {
            files = await readDir(src, { recursive: false });
        } catch (error) {
            console.error(`Error reading directory ${src}:`, error);
            return;
        }
for (const file of files) {
            const srcPath = `${src}/${file.name}`;
            const dstPath = `${dst}/${file.name}`;
            try {
                if (file.isDirectory) {
                    await mkdir(dstPath, { recursive: true });
                    await copyAllFilesRecursive(srcPath, dstPath);
                } else {
                    await copyFile(srcPath, dstPath);
                }
            } catch (error) {
                console.error(`Error copying ${srcPath} → ${dstPath}:`, error);
            }
        }
    }
    import { getContext } from 'svelte';
    const { TriggerError, TriggerWarning, TriggerSuccess, backoffDownload } = getContext('toast');

    import { GetTmpPath } from "$lib/utils/path.js";
    import { activeInstance, instanceVersions } from "$lib/stores/instances.js";
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';


    import AssetStatusCell from '$lib/components/AssetStatusCell.svelte';
    import VersionNumberChip from '$lib/components/VersionNumberChip.svelte';

    // Asset folders relative to an instance
    const assetBaseDirs = {
        "Skins": "System",
        "Characters": "Global/Characters",
        "Puchicharas": "Global/PuchiChara"
    };


    // When embedded in the Home sub-tabs the asset type is controlled by the parent
    
    /**
     * @typedef {Object} Props
     * @property {boolean} [ShowTabs] - and the internal tab bar is hidden
     * @property {number} [currentAsset]
     */

    /** @type {Props} */
    let { ShowTabs = true, currentAsset = $bindable(0) } = $props();

    // Rescan whenever another instance becomes active
    let scannedInstanceId = null;   // bookkeeping only, never rendered

    const assetsInfoUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Skins/main/assets_info.json';
    let assetsInfo = $state({
        "Skins":[],
        "Characters":[],
        "Puchicharas":[]
    });
    let currentAssets = $state({
        "Skins":{},
        "Characters":{},
        "Puchicharas":{}
    });
    let assetScanning = $state(false);
    let assetDLProgress = $state({
        "Skins":{},
        "Characters":{},
        "Puchicharas":{}
    });
    let assetCountProgress = {
        "Skins":0,
        "Characters":0,
        "Puchicharas":0
    };
    let assetCountProgressBar = $state({
        "Skins":null,
        "Characters":null,
        "Puchicharas":null
    });


    const updateAssetsInfo = async () => {
        try {
            const response = await fetch(assetsInfoUrl);
        if (response.ok) {
            const text = await response.text();
            assetsInfo = JSON.parse(text);
        } else {
            assetsInfo = {
                "Skins":[],
                "Characters":[],
                "Puchicharas":[]
            };
        }
        } catch (error) {
            assetsInfo = {
                "Skins":[],
                "Characters":[],
                "Puchicharas":[]
            };
        }
    }

    const crawlAssets = async () => {
        const instance = get(activeInstance);
        if (!instance) return;
        assetScanning = true;
        const scanned = {
            "Skins": await crawlAsset(instance, "Skins"),
            "Characters": await crawlAsset(instance, "Characters"),
            "Puchicharas": await crawlAsset(instance, "Puchicharas")
        };
        // Reassign so the status cells react (Svelte 4 needs a fresh reference)
        if (get(activeInstance)?.id === instance.id) currentAssets = scanned;
        assetScanning = false;
    }

    const crawlAsset = async (instance, assetType) => {
        const targetFile = {
            "Skins": "SkinConfig.ini",
            "Characters": "CharaConfig.txt",
            "Puchicharas": "PuchiConfig.txt"
        }[assetType];

        // Native single-call scan: walking a full build's asset tree (thousands of
        // files) from JS with per-file IPC would flood the event loop and freeze the UI.
        const scanned = {};
        try {
            const baseDir = await path.join(instance.path, assetBaseDirs[assetType]);
            const found = await invoke('scan_asset_versions', { baseDir, targetFile });
            for (const { relPath, version } of found) {
                scanned[relPath] = { assetFolderName: relPath, assetVersion: version };
            }
        } catch (error) {
            console.error(`Error scanning ${assetType}:`, error);
        }
        return scanned;
    }

    const AssetPrefix = (assetType) => (assetType === "Skins") ? "skin" : "chara";
    // Total function: an out-of-range index falls back to Skins instead of returning
    // undefined — `assetsInfo[undefined]` would make `{#each}` throw during render,
    // which breaks Svelte's update cycle and softlocks the whole app.
    const AssetTabType = (currentAsset) => ["Skins", "Characters", "Puchicharas"][currentAsset] ?? "Skins";
    const AssetTabPrefix = (currentAsset) => AssetPrefix(AssetTabType(currentAsset));

    const DownloadDisplayedAssets = async (assetType) => {
        if (assetScanning === true) {
            const translatedType = get(_)(`assets.type.${assetType.toLowerCase()}`);
            TriggerError(get(_)('assets.error.scanning', { values: { type: translatedType } }));
            return ;
        }

        const assetPrefix = AssetPrefix(assetType);

        const _filter = (a) => {
            const _notdownloaded = !Object.keys(currentAssets[assetType]).includes(a[`${assetPrefix}Folder`]);
            const _outdated = _notdownloaded || currentAssets[assetType][a[`${assetPrefix}Folder`]].assetVersion !== a[`${assetPrefix}Version`];
            return _notdownloaded || _outdated;
        };

        const AInfo = assetsInfo[assetType].filter((a) => _filter(a));

        const assetCount = AInfo.length;

        if (assetCount === 0) {
            const translatedType = get(_)(`assets.type.${assetType.toLowerCase()}`);
            TriggerSuccess(get(_)('assets.success.all_up_to_date', { values: { type: translatedType } }));
            return ;
        }

        assetCountProgress[assetType] = 0;
        for (const Aif of AInfo) {
            const assetRelpath = Aif[`${AssetPrefix(assetType)}Folder`];

            assetCountProgressBar[assetType] = 100 * (assetCountProgress[assetType] / assetCount);

            console.log(`Downloading ${assetType} ${assetCountProgress[assetType] + 1} out of ${assetCount}...`);
            console.log(Aif);

            let curObj = null;
            if (Object.keys(currentAssets[assetType]).includes(assetRelpath)) curObj = currentAssets[assetType][assetRelpath];

            await DownloadAsset(Aif, curObj, assetType, assetCountProgress[assetType] + 1, assetCount);
            assetCountProgress[assetType]++;
        }
        assetCountProgressBar[assetType] = null
    }

    const DownloadAsset = async (assetObj, currentObj, assetType, assetNb = undefined, assetTotal = undefined) => {
        const instance = get(activeInstance);
        if (!instance) return;
        if (instance.channel === 'experimental') {
            TriggerError(get(_)('assets.experimental.blocked_text'));
            return;
        }
        const res = instance.path;

        const baseDir = assetBaseDirs[assetType];
        const assetPrefix = AssetPrefix(assetType);
        const assetRelpath = assetObj[`${assetPrefix}Folder`];
        const assetSize = assetObj[`${assetPrefix}Size`];
        const assetVersion = assetObj[`${assetPrefix}Version`];

        assetDLProgress[assetType][assetRelpath] = 0;

        const localPath = `${baseDir}/${assetRelpath}`;
        const assetFullPath = await path.join(res, localPath);

        if (!await exists(assetFullPath))
            await mkdir(assetFullPath, { recursive: true });

        const assetDownloadFolder = await GetTmpPath(crypto.randomUUID());

        if (!await exists(assetDownloadFolder))
            await mkdir(assetDownloadFolder, { recursive: true });

        if (assetType === "Skins") {
            // Zip name: spaces → dots, parentheses stripped (matches release asset naming)
            const zipName = assetObj.skinFolder.replace(/[()]/g, '').replace(/ /g, '.') + '.zip';
            const zipUrl = `https://github.com/OpenTaiko/OpenTaiko-Skins/releases/download/system-assets/${zipName}`;
            const zipPath = await path.join(assetDownloadFolder, 'skin.zip');

            let totbyts = 0;
            const success = await backoffDownload(
                zipUrl,
                zipPath,
                (pr) => {
                    totbyts += pr.progress;
                    assetDLProgress[assetType][assetRelpath] = 100 * (totbyts / (assetSize * 1024 * 1024));
                    assetDLProgress = assetDLProgress;
                }
            );

            if (!success) {
                // backoffDownload already shows an error toast; show zip-specific guidance on top
                TriggerError(get(_)('assets.error.zip_not_found'));
                await remove(assetDownloadFolder, { recursive: true });
                delete assetDLProgress[assetType][assetRelpath];
                return;
            }

            assetDLProgress[assetType][assetRelpath] = 0;
            assetDLProgress = assetDLProgress;

            let extracting = true;
            const capturedRelpath = assetRelpath;
            const capturedType = assetType;
            const unlisten = await listen('extract-progress', (event) => {
                if (!extracting) return;
                assetDLProgress[capturedType][capturedRelpath] = event.payload;
                assetDLProgress = assetDLProgress;
            });

            await invoke('unzip_and_get_first_folder', { zipPath });
            extracting = false;
            unlisten();

            // All skin zips are flat (files/folders at zip root), so copy directly
            // from the extraction folder into the skin destination.
            await remove(zipPath);
            await copyAllFilesRecursive(assetDownloadFolder.replace(/\\/g, '/'), assetFullPath);

            await remove(assetDownloadFolder, { recursive: true });

        } else {
            // Per-file download for Characters and Puchicharas
            const subDir = "Global";
            const assetFpath = assetObj.charaFilesPath;
            const baseDirPath = await path.join(res, baseDir);

            let fileNames = [];
            let totbyts = 0;

            for (const filePath of assetFpath) {
                const localFileName = filePath.replace(/^[^\\]+\\/, '');
                const assetFileUrl = `https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Skins/main/${subDir}/${filePath}`;
                const dlPath = await path.join(assetDownloadFolder, localFileName.replace(/\\/g, '/'));
                const fileFold = await path.join(assetDownloadFolder, (filePath.split('\\').length > 2 ? filePath.replace(/^[^\\]+\\/, '').replace(/\\[^\\]+$/, '') : '').replace(/\\/g, '/'));

                if (!await exists(fileFold))
                    await mkdir(fileFold, { recursive: true });

                const success = await backoffDownload(
                    assetFileUrl,
                    dlPath,
                    (pr) => {
                        totbyts += pr.progress;
                        assetDLProgress[assetType][assetRelpath] = 100 * (totbyts / (assetSize * 1024 * 1024));
                        assetDLProgress = assetDLProgress;
                    }
                );

                if (!success) {
                    await remove(assetDownloadFolder, { recursive: true });
                    delete assetDLProgress[assetType][assetRelpath];
                    return;
                }

                fileNames.push(localFileName);
            }

            assetDLProgress[assetType][assetRelpath] = 0;
            await Promise.all(fileNames.map(async (fn, idx) => {
                const strPath = (await path.join(assetDownloadFolder, fn)).replace(/\\/g, '/');
                const destPath = (await path.join(baseDirPath, fn)).replace(/\\/g, '/');
                const fileFold = await path.dirname(destPath);

                if (!await exists(fileFold))
                    await mkdir(fileFold, { recursive: true });

                await copyFile(strPath, destPath);
                assetDLProgress[assetType][assetRelpath] = (idx + 1) * (100 / fileNames.length);
            }));

            await remove(assetDownloadFolder, { recursive: true });
        }

        if (assetNb === undefined)
            TriggerSuccess(get(_)('assets.success.download_complete'));
        else {
            const translatedType = get(_)(`assets.type.${assetType.toLowerCase()}`);
            TriggerSuccess(get(_)('assets.success.download_nb', { values: { type: translatedType, nb: assetNb, total: assetTotal } }));
        }

        currentAssets[assetType][assetRelpath] = {
            assetFolderName: assetRelpath,
            assetVersion: assetVersion
        };

        delete assetDLProgress[assetType][assetRelpath];
        assetDLProgress = assetDLProgress;
    }

    onMount(async () => {
        updateAssetsInfo();
        // Local asset scanning is triggered reactively when the active instance is known
    });

    let optk_version = $derived(($activeInstance && $instanceVersions[$activeInstance.id]) || "0.0.0.0");
    let isExperimental = $derived($activeInstance?.channel === 'experimental');
    $effect(() => {
        if ($activeInstance && !isExperimental && $activeInstance.id !== scannedInstanceId) {
            scannedInstanceId = $activeInstance.id;
            currentAssets = { "Skins": {}, "Characters": {}, "Puchicharas": {} };
            crawlAssets();
        }
    });
</script>

{#if isExperimental}
<section class="card w-full">
	<div class="p-6 space-y-3 text-center">
		<p class="text-2xl"><i class="fa-solid fa-flask"></i></p>
		<p><b>{$_('assets.experimental.blocked_title')}</b></p>
		<p class="opacity-70">{$_('assets.experimental.blocked_text')}</p>
	</div>
</section>
{:else}
{#if ShowTabs}
<Tabs value={String(currentAsset)} onValueChange={(details) => currentAsset = Number(details.value)} class="tab-bar w-full">
	<Tabs.List class="justify-center">
		<Tabs.Trigger value="0">
			<i class="fa-solid fa-palette"></i>
			<span>{$_('assets.tab.skins')}</span>
		</Tabs.Trigger>
		<Tabs.Trigger value="1">
			<i class="fa-solid fa-user"></i>
			<span>{$_('assets.tab.characters')}</span>
		</Tabs.Trigger>
		<Tabs.Trigger value="2">
			<i class="fa-solid fa-circle-half-stroke"></i>
			<span>{$_('assets.tab.puchicharas')}</span>
		</Tabs.Trigger>
	</Tabs.List>
</Tabs>
{/if}
<div class="table-wrap">
	<table class="table table-hover">
		<thead>
			<tr>
				<th>{$_('assets.col.asset')}</th>
				<th>{$_('assets.col.version')}</th>
				<th>{$_('assets.col.resolution')}</th>
				<th>{$_('assets.col.author')}</th>
				<th>{$_('assets.col.size')}</th>
				<th class="w-1/6">{$_('assets.col.status')}</th>
			</tr>
			<tr>
				<th></th>
				<th></th>
				<th></th>
				<th></th>
				<th></th>
				<th>
					{#if assetCountProgressBar[AssetTabType(currentAsset)] !== null}
					<ProgressBar value={assetCountProgressBar[AssetTabType(currentAsset)]} max={100} />
					{:else}
					<button type="button" onclick={() => DownloadDisplayedAssets(AssetTabType(currentAsset))} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('assets.button.bulk_download')}</button>
					{/if}
				</th>
			</tr>
		</thead>
		<tbody>
			{#each assetsInfo[AssetTabType(currentAsset)] ?? [] as info}
			<tr>
				<td>{info[`${AssetTabPrefix(currentAsset)}Name`]}</td>
				<td>
					<VersionNumberChip LatestVersion={info[`${AssetTabPrefix(currentAsset)}Version`]} CurrentVersion={optk_version} Strictness="Error" />
				</td>
				<td>{info[`${AssetTabPrefix(currentAsset)}Resolution`]}</td>
				<td>{info[`${AssetTabPrefix(currentAsset)}Creator`]}</td>
				<td>{info[`${AssetTabPrefix(currentAsset)}Size`]}Mb</td>
				<td>
					<AssetStatusCell
						IsScanning={assetScanning}
						AssetType={AssetTabType(currentAsset)}
						AssetInfo={info}
						CurrentAssets={currentAssets}
						DownloadMethod={DownloadAsset}
						Progress={assetDLProgress[AssetTabType(currentAsset)][info[`${AssetTabPrefix(currentAsset)}Folder`]]}
						/>
				</td>
			</tr>
			{/each}
		</tbody>
	</table>
</div>
{/if}

<style>

</style>