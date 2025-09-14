<script>
    import { ProgressBar } from '@skeletonlabs/skeleton';
    
    export let IsScanning = true;
    export let AssetType = "Skins";
    export let AssetInfo = {};
    export let CurrentAssets = {};
    export let DownloadMethod = () => {};
    export let Progress = undefined;

    $: AssetPrefix = (AssetType === "Skins") ? "skin" : "chara";
 
    </script>

    {#if IsScanning === true || CurrentAssets[AssetType] === undefined}
        <p>Scanning...</p>
    {:else if !Object.keys(CurrentAssets[AssetType]).includes(AssetInfo[`${AssetPrefix}Folder`])}
        <p>Not downloaded</p>
        <br />
        {#if Progress === undefined}
            <button type="button" on:click={() => DownloadMethod(AssetInfo, null, AssetType)} class="button-green button-main"><i class="fa-solid fa-download"></i> Download</button>
        {:else}
            <ProgressBar bind:value={Progress} max={100} />
        {/if}
    {:else if CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]].assetVersion === AssetInfo[`${AssetPrefix}Version`]}
        <p>{CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]].assetVersion}</p>
        <br />
        {#if Progress === undefined}
            <button type="button" on:click={() => DownloadMethod(AssetInfo, CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]], AssetType)} class="button-gray button-main"><i class="fa-solid fa-download"></i> Redownload</button>
        {:else}
            <ProgressBar bind:value={Progress} max={100} />
        {/if}
    {:else}
        <p>{CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]].assetVersion}</p>
        <br />
        {#if Progress === undefined}
            <button type="button" on:click={() => DownloadMethod(AssetInfo, CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]], AssetType)} class="button-green button-main"><i class="fa-solid fa-download"></i> Update</button>
        {:else}
            <ProgressBar bind:value={Progress} max={100} />
        {/if}
    {/if}
    
    <style>

    </style>