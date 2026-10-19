<script lang="ts">
    import { ipc } from '$lib/ipc';
    import { formatSize } from '$lib/utils/format';
    import { ValidAssetCatalog } from '$lib/utils/validate';
    // Dependencies
    import { onMount } from 'svelte';
    import { Tabs } from '@skeletonlabs/skeleton-svelte';
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { mkdir, remove } from '@tauri-apps/plugin-fs';
    import { download, installFiles, fetchCachedJson, type InstallEntry } from '$lib/utils/download';
    import { startJob, isCancelled, InstanceInUse, type Job } from '$lib/stores/activity';
    import { DescribeError, ErrorCause, ErrorText } from '$lib/utils/errors';
    import { path } from '@tauri-apps/api';
    import { Channel } from '@tauri-apps/api/core';
    import type { AssetCatalog, AssetInfo, AssetPrefix as AssetKeyPrefix, AssetType, AssetVersion, CharaInfo, Instance, LocalAsset, ToastContext } from '$lib/types';

    import { getContext } from 'svelte';
    const { TriggerError, TriggerWarning, TriggerSuccess } = getContext<ToastContext>('toast');

    import { GetTmpPath, IsPlainRelPath } from "$lib/utils/path";
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
    // Running bulk downloads, per asset type, so each can be cancelled
    const bulkJobs: Partial<Record<AssetType, Job>> = {};
    // Read by the Cancel button, which says it is cancelling until the run stops
    const bulkCancelRequested = $state<Partial<Record<AssetType, boolean>>>({});
    // Set when the catalog could not be loaded: an empty table would otherwise read as
    // "everything is up to date"
    let catalogFetchFailed = $state(false);


    const updateAssetsInfo = async () => {
        catalogFetchFailed = false;
        try {
            assetsInfo = ValidAssetCatalog(await fetchCachedJson<unknown>(assetsInfoUrl));
        } catch (error) {
            console.error('Failed to fetch the asset catalog:', error);
            assetsInfo = emptyCatalog();
            catalogFetchFailed = true;
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
            const found = await ipc.scanAssetVersions(baseDir, targetFile[assetType]);
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
            TriggerWarning(get(_)('assets.error.scanning', { values: { type: translatedType } }));
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

        const instance = get(activeInstance);
        if (!instance || !CanWriteInto(instance)) return;
        const job = startJob('home', instance.id);
        bulkJobs[assetType] = job;
        bulkCancelRequested[assetType] = false;
        // One summary at the end instead of a toast per asset
        let done = 0;
        let failed = 0;
        assetCountProgress[assetType] = 0;
        assetCountProgressBar[assetType] = 0;
        try {
            for (const Aif of AInfo) {
                if (bulkCancelRequested[assetType]) break;
                const assetRelpath = Field(Aif, assetPrefix, 'Folder');
                const curObj = currentAssets[assetType][assetRelpath] ?? null;

                const outcome = await DownloadAsset(Aif, curObj, assetType, assetCountProgress[assetType] + 1, assetCount, job);
                if (outcome === 'done') done++;
                else if (outcome === 'failed') failed++;
                assetCountProgress[assetType]++;
                assetCountProgressBar[assetType] = 100 * (assetCountProgress[assetType] / assetCount);
            }
            const values = { count: done, total: assetCount, failed };
            if (bulkCancelRequested[assetType]) TriggerWarning(get(_)('bulk.cancelled', { values }));
            else if (failed > 0) TriggerError(get(_)('bulk.failed', { values }));
            else TriggerSuccess(get(_)('bulk.done', { values }));
        } finally {
            job.end();
            delete bulkJobs[assetType];
            assetCountProgressBar[assetType] = null;
        }
    }

    const CancelBulk = (assetType: AssetType) => {
        bulkCancelRequested[assetType] = true;
        bulkJobs[assetType]?.cancel();
    };

    type DownloadOutcome = 'done' | 'failed' | 'cancelled' | 'skipped';

    // A bulk run passes its job and reports a summary itself, so the asset stays quiet.
    // A single asset gets a job of its own, so it shows as running and can be noticed
    // before the window is closed on it.
    // Said once, before a run starts: a bulk run used to say it again for every asset
    const CanWriteInto = (instance: Instance): boolean => {
        if (instance.channel === 'experimental') {
            TriggerWarning(get(_)('assets.experimental.blocked_text'));
            return false;
        }
        // An install replaces the game's files, these included
        if (InstanceInUse(instance.id, false)) {
            TriggerWarning(get(_)('assets.error.instance_busy'));
            return false;
        }
        return true;
    };

    const DownloadAsset = async (assetObj: AssetInfo, currentObj: LocalAsset | null, assetType: AssetType, assetNb?: number, assetTotal?: number, bulkJob?: Job): Promise<DownloadOutcome> => {
        const instance = get(activeInstance);
        if (!instance) return 'skipped';
        if (!bulkJob && !CanWriteInto(instance)) return 'skipped';
        const quiet = bulkJob !== undefined;
        const job = bulkJob ?? startJob('home', instance.id);
        try {
            return await InstallAsset(instance.path, assetObj, assetType, quiet, job);
        } finally {
            if (!bulkJob) job.end();
        }
    }

    const InstallAsset = async (res: string, assetObj: AssetInfo, assetType: AssetType, quiet: boolean, job: Job): Promise<DownloadOutcome> => {
        const baseDir = assetBaseDirs[assetType];
        const assetPrefix = AssetPrefix(assetType);
        const assetRelpath = Field(assetObj, assetPrefix, 'Folder');
        const assetSize = SizeMb(assetObj, assetPrefix);
        const assetVersion = Field(assetObj, assetPrefix, 'Version');

        // Catalog paths decide where files are written, so a malformed one is refused
        // before anything is downloaded
        const catalogPaths = assetType === 'Skins' ? [assetRelpath] : [assetRelpath, ...(assetObj as CharaInfo).charaFilesPath];
        if (!catalogPaths.every(IsPlainRelPath)) {
            console.error(`Refusing catalog paths of ${assetRelpath}:`, catalogPaths);
            if (!quiet) TriggerError(get(_)('dl.error.failed_abort'));
            return 'failed';
        }

        assetDLProgress[assetType][assetRelpath] = 0;

        const localPath = `${baseDir}/${assetRelpath}`;
        const assetFullPath = await path.join(res, localPath);

        // Any failure below clears the progress row, says so, and records nothing: a
        // half-installed asset must not show as up to date
        let assetDownloadFolder: string | null = null;
        try {
            await mkdir(assetFullPath, { recursive: true });

            if (assetType === "Skins") {
                assetDownloadFolder = await GetTmpPath(crypto.randomUUID());
                await mkdir(assetDownloadFolder, { recursive: true });

                // Zip name: spaces → dots, parentheses stripped (matches release asset naming)
                const zipName = assetRelpath.replace(/[()]/g, '').replace(/ /g, '.') + '.zip';
                const zipUrl = `https://github.com/OpenTaiko/OpenTaiko-Skins/releases/download/system-assets/${zipName}`;
                const zipPath = await path.join(assetDownloadFolder, 'skin.zip');

                let totbyts = 0;
                await download(
                    zipUrl,
                    zipPath,
                    (pr) => {
                        totbyts += pr.progress;
                        assetDLProgress[assetType][assetRelpath] = 100 * (totbyts / (assetSize * 1024 * 1024));
                    },
                    undefined,
                    job.id
                );

                assetDLProgress[assetType][assetRelpath] = 0;

                const extractProgress = new Channel<number>();
                extractProgress.onmessage = (percent) => assetDLProgress[assetType][assetRelpath] = percent;
                await ipc.unzip(zipPath, extractProgress, job.id);

                // All skin zips are flat (files/folders at zip root), so the extraction folder's
                // content is moved straight into the skin folder. The move is done natively:
                // copying a skin's thousands of files one call at a time from here was slow,
                // and it skipped any file it failed to copy without saying so. The move also
                // removes the extraction folder.
                await remove(zipPath);
                await ipc.mergeMoveDir(assetDownloadFolder, assetFullPath);
            } else {
                // Characters and Puchicharas are separate files, downloaded straight into
                // place a few at a time. The first folder of each catalog path is the
                // repository's own (Characters\ / Puchicharas\) and is dropped.
                const subDir = "Global";
                const baseDirPath = await path.join(res, baseDir);
                const entries: InstallEntry[] = (assetObj as CharaInfo).charaFilesPath.map((filePath) => ({
                    url: `https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Skins/main/${subDir}/${filePath.replace(/\\/g, '/')}`,
                    relPath: filePath.replace(/^[^\\]+\\/, '').replace(/\\/g, '/')
                }));

                let totbyts = 0;
                await installFiles(baseDirPath, entries, (pr) => {
                    totbyts += pr.progress;
                    assetDLProgress[assetType][assetRelpath] = 100 * (totbyts / (assetSize * 1024 * 1024));
                }, job.id);
            }
        } catch (error) {
            if (isCancelled(error)) return 'cancelled';
            console.error(`Installing ${assetRelpath} failed:`, error);
            if (!quiet) {
                // A skin package missing on the server has advice of its own
                const missing = assetType === 'Skins' && ErrorCause(error) === 'errors.not_found';
                TriggerError(missing ? get(_)('assets.error.zip_not_found') : `${get(_)('dl.error.failed_abort')}<br>${DescribeError(error)}`, { details: ErrorText(error) });
            }
            return 'failed';
        } finally {
            delete assetDLProgress[assetType][assetRelpath];
            // Already gone after a successful move
            if (assetDownloadFolder) {
                try { await remove(assetDownloadFolder, { recursive: true }); } catch {}
            }
        }

        if (!quiet) TriggerSuccess(get(_)('assets.success.download_complete'));

        currentAssets[assetType][assetRelpath] = {
            assetFolderName: assetRelpath,
            assetVersion: assetVersion
        };
        return 'done';
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
{#if catalogFetchFailed}
<aside class="card p-3 mb-2 flex items-center gap-3 flex-wrap">
	<i class="fa-solid fa-triangle-exclamation text-red-500"></i>
	<span class="flex-1"><b>{$_('common.fetch_error')}</b></span>
	<button type="button" class="button-blue button-main" onclick={updateAssetsInfo}>
		<i class="fa-solid fa-rotate"></i> {$_('common.retry')}
	</button>
</aside>
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
					<div class="flex items-center gap-2">
						<ProgressBar value={assetCountProgressBar[AssetTabType(currentAsset)]} max={100} />
						<button type="button" onclick={() => CancelBulk(AssetTabType(currentAsset))} class="button-gray button-main" disabled={bulkCancelRequested[AssetTabType(currentAsset)]} title={bulkCancelRequested[AssetTabType(currentAsset)] ? $_('jobs.cancelling') : $_('common.cancel')} aria-label={bulkCancelRequested[AssetTabType(currentAsset)] ? $_('jobs.cancelling') : $_('common.cancel')}><i class="fa-solid {bulkCancelRequested[AssetTabType(currentAsset)] ? 'fa-spinner fa-spin' : 'fa-xmark'}"></i></button>
					</div>
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
				<td>{formatSize(SizeMb(info, AssetTabPrefix(currentAsset)))}</td>
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