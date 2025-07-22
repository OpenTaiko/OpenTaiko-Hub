<script>
    import { onMount } from 'svelte';
    import { fetch } from "@tauri-apps/plugin-http";

    import { getContext } from 'svelte';
    const { TriggerError, TriggerSuccess } = getContext('toast');

    // Version Number
    const versionHub = "v0.1.6";

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
            
            if (versionHub == latestVersion) {
                console.log("Hub version is up to date!");
            } 
            else if (versionHub != latestVersion) {
                const UpdateHubButton = document.getElementById('UpdateHubButton'); 
                UpdateHubButton.style.display = 'block';
                
                console.log("Hub version is out of date, or newer than the current version on GitHub.");
                TriggerSuccess('Your OpenTaiko Hub installation is out of date.<br>Please click the "Update OpenTaiko Hub" button.');
            }
        } catch (err) {
            latestVersionErrorFound = true;
            TriggerError(`Failed to fetch latest release: ${err}`);
        }
    }

    const UpdateHub = async () => {window.open("https://github.com/OpenTaiko/OpenTaiko-Hub/releases/latest");} 
    
    onMount(async () => {
        await TryFetchingLatestVersion();
    });
</script>

<section id="UpdateHubButton" class="card w-full updatebutton">
    <div class="p-4 space-y-4">
        <div class="flex gap-4">
            Your OpenTaiko Hub installation is out of date;
            <button type="button" on:click={UpdateHub} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700">Update OpenTaiko Hub</button>
        </div>
    </div>
</section>

<style>
.updatebutton {display: none;}
</style>