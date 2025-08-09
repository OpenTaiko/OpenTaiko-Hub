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
            <button type="button" on:click={() => DownloadMethod(AssetInfo, null, AssetType)} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700 button-green">Download</button>
        {:else}
            <ProgressBar bind:value={Progress} max={100} />
        {/if}
    {:else if CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]].assetVersion === AssetInfo[`${AssetPrefix}Version`]}
        <p>{CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]].assetVersion}</p>
        <br />
        {#if Progress === undefined}
            <button type="button" on:click={() => DownloadMethod(AssetInfo, CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]], AssetType)} class="text-white bg-gray-700 hover:bg-gray-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-gray-600 dark:hover:bg-gray-700 button-gray">Redownload</button>
        {:else}
            <ProgressBar bind:value={Progress} max={100} />
        {/if}
    {:else}
        <p>{CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]].assetVersion}</p>
        <br />
        {#if Progress === undefined}
            <button type="button" on:click={() => DownloadMethod(AssetInfo, CurrentAssets[AssetType][AssetInfo[`${AssetPrefix}Folder`]], AssetType)} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700 button-green">Update</button>
        {:else}
            <ProgressBar bind:value={Progress} max={100} />
        {/if}
    {/if}
    
    <style>

    </style>