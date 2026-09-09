<script lang="ts">
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { _ } from 'svelte-i18n';
    import type { AssetInfo as AssetInfoRecord, AssetType as AssetKind, LocalAsset } from '$lib/types';

    interface Props {
        IsScanning?: boolean;
        AssetType?: AssetKind;
        AssetInfo: AssetInfoRecord;
        /** Assets found in the instance, per type, keyed by folder name. */
        CurrentAssets?: Partial<Record<AssetKind, Record<string, LocalAsset>>>;
        DownloadMethod?: (asset: AssetInfoRecord, current: LocalAsset | null, type: AssetKind) => void;
        /** Download progress in percent; undefined while idle. */
        Progress?: number;
    }

    let {
        IsScanning = true,
        AssetType = "Skins",
        AssetInfo,
        CurrentAssets = {},
        DownloadMethod = () => {},
        Progress = undefined
    }: Props = $props();

    let AssetPrefix = $derived((AssetType === "Skins") ? "skin" : "chara");

    // Catalog keys are prefixed with the asset kind ("skinFolder", "charaFolder", ...)
    let folder = $derived(String((AssetInfo as Record<string, unknown>)[`${AssetPrefix}Folder`]));
    let catalogVersion = $derived(String((AssetInfo as Record<string, unknown>)[`${AssetPrefix}Version`]));
    let installed = $derived(CurrentAssets[AssetType]);
    let local = $derived(installed?.[folder] ?? null);
</script>

    {#if IsScanning === true || installed === undefined}
        <p>{$_('asset_cell.scanning')}</p>
    {:else}
        <p>{local?.assetVersion ?? $_('asset_cell.not_downloaded')}</p>
        <br />
        {#if Progress === undefined}
            {#if local === null}
                <button type="button" onclick={() => DownloadMethod(AssetInfo, null, AssetType)} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('asset_cell.button.download')}</button>
            {:else if local.assetVersion !== catalogVersion}
                <button type="button" onclick={() => DownloadMethod(AssetInfo, local, AssetType)} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('asset_cell.button.update')}</button>
            {:else}
                <button type="button" onclick={() => DownloadMethod(AssetInfo, local, AssetType)} class="button-gray button-main"><i class="fa-solid fa-download"></i> {$_('asset_cell.button.redownload')}</button>
            {/if}
        {:else}
            <ProgressBar value={Progress} max={100} />
        {/if}
    {/if}

    <style>

    </style>
