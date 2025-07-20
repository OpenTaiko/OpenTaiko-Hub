<script>
    import { onMount } from 'svelte';
    import { AppRail, AppRailTile, AppRailAnchor, ProgressBar } from '@skeletonlabs/skeleton';
    import { readTextFile, writeTextFile, mkdir, readDir, exists, copyFile, remove } from '@tauri-apps/plugin-fs';
    import { fetch } from "@tauri-apps/plugin-http";
    import { openPath } from '@tauri-apps/plugin-opener';

    import { getContext } from 'svelte';
    const { TriggerError, TriggerWarning, TriggerSuccess, backoffDownload, GetOS } = getContext('toast');

    import { GetRootPath } from "../lib/utils/path.js";

    // Pages
    import SongsTab from '$lib/pages/SongsTab.svelte';
    import AssetsTab from '$lib/pages/AssetsTab.svelte';
    import InformationTab from '$lib/pages/InformationTab.svelte';
    import ToolsTab from '$lib/pages/ToolsTab.svelte';
    import SaWTab from '$lib/pages/LinksTab.svelte';
    import SecretTab from '$lib/pages/SecretTab.svelte';

    // Logo
    import optkLogoUrl from '$lib/optk.png';

    import { path } from '@tauri-apps/api';
    import { invoke } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';
    import Secret from '../lib/components/Secret.svelte';

    // Navigation
    let currentTile = 0;
    

    // OpTk Version
    const repoOwner = '0AuBSQ';//'OpenTaiko'; 
    const repoName = 'OpenTaiko';//OpenTaiko-Dev-Mirror'; 
    let optk_version = '0.1.0.0';
    let optk_OS = 'Win';
    let buildDetails = 'Loading...';
    let latestVersion = 'Loading...';
    let buildDetailsNotFound = false;
    let latestVersionErrorFound = false;
    let progress = 0;
    let downloadBusy = false;

    const LaunchOpenTaiko = async () => {
        try {
            const os = optk_OS;
            const res = await GetRootPath();
            const appPath = await path.join(res, "./OpenTaiko/OpenTaiko");
            await invoke('execute_external_app', { os, path: appPath });
        } catch (error) {
            TriggerError('Error executing app:' + error);
        }
    }

    const checkSkinCompatibility = (version1, version2) => {
        const regex = /^\d+\.\d+\.\d+\.\d+$/; // Match versions in the form <main>.<major>.<minor>.<patch>

        if (!regex.test(version1) || !regex.test(version2)) {
            return false;
        }

        const [main1, major1, minor1] = version1.split('.').map(Number);
        const [main2, major2, minor2] = version2.split('.').map(Number);

        return main1 === main2 && major1 === major2 && minor1 === minor2;
    }

    const getLatestReleaseUrl = async () => {
        const apiUrl = `https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`;

        try {
            const response = await fetch(apiUrl);
            const data = await response.json();
            
            if (!response.ok) {
                throw new Error(data.message);
            }

            // Find the desired asset in the release
            const asset = data.assets.find(asset => asset.name.endsWith(`${optk_OS}.x64.zip`));
            if (!asset) {
                throw new Error('Desired asset not found in the latest release');
            }

            return asset.browser_download_url; //browser_download_url;
        } catch (err) {
            TriggerError(`Failed to fetch latest release: ${err}`);
            return null;
        }
    }

    const copyAllFilesRecursive = async (src, dst) => {
        try {
            // Read the files and directories in the source folder
            const files = await readDir(src, { recursive: false });

            for (const file of files) {
                const srcPath = `${src}/${file.name}`;
                const dstPath = `${dst}/${file.name}`;
                if (file.isDirectory) {

                    // It's a directory, create it in the destination
                    await mkdir(dstPath, { recursive: true });

                    // Recursively move files from this directory
                    await copyAllFilesRecursive(srcPath, dstPath);
                } else {
                    // It's a file, move it to the destination
                    await copyFile(srcPath, dstPath);
                    console.log(`Moved: ${file.name}`);
                }
            }

            console.log('All files and directories have been moved successfully.');
        } catch (error) {
            console.error('Error moving files:', error);
        }
    }

    const DownloadOpenTaiko = async () => {
        if (downloadBusy === true) {
            TriggerError(`Currently already downloading`);
            return;
        }

        const url = await getLatestReleaseUrl();
        if (!url) return;

        try {
            progress = 0;
            downloadBusy = true;

            // Download the latest OpenTaiko release
            const res = await GetRootPath();
            const base = await path.join(res, "./tmp");
            const dled = await path.join(base, "/OpenTaiko.zip");
            
            const fold_exists = await exists(base);
            if (!fold_exists)
                await mkdir(base);

            let totbyts = 0;
            const success = await backoffDownload(
                url,
                dled,
                (pr) => {
                    totbyts += pr.progress;
                    progress = 100 * (totbyts / pr.total);
                    console.log(progress);
                }
            );

            if (!success) {
                process = undefined;
                downloadBusy = false;
                return ;
            }

            TriggerSuccess('Download complete, now unzipping...');

            // Prepare the optk build folder if doesn't exist
            const optk_folder = await path.join(res, "./OpenTaiko");

            const optkfold_exists = await exists(optk_folder);
            if (!optkfold_exists)
                await mkdir(optk_folder);

            // Unzip and transfer OpenTaiko
            progress = undefined;

            const unlisten = await listen('extract-progress', (event) => {
                progress = event.payload;  // Update progress value
            });

            const folderName = await invoke('unzip_and_get_first_folder', { zipPath: dled });

            unlisten();

            TriggerSuccess('Unzip complete, now installing...');

            // Move unzipped files to OpenTaiko folder
            progress = undefined;

            const source_folder = folderName.replace(/\\/g, '/');
            await copyAllFilesRecursive(source_folder, optk_folder);
            

            // Generate the new version json (temporary? to do on build?)
            const version_file = await path.join(optk_folder, "/version.json");

            const json_ver = {
                version: latestVersion,
                build: optk_OS
            };

            await writeTextFile(version_file, JSON.stringify(json_ver));

            // Clean after pooping
            await remove(dled);
            await remove(source_folder, { recursive: true });

            TriggerSuccess('Download and installation complete');

            
        } catch (err) {
            TriggerError(`Failed to download OpenTaiko: ${err}`)
        }
        await TryFetchingCurrentVersion();
        downloadBusy = false;
    } 

    const TryFetchingLatestVersion = async () => {
        try {
            latestVersionErrorFound = false;
            latestVersion = 'Loading...';
            const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`);
            if (!response.ok) {
                throw new Error(`HTTP error status: ${response.status}`);
            }
            const data = await response.json();
            latestVersion = data.tag_name; // Latest tag version number
        } catch (err) {
            latestVersionErrorFound = true;
            TriggerError(`Failed to fetch latest release: ${err}`);
        }
    }

    const TryFetchingCurrentVersion = async () => {
        try {
            buildDetailsNotFound = false;
            buildDetails = 'Loading...';
            const filePath = './OpenTaiko/version.json';
            const res = await GetRootPath();
            const versionFilePath = await path.join(res,filePath)
            const fileContent = await readTextFile(versionFilePath);
            const jsonData = JSON.parse(fileContent);
            optk_version = jsonData.version;
            optk_OS = jsonData.build;
            buildDetails = `${optk_version} (${optk_OS})`;
        } catch (err) {
            buildDetailsNotFound = true;
            optk_version = '0.1.0.0';
            buildDetails = "No valid OpenTaiko version found";
        }
    }

    const OpenInExplorer = async () => {
        try {
            const res = await GetRootPath();
            const appPath = await path.join(res, "./OpenTaiko");
            await openPath(appPath);
        } catch (error) {
            TriggerError('Error opening the folder:' + error);
        }
    }
  
    onMount(async () => {
        optk_OS = await GetOS();
        await TryFetchingCurrentVersion();
        await TryFetchingLatestVersion();
    });
</script>

<div class="grid h-screen grid-rows-[auto_1fr_auto]">
    <!-- Grid Columns -->
    <div class="grid grid-cols-1 md:grid-cols-[auto_1fr]">
      <!-- Left Sidebar. -->
      <aside class="bg-yellow-500">
        <AppRail height="h-full">
            <!-- <svelte:fragment slot="lead">
                <AppRailAnchor href="/" >(icon)</AppRailAnchor>
            </svelte:fragment> -->
            <!-- --- -->
            <AppRailTile bind:group={currentTile} name="tile-1" value={0} title="Download, update, or launch OpenTaiko.">
                <svelte:fragment slot="lead"><i class="fa-solid fa-download"></i></svelte:fragment>
                <span>OpenTaiko Version</span>
            </AppRailTile>
            <AppRailTile bind:group={currentTile} name="tile-2" value={1} title="Update your OpenTaiko songs and download the latest ones.">
                <svelte:fragment slot="lead"><i class="fa-solid fa-music"></i></svelte:fragment>
                <span>Songlist</span>
            </AppRailTile>
            <AppRailTile bind:group={currentTile} name="tile-3" value={2} title="Get the newest OpenTaiko skins and related assets.">
                <svelte:fragment slot="lead"><i class="fa-solid fa-pen-ruler"></i></svelte:fragment>
                <span>Skins</span>
            </AppRailTile>
            <AppRailTile bind:group={currentTile} name="tile-4" value={3} title="Tools that can improve your OpenTaiko experience.">
                <svelte:fragment slot="lead"><i class="fa-solid fa-screwdriver-wrench"></i></svelte:fragment>
                <span>Tools</span>
            </AppRailTile>
            <AppRailTile bind:group={currentTile} name="tile-5" value={4} title="???">
                <svelte:fragment slot="lead"><i class="fa-solid fa-question"></i></svelte:fragment>
                <span>Secrets</span>
            </AppRailTile>
            <hr> <!-- Trail -->
            <svelte:fragment slot="trail">
                <AppRailTile bind:group={currentTile} name="tile-6" value={5} title="To consult the changelogs, the documentation, or for troubleshooting.">
                    <svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
                    <span>Information</span>
                </AppRailTile>
                <AppRailTile bind:group={currentTile} name="tile-7" value={6} title="Check out OpenTaiko's socials and websites!">
                    <svelte:fragment slot="lead"><i class="fa-solid fa-globe"></i></svelte:fragment>
                    <span>Links</span>
                </AppRailTile>
				<AppRailAnchor href="https://github.com/OpenTaiko/OpenTaiko-Hub" target="_blank" title="View the OpenTaiko Hub source code.">
					<i class="fa-brands fa-github text-2xl"></i>
				</AppRailAnchor>
			</svelte:fragment>
        </AppRail>
      </aside>
      <!-- Main Content -->
      <main class="h-screen space-y-4 p-4">
        <!-- OpenTaiko Version Page -->
        {#if currentTile === 0}
            <img src={optkLogoUrl} alt="Logo" class="mx-auto" />
            

            <section class="card w-full">
                <div class="p-4 space-y-4">
                    <div class="flex gap-4">
                        <span>Current version: </span>
                        {#if buildDetails === "Loading..."}
                            <div class="placeholder animate-pulse flex-1" />
                        {:else}
                            {#if downloadBusy === true}
                                <ProgressBar bind:value={progress} max={100} />
                            {:else}
                                <span>{buildDetails}</span>
                                <button type="button" on:click={TryFetchingCurrentVersion} class="text-white bg-blue-700 hover:bg-blue-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-blue-600 dark:hover:bg-blue-700">Reload</button>
                                {#if buildDetailsNotFound === true}
                                    <button type="button" on:click={DownloadOpenTaiko} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700">Download OpenTaiko</button>
                                {:else if latestVersion !== optk_version && 'Loading...' !== latestVersion}
                                    <button type="button" on:click={LaunchOpenTaiko} class="text-white bg-blue-700 hover:bg-blue-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-blue-600 dark:hover:bg-blue-700">Launch OpenTaiko</button>
                                    <button type="button" on:click={OpenInExplorer} class="text-white bg-blue-700 hover:bg-blue-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-blue-600 dark:hover:bg-blue-700">Open in Explorer</button>
                                    <button type="button" on:click={DownloadOpenTaiko} class="text-white bg-green-700 hover:bg-green-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-green-600 dark:hover:bg-green-700">Update OpenTaiko</button>
                                    {#if checkSkinCompatibility(latestVersion, optk_version) === false}
                                        <span class="text-red-500">(Updating will require a skin update)</span>
                                    {/if}
                                    
                                {:else}
                                    <button type="button" on:click={LaunchOpenTaiko} class="text-white bg-blue-700 hover:bg-blue-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-blue-600 dark:hover:bg-blue-700">Launch OpenTaiko</button>
                                    <button type="button" on:click={OpenInExplorer} class="text-white bg-blue-700 hover:bg-blue-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-blue-600 dark:hover:bg-blue-700">Open in Explorer</button>
                                    <button type="button" on:click={DownloadOpenTaiko} class="text-white bg-gray-700 hover:bg-gray-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-gray-600 dark:hover:bg-gray-700">Redownload OpenTaiko</button>
                                {/if}
                            {/if}
                        {/if}
                    </div>
                </div>
            </section>

            <section class="card w-full">
                <div class="p-4 space-y-4">
                    <div class="flex gap-4">
                        <span>Latest version: </span>
                        {#if latestVersionErrorFound === true}
                            <span class="text-red-500">Fetch Error</span>
                            <button type="button" on:click={TryFetchingLatestVersion} class="text-white bg-red-700 hover:bg-red-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-red-600 dark:hover:bg-red-700">Retry</button>
                        {:else if latestVersion === "Loading..."}
                            <div class="placeholder animate-pulse flex-1" />
                        {:else}
                            <span>{latestVersion}</span>
                            <button type="button" on:click={TryFetchingLatestVersion} class="text-white bg-blue-700 hover:bg-blue-800 font-medium rounded-lg text-sm px-3 py-1 dark:bg-blue-600 dark:hover:bg-blue-700">Reload</button>
                        {/if}
                        
                    </div>
                </div>
            </section>

            <p>Current OS: {optk_OS}</p>
            <p>Be sure to download a skin (Skins tab) and songs (Songlist tab) before first starting the game!</p>
        {/if}

        <!-- Songs -->
        <div class="w-full h-full" style:display={currentTile === 1 ? null : 'none'}>
            <SongsTab />
        </div>
        

        <!-- Skins -->
        <div class="w-full h-full" style:display={currentTile === 2 ? null : 'none'}>
            <AssetsTab
                bind:optk_version
            />
        </div>
        
        <!-- Tools -->
        {#if currentTile === 3}
            <ToolsTab />
        {/if}

        <!-- ??? -->
        {#if currentTile === 4}
            <SecretTab />
        {/if}

        <!-- Information -->
        {#if currentTile === 5}
            <InformationTab />
        {/if}

        <!-- Socials and Websites -->
        {#if currentTile === 6}
            <SaWTab />
        {/if}
      </main>    
    </div>
</div>

<style>
    main {
        overflow-y: auto;
    }
    hr {
        width: 90%;
        margin-top: 29px;
        margin-left: auto;
        margin-right: auto;
        border-width: 1px;
    }
</style>