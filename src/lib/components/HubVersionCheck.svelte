<script>
    import { onMount } from 'svelte';
    import { fetch } from "@tauri-apps/plugin-http";

    import { getContext } from 'svelte';
    const { TriggerError, TriggerSuccess } = getContext('toast');

    // Version Number
    const VersionHub = "v0.1.7";

    // Check version number
    const repoOwner = 'OpenTaiko';//'OpenTaiko'; 
    const repoName = 'OpenTaiko-Hub';//OpenTaiko-Dev-Mirror'; 
    let latestVersion = 'Loading...';
    let latestVersionErrorFound = false;


    const TryFetchingLatestVersion = async () => {
        try {
            latestVersionErrorFound = false;
            const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`);
            if (!response.ok) {
                throw new Error(`HTTP error status: ${response.status}`);
            }
            const data = await response.json();
            latestVersion = data.tag_name; // Latest tag version number
            
            if (VersionHub == latestVersion) {
                console.log("Hub version is up to date!");
            } 
            else if (VersionHub != latestVersion) {
                console.log("Hub version is out of date, or newer than the current version on GitHub.");
                TriggerSuccess('Your OpenTaiko Hub installation is out of date.<br>Please click the "Update OpenTaiko Hub" button.'); 
            }
        } catch (err) {
            latestVersionErrorFound = true;
            TriggerError(`Failed to fetch latest OpenTaiko Hub release: ${err}`);
        }
    }

    const UpdateHub = async () => {window.open("https://github.com/OpenTaiko/OpenTaiko-Hub/releases/latest");} 
    
    onMount(async () => {
        await TryFetchingLatestVersion();
    });
</script>


<section class="card w-full">
    <div class="p-4 space-y-4">
        <div class="flex gap-4">
            {#if latestVersionErrorFound === true}
                <span>Failed to check for updates.</span>
                <span class="text-red-500">Fetch Error</span>
                <button type="button" on:click={TryFetchingLatestVersion} class="text-white bg-red-700 hover:bg-red-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-red-600 dark:hover:bg-red-700 button-red">Retry</button>
            {:else if latestVersion === "Loading..."}
                <span>Checking for updates...</span>
                <div class="placeholder animate-pulse flex-1" />
            {:else if VersionHub != latestVersion}
                <span><b>Your OpenTaiko Hub installation is out of date.</b></span>
                <button type="button" on:click={UpdateHub} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700 button-green">Update OpenTaiko Hub to {latestVersion.slice(1)}</button>
            {:else}
                <span><b>Your OpenTaiko Hub installation is up to date!</b></span>
            {/if}
        </div>
    </div>
</section>

<div class="hubversionnumber">OpenTaiko Hub ({VersionHub.slice(1)})</div>

<style>
    .hubversionnumber {
        font-weight: bold;
        position: fixed;
        bottom: 10px;
        right: 15px;
    }
</style>