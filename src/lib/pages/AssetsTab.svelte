<script lang="ts">
    // Dependencies
    import { onMount } from 'svelte';
    import { Tabs } from '@skeletonlabs/skeleton-svelte';
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { mkdir, readDir, exists, copyFile, remove } from '@tauri-apps/plugin-fs';
    import { fetch } from "@tauri-apps/plugin-http";
    import { path } from '@tauri-apps/api';
    import { invoke } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';
    import type { AssetCatalog, AssetInfo, AssetPrefix as AssetKeyPrefix, AssetType, AssetVersion, CharaInfo, LocalAsset, ToastContext } from '$lib/types';

    const copyAllFilesRecursive = async (src: string, dst: string): Promise<void> => {
        let files;
        try {
            files = await readDir(src);
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
    const { TriggerError, TriggerSuccess, backoffDownload } = getContext<ToastContext>('toast');

    import { GetTmpPath } from "$lib/utils/path";
    import { activeInstance, instanceVersions } from "$lib/stores/instances";
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';


    import AssetStatusCell from '$lib/components/AssetStatusCell.svelte';
    import VersionNumberChip from '$lib/components/VersionNumberChip.svelte';

    // Asset folders relative to an instance
    const assetBaseDirs: Record<AssetType, string> = {
        "Skins": "System",
        "Characters": "Global/Characters",
        "Puchicharas": "Global/PuchiChara"
    };

    const ASSET_TYPES: AssetType[] = ["Skins", "Characters", "Puchicharas"];

    type LocalAssets = Record<AssetType, Record<string, LocalAsset>>;
    type ProgressByType = Record<AssetType, Record<string, number>>;

    const emptyCatalog = (): AssetCatalog => ({ "Skins": [], "Characters": [], "Puchicharas": [] });
    const emptyByType = <T,>(): Record<AssetType, Record<string, T>> => ({ "Skins": {}, "Characters": {}, "Puchicharas": {} });

    // When embedded in the Home sub-tabs the asset type is controlled by the parent
    interface Props {
        /** false when embedded in the Home sub-tabs: the internal tab bar is hidden. */
        ShowTabs?: boolean;
        /** 0 Skins, 1 Characters, 2 Puchicharas. */
        currentAsset?: number;
    }

    let { ShowTabs = true, currentAsset = $bindable(0) }: Props = $props();

    // Rescan whenever another instance becomes active
    let scannedInstanceId: string | null = null;   // bookkeeping only, never rendered

    const assetsInfoUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Skins/main/assets_info.json';
    let assetsInfo = $state<AssetCatalog>(emptyCatalog());
    let currentAssets = $state<LocalAssets>(emptyByType<LocalAsset>());
    let assetScanning = $state(false);
    let assetDLProgress = $state<ProgressByType>(emptyByType<number>());
    let assetCountProgress: Record<AssetType, number> = {
        "Skins":0,
        "Characters":0,
        "Puchicharas":0
    };
    let assetCountProgressBar = $state<Record<AssetType, number | null>>({
        "Skins":null,
        "Characters":null,
        "Puchicharas":null
    });


    const updateAssetsInfo = async () => {
        try {
            const response = await fetch(assetsInfoUrl);
            if (response.ok) {
                const text = await response.text();
                assetsInfo = JSON.parse(text) as AssetCatalog;
            } else {
                assetsInfo = emptyCatalog();
            }
        } catch (error) {
            assetsInfo = emptyCatalog();
        }
    }

    const crawlAssets = async () => {
        const instance = get(activeInstance);
        if (!instance) return;
        assetScanning = true;
        const scanned: LocalAssets = {
            "Skins": await crawlAsset(instance.path, "Skins"),
            "Characters": await crawlAsset(instance.path, "Characters"),
            "Puchicharas": await crawlAsset(instance.path, "Puchicharas")
        };
        // Reassign so the status cells react (Svelte 4 needs a fresh reference)
        if (get(activeInstance)?.id === instance.id) currentAssets = scanned;
        assetScanning = false;
    }

    const crawlAsset = async (instancePath: string, assetType: AssetType): Promise<Record<string, LocalAsset>> => {
        const targetFile: Record<AssetType, string> = {
            "Skins": "SkinConfig.ini",
            "Characters": "CharaConfig.txt",
            "Puchicharas": "PuchiConfig.txt"
        };

        // Native single-call scan: walking a full build's asset tree (thousands of
        // files) from JS with per-file IPC would flood the event loop and freeze the UI.
        const scanned: Record<string, LocalAsset> = {};
        try {
            const baseDir = await path.join(instancePath, assetBaseDirs[assetType]);
            const found = await invoke<AssetVersion[]>('scan_asset_versions', { baseDir, targetFile: targetFile[assetType] });
            for (const { relPath, version } of found) {
                scanned[relPath] = { assetFolderName: relPath, assetVersion: version };
            }
        } catch (error) {
            console.error(`Error scanning ${assetType}:`, error);
        }
        return scanned;
    }

    const AssetPrefix = (assetType: AssetType): AssetKeyPrefix => (assetType === "Skins") ? "skin" : "chara";
    // Total function: an out-of-range index falls back to Skins instead of returning
    // undefined — `assetsInfo[undefined]` would make `{#each}` throw during render,
    // which breaks Svelte's update cycle and softlocks the whole app.
    const AssetTabType = (currentAsset: number): AssetType => ASSET_TYPES[currentAsset] ?? "Skins";
    const AssetTabPrefix = (currentAsset: number): AssetKeyPrefix => AssetPrefix(AssetTabType(currentAsset));

    // Catalog entries prefix their keys with the asset kind ("skinName" / "charaName")
    const Field = (info: AssetInfo, prefix: AssetKeyPrefix, name: string): string =>
        String((info as Record<string, unknown>)[`${prefix}${name}`] ?? '');
    const SizeMb = (info: AssetInfo, prefix: AssetKeyPrefix): number =>
        Number((info as Record<string, unknown>)[`${prefix}Size`] ?? 0);

    const DownloadDisplayedAssets = async (assetType: AssetType) => {
        if (assetScanning === true) {
            const translatedType = get(_)(`assets.type.${assetType.toLowerCase()}`);
            TriggerError(get(_)('assets.error.scanning', { values: { type: translatedType } }));
            return ;
        }

        const assetPrefix = AssetPrefix(assetType);

        const _filter = (a: AssetInfo): boolean => {
            const local = currentAssets[assetType][Field(a, assetPrefix, 'Folder')];
            const _notdownloaded = local === undefined;
            const _outdated = _notdownloaded || local.assetVersion !== Field(a, assetPrefix, 'Version');
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
            const assetRelpath = Field(Aif, assetPrefix, 'Folder');

            assetCountProgressBar[assetType] = 100 * (assetCountProgress[assetType] / assetCount);

            console.log(`Downloading ${assetType} ${assetCountProgress[assetType] + 1} out of ${assetCount}...`);
            console.log(Aif);

            const curObj = currentAssets[assetType][assetRelpath] ?? null;

            await DownloadAsset(Aif, curObj, assetType, assetCountProgress[assetType] + 1, assetCount);
            assetCountProgress[assetType]++;
        }
        assetCountProgressBar[assetType] = null
    }

    const DownloadAsset = async (assetObj: AssetInfo, currentObj: LocalAsset | null, assetType: AssetType, assetNb?: number, assetTotal?: number) => {
        const instance = get(activeInstance);
        if (!instance) return;
        if (instance.channel === 'experimental') {
            TriggerError(get(_)('assets.experimental.blocked_text'));
            return;
        }
        const res = instance.path;

        const baseDir = assetBaseDirs[assetType];
        const assetPrefix = AssetPrefix(assetType);
        const assetRelpath = Field(assetObj, assetPrefix, 'Folder');
        const assetSize = SizeMb(assetObj, assetPrefix);
        const assetVersion = Field(assetObj, assetPrefix, 'Version');

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
            const zipName = assetRelpath.replace(/[()]/g, '').replace(/ /g, '.') + '.zip';
            const zipUrl = `https://github.com/OpenTaiko/OpenTaiko-Skins/releases/download/system-assets/${zipName}`;
            const zipPath = await path.join(assetDownloadFolder, 'skin.zip');

            let totbyts = 0;
            const success = await backoffDownload(
                zipUrl,
                zipPath,
                (pr) => {
                    totbyts += pr.progress;
                    assetDLProgress[assetType][assetRelpath] = 100 * (totbyts / (assetSize * 1024 * 1024));
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

            let extracting = true;
            const capturedRelpath = assetRelpath;
            const capturedType = assetType;
            const unlisten = await listen<number>('extract-progress', (event) => {
                if (!extracting) return;
                assetDLProgress[capturedType][capturedRelpath] = event.payload;
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
            const assetFpath = (assetObj as CharaInfo).charaFilesPath;
            const baseDirPath = await path.join(res, baseDir);

            let fileNames: string[] = [];
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
            currentAssets = emptyByType<LocalAsset>();
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
			{#each assetsInfo[AssetTabType(currentAsset)] ?? [] as info (Field(info, AssetTabPrefix(currentAsset), 'Folder'))}
			<tr>
				<td>{Field(info, AssetTabPrefix(currentAsset), 'Name')}</td>
				<td>
					<VersionNumberChip LatestVersion={Field(info, AssetTabPrefix(currentAsset), 'Version')} CurrentVersion={optk_version} Strictness="Error" />
				</td>
				<td>{Field(info, AssetTabPrefix(currentAsset), 'Resolution')}</td>
				<td>{Field(info, AssetTabPrefix(currentAsset), 'Creator')}</td>
				<td>{SizeMb(info, AssetTabPrefix(currentAsset))}Mb</td>
				<td>
					<AssetStatusCell
						IsScanning={assetScanning}
						AssetType={AssetTabType(currentAsset)}
						AssetInfo={info}
						CurrentAssets={currentAssets}
						DownloadMethod={DownloadAsset}
						Progress={assetDLProgress[AssetTabType(currentAsset)][Field(info, AssetTabPrefix(currentAsset), 'Folder')]}
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