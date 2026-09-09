<script lang="ts">
    import { getContext } from 'svelte';
    import { Tabs } from '@skeletonlabs/skeleton-svelte';
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { writeTextFile, mkdir, exists, remove } from '@tauri-apps/plugin-fs';
    import { path } from '@tauri-apps/api';
    import { invoke, Channel } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import type { BuildOption, GitHubRelease, HubOS, IndevBuildInfo, Instance, ToastContext } from '$lib/types';
    const { TriggerError, TriggerSuccess, backoffDownload } = getContext<ToastContext>('toast');

    import { GetOS, GetTmpPath } from "$lib/utils/path";
    import { isVersionAtLeast } from "$lib/utils/versions";
    import {
        repoOwner,
        repoName,
        INDEV_BRANCH,
        INDEV_LABEL,
        fetchLatestRelease,
        fetchReleaseByTag,
        fetchBranchHeadSha
    } from "$lib/utils/builds";
    import {
        activeInstance,
        instanceVersions,
        refreshInstanceVersion,
        updateInstance,
        linkGlobalSongs
    } from "$lib/stores/instances";

    // Components
    import HubVersionCheck from '$lib/components/HubVersionCheck.svelte';
    import BuildPickerModal from '$lib/components/BuildPickerModal.svelte';
    import AssetsTab from '$lib/pages/AssetsTab.svelte';
    import DocsTab from '$lib/pages/DocsTab.svelte';
    import SavesTab from '$lib/pages/SavesTab.svelte';

    // Images
    import optkLogoUrl from '$lib/optk.png';

    let optk_OS = $state<HubOS>('Win');
    GetOS().then((os) => optk_OS = os);

    // Sub-tabs: 0 Overview, 1 Skins, 2 Characters, 3 Puchicharas, 4 Docs
    let homeSubTab = $state(0);

    // Latest stable release
    let latestVersion = $state<string | null>(null);
    let latestRelease: GitHubRelease | null = null;
    let latestLoading = $state(true);
    let latestVersionErrorFound = $state(false);

    // Active instance state
    let versionLoading = $state(false);
    let versionLoadedFor: string | null = null;       // bookkeeping only, never rendered
    let indevLatestSha = $state<string | null>(null);

    // Download / build state
    let downloadBusy = $state(false);
    // undefined renders an indeterminate bar
    let progress = $state<number | undefined>(0);
    let buildStage = $state<BuildStage | null>(null);
    let buildLog = $state<string[]>([]);
    let buildLogElement = $state<HTMLPreElement | null>(null);

    type BuildStage = 'check_sdk' | 'fetch' | 'download' | 'extract' | 'build' | 'install';

    // Build picker
    let showBuildPicker = $state(false);

    let current = $derived($activeInstance);
    let currentVersion = $derived(current ? ($instanceVersions[current.id] ?? null) : null);
    // The experimental build record, narrowed to the branch-built kind
    let indevBuild = $derived<IndevBuildInfo | null>(
        current?.channel === 'experimental' && current.experimental?.kind === 'indev' ? current.experimental : null
    );
    let isIndev = $derived(indevBuild !== null);
    let isPrerelease = $derived(current?.channel === 'experimental' && current?.experimental?.kind === 'prerelease');
    let indevUpdateAvailable = $derived(indevBuild !== null && indevLatestSha !== null && indevLatestSha !== indevBuild.sha);

    // Sub-tab availability
    let assetsAvailable = $derived(current !== null && current.channel !== 'experimental' && currentVersion !== null);
    let docsAvailable = $derived(current !== null && isVersionAtLeast(currentVersion, '0.6.1'));
    let savesAvailable = $derived(current !== null && currentVersion !== null); // any installed build has saves
    $effect(() => {
        if ((homeSubTab >= 1 && homeSubTab <= 3 && !assetsAvailable) || (homeSubTab === 4 && !docsAvailable) || (homeSubTab === 5 && !savesAvailable)) homeSubTab = 0;
    });

    $effect(() => {
        if (current && versionLoadedFor !== current.id) {
            versionLoadedFor = current.id;
            LoadInstanceState();
        }
    });

    // An instance created through the build picker carries its chosen build; start it
    let pendingStartedFor: string | null = null;      // bookkeeping only, never rendered
    $effect(() => {
        if (current?.pendingInstall && !downloadBusy && pendingStartedFor !== current.id) {
            pendingStartedFor = current.id;
            StartPendingInstall(current);
        }
    });

    // Keep the build log scrolled to its newest line as output streams in
    $effect(() => {
        void buildLog.length;
        if (buildLogElement) buildLogElement.scrollTop = buildLogElement.scrollHeight;
    });

    const LoadInstanceState = async () => {
        if (!current) return;
        versionLoading = true;
        indevLatestSha = null;
        await refreshInstanceVersion(current.id);
        versionLoading = false;
        if (indevBuild) {
            CheckIndevUpdate();
        }
    };

    const CheckIndevUpdate = async () => {
        try {
            indevLatestSha = await fetchBranchHeadSha(indevBuild?.branch ?? INDEV_BRANCH);
        } catch (error) {
            console.error('Failed to check the experimental branch:', error);
        }
    };

    const checkSkinCompatibility = (version1: string | null, version2: string | null): boolean => {
        const regex = /^\d+\.\d+\.\d+\.\d+$/; // Match versions in the form <main>.<major>.<minor>.<patch>

        if (!version1 || !version2 || !regex.test(version1) || !regex.test(version2)) {
            return false;
        }

        const [main1, major1, minor1] = version1.split('.').map(Number);
        const [main2, major2, minor2] = version2.split('.').map(Number);

        return main1 === main2 && major1 === major2 && minor1 === minor2;
    }

    const TryFetchingLatestVersion = async () => {
        try {
            latestVersionErrorFound = false;
            latestLoading = true;
            latestRelease = await fetchLatestRelease();
            latestVersion = latestRelease.tag_name; // Latest tag version number
        } catch (err) {
            latestVersionErrorFound = true;
            TriggerError(get(_)('home.error.fetch_release', { values: { error: String(err) } }));
        }
        latestLoading = false;
    }

    const StartPendingInstall = async (instance: Instance) => {
        const build = instance.pendingInstall;
        await updateInstance(instance.id, { pendingInstall: null });
        if (!build) return;
        if (build.kind === 'stable') {
            await DownloadStable();
        } else if (build.kind === 'indev') {
            await DownloadIndev(build);
        } else if (build.kind === 'prerelease') {
            await DownloadPrerelease(build);
        }
    };

    const OnBuildPicked = async (build: BuildOption) => {
        if (!current) return;
        pendingStartedFor = null;
        await updateInstance(current.id, {
            channel: build.kind === 'stable' ? 'stable' : 'experimental',
            pendingInstall: build
        });
    };

    const WriteVersionJson = async (instancePath: string, version: string | null) => {
        // Backwards compatibility with pre-0.2 Hub versions
        try {
            const versionFile = await path.join(instancePath, 'version.json');
            await writeTextFile(versionFile, JSON.stringify({ version, build: optk_OS }));
        } catch (error) {
            console.error('Failed to write version.json:', error);
        }
    };

    // A finished install must at least contain the game executable; a missing one
    // means the install was interrupted or the build output is broken.
    const VerifyInstall = async (instancePath: string) => {
        const exeName = optk_OS === 'Win' ? 'OpenTaiko.exe' : 'OpenTaiko';
        if (!(await exists(await path.join(instancePath, exeName)))) {
            throw new Error(get(_)('home.error.incomplete_build'));
        }
    };

    // Downloads a release zip and merges its content into the instance folder.
    const DownloadAndInstallZip = async (url: string, instancePath: string): Promise<boolean> => {
        const tmpDir = await GetTmpPath(crypto.randomUUID());
        await mkdir(tmpDir, { recursive: true });
        const zipPath = await path.join(tmpDir, 'OpenTaiko.zip');

        try {
            let totalBytes = 0;
            const success = await backoffDownload(
                url,
                zipPath,
                (pr) => {
                    totalBytes += pr.progress;
                    progress = pr.total ? 100 * (totalBytes / pr.total) : undefined;
                }
            );
            if (!success) {
                return false;
            }

            TriggerSuccess(get(_)('home.success.unzipping'));
            progress = undefined;

            const unlisten = await listen<number>('extract-progress', (event) => {
                progress = event.payload;
            });
            let sourceFolder: string;
            try {
                sourceFolder = await invoke<string>('unzip_and_get_first_folder', { zipPath });
            } finally {
                unlisten();
            }

            TriggerSuccess(get(_)('home.success.installing'));
            progress = undefined;

            await remove(zipPath);
            // Songs live in the shared library, so never install a build's own copies
            await invoke('strip_song_content', { songsDir: await path.join(sourceFolder, 'Songs') });
            await invoke('merge_move_dir', { src: sourceFolder, dest: instancePath });
            await VerifyInstall(instancePath);
            return true;
        } finally {
            try { await remove(tmpDir, { recursive: true }); } catch {}
        }
    };

    const DownloadStable = async () => {
        if (downloadBusy === true) {
            TriggerError(get(_)('home.error.already_downloading'));
            return;
        }
        if (!current) return;
        // Capture the target so switching instances mid-download cannot retarget it
        const instance = current;

        try {
            if (!latestRelease) await TryFetchingLatestVersion();
            const asset = latestRelease?.assets?.find((asset) => asset.name.endsWith(`${optk_OS}.x64.zip`));
            if (!asset) {
                throw new Error('Desired asset not found in the latest release');
            }

            progress = 0;
            downloadBusy = true;

            const installed = await DownloadAndInstallZip(asset.browser_download_url, instance.path);
            if (installed) {
                await WriteVersionJson(instance.path, latestVersion);
                const updated = await updateInstance(instance.id, { channel: 'stable', experimental: null });
                await linkGlobalSongs(updated);
                await refreshInstanceVersion(instance.id);
                TriggerSuccess(get(_)('home.success.installed'));
            }
        } catch (err) {
            TriggerError(get(_)('home.error.download_failed', { values: { error: String(err) } }));
        }
        downloadBusy = false;
    };

    const DownloadPrerelease = async (option: { tag: string; label: string }) => {
        if (downloadBusy === true) {
            TriggerError(get(_)('home.error.already_downloading'));
            return;
        }
        if (!current) return;
        // Capture the target so switching instances mid-download cannot retarget it
        const instance = current;

        try {
            const release = await fetchReleaseByTag(option.tag);
            const asset = release.assets?.find((asset) => asset.name.endsWith(`${optk_OS}.x64.zip`));
            if (!asset) {
                throw new Error('Desired asset not found in the prerelease');
            }

            progress = 0;
            downloadBusy = true;

            const installed = await DownloadAndInstallZip(asset.browser_download_url, instance.path);
            if (installed) {
                await WriteVersionJson(instance.path, option.tag);
                const updated = await updateInstance(instance.id, {
                    channel: 'experimental',
                    experimental: { kind: 'prerelease', tag: option.tag, label: option.label }
                });
                await linkGlobalSongs(updated);
                await refreshInstanceVersion(instance.id);
                TriggerSuccess(get(_)('home.success.installed'));
            }
        } catch (err) {
            TriggerError(get(_)('home.error.download_failed', { values: { error: String(err) } }));
        }
        downloadBusy = false;
    };

    // Fetches the experimental branch source at its current head commit, builds it with
    // the .NET SDK and installs the publish output into the instance folder.
    const DownloadIndev = async (option: { branch?: string; label?: string }) => {
        if (downloadBusy === true) {
            TriggerError(get(_)('home.error.already_downloading'));
            return;
        }
        if (!current) return;
        // Capture the target so switching instances mid-download cannot retarget it
        const instance = current;

        downloadBusy = true;
        buildLog = [];
        let tmpDir: string | null = null;

        try {
            // 1. The build requires the .NET SDK 8
            buildStage = 'check_sdk';
            progress = undefined;
            const sdkLines: string[] = [];
            const sdkChannel = new Channel<string>();
            sdkChannel.onmessage = (line) => sdkLines.push(line);
            let sdkExitCode = -1;
            try {
                sdkExitCode = await invoke<number>('run_streamed', {
                    program: 'dotnet',
                    args: ['--list-sdks'],
                    cwd: null,
                    onOutput: sdkChannel
                });
            } catch {
                sdkExitCode = -1;
            }
            if (sdkExitCode !== 0 || !sdkLines.some((line) => line.trim().startsWith('8.'))) {
                TriggerError(get(_)('home.experimental.requires_dotnet'));
                return;
            }

            // 2. Resolve the branch head so the source and the update check are consistent
            buildStage = 'fetch';
            const branch = option.branch ?? INDEV_BRANCH;
            const sha = await fetchBranchHeadSha(branch);
            if (!sha) {
                throw new Error('Could not resolve the branch head commit');
            }

            // 3. Download the source archive of that exact commit
            buildStage = 'download';
            progress = undefined;
            tmpDir = await GetTmpPath(`indev-${crypto.randomUUID()}`);
            await mkdir(tmpDir, { recursive: true });
            const srcZip = await path.join(tmpDir, 'src.zip');
            let totalBytes = 0;
            const success = await backoffDownload(
                `https://codeload.github.com/${repoOwner}/${repoName}/zip/${sha}`,
                srcZip,
                (pr) => {
                    totalBytes += pr.progress;
                    progress = pr.total ? 100 * (totalBytes / pr.total) : undefined;
                },
                3
            );
            if (!success) {
                throw new Error('Source download failed');
            }

            // 4. Extract
            buildStage = 'extract';
            progress = 0;
            const unlisten = await listen<number>('extract-progress', (event) => {
                progress = event.payload;
            });
            let srcFolder: string;
            try {
                srcFolder = await invoke<string>('unzip_and_get_first_folder', { zipPath: srcZip });
            } finally {
                unlisten();
            }
            await remove(srcZip);

            // 5. Build (same command as the game's build scripts)
            buildStage = 'build';
            progress = undefined;
            const rid = optk_OS === 'Win' ? 'win-x64' : 'linux-x64';
            const logChannel = new Channel<string>();
            logChannel.onmessage = (line) => {
                buildLog = [...buildLog.slice(-199), line];
            };
            const exitCode = await invoke<number>('run_streamed', {
                program: 'dotnet',
                args: ['publish', 'OpenTaiko/OpenTaiko.csproj', '--configuration', 'Release', '--self-contained', '-p:PublishSingleFile=true', '--runtime', rid],
                cwd: srcFolder,
                onOutput: logChannel
            });
            if (exitCode !== 0) {
                throw new Error(`dotnet publish exited with code ${exitCode}`);
            }

            // 6. Install the publish output (and local docs when the branch ships them)
            buildStage = 'install';
            const publishDir = await path.join(srcFolder, 'OpenTaiko', 'bin', 'Release', 'net8.0', rid, 'publish');
            // A local publish carries the repository's whole Songs tree; drop those
            // charts so the instance uses only the shared library
            await invoke('strip_song_content', { songsDir: await path.join(publishDir, 'Songs') });
            await invoke('merge_move_dir', { src: publishDir, dest: instance.path });
            await VerifyInstall(instance.path);
            const docsDir = await path.join(srcFolder, 'OpenTaiko', 'docs');
            if (await exists(docsDir)) {
                await invoke('merge_move_dir', { src: docsDir, dest: await path.join(instance.path, 'docs') });
            }

            // 7. Record build metadata for update checks and link the shared library
            await WriteVersionJson(instance.path, '0.6.1.0');
            const updated = await updateInstance(instance.id, {
                channel: 'experimental',
                experimental: { kind: 'indev', branch, label: option.label ?? INDEV_LABEL, sha, builtAt: new Date().toISOString() }
            });
            await linkGlobalSongs(updated);
            const version = await refreshInstanceVersion(instance.id);
            if (version) await WriteVersionJson(instance.path, version);
            indevLatestSha = sha;

            TriggerSuccess(get(_)('home.success.installed'));
        } catch (err) {
            TriggerError(get(_)('home.error.download_failed', { values: { error: String(err) } }));
        } finally {
            if (tmpDir) {
                try { await remove(tmpDir, { recursive: true }); } catch {}
            }
            downloadBusy = false;
            buildStage = null;
        }
    };

    const LaunchOpenTaiko = async () => {
        if (!current) return;
        try {
            const appPath = await path.join(current.path, "OpenTaiko");
            await invoke('execute_external_app', { os: optk_OS, path: appPath });
        } catch (error) {
            TriggerError(get(_)('home.error.launch', { values: { error: String(error) } }));
        }
    }

    // Opening the instance folder lives in the instance bar (one canonical place,
    // next to the instance selector) instead of being repeated in every button row.

    TryFetchingLatestVersion();
</script>

<Tabs value={String(homeSubTab)} onValueChange={(details) => homeSubTab = Number(details.value)} class="tab-bar w-full">
    <Tabs.List class="justify-center">
        <Tabs.Trigger value="0">
            <i class="fa-solid fa-house"></i>
            <span>{$_('home.tab.overview')}</span>
        </Tabs.Trigger>
        {#if assetsAvailable}
        <Tabs.Trigger value="1">
            <i class="fa-solid fa-palette"></i>
            <span>{$_('assets.tab.skins')}</span>
        </Tabs.Trigger>
        <Tabs.Trigger value="2">
            <i class="fa-solid fa-user"></i>
            <span>{$_('assets.tab.characters')}</span>
        </Tabs.Trigger>
        <Tabs.Trigger value="3">
            <i class="fa-solid fa-circle-half-stroke"></i>
            <span>{$_('assets.tab.puchicharas')}</span>
        </Tabs.Trigger>
        {/if}
        {#if docsAvailable}
        <Tabs.Trigger value="4">
            <i class="fa-solid fa-book"></i>
            <span>{$_('docs.title')}</span>
        </Tabs.Trigger>
        {/if}
        {#if savesAvailable}
        <Tabs.Trigger value="5">
            <i class="fa-solid fa-floppy-disk"></i>
            <span>{$_('saves.tab')}</span>
        </Tabs.Trigger>
        {/if}
    </Tabs.List>
</Tabs>

<!-- Overview (bottom padding keeps content scrollable above the fixed Hub-version footer) -->
<div class="pb-24" style:display={homeSubTab === 0 ? null : 'none'}>
    <img src={optkLogoUrl} alt="Logo" class="mx-auto" />

    <section class="card w-full">
        {#if !current}
            <div class="p-4 space-y-4">
                <p><b>{$_('instances.none')}</b></p>
            </div>
        {:else}
            <div class="p-4 space-y-4">
                <div class="flex gap-4 flex-wrap items-center">
                    <span class="nowrap"><b>{$_('home.label.current_version')}</b></span>
                    {#if versionLoading}
                        <div class="placeholder animate-pulse flex-1"></div>
                    {:else if downloadBusy === true}
                        <div class="progressbar">
                            {#if buildStage}
                                <p class="text-sm mb-1"><i class="fa-solid fa-flask"></i> {$_(`home.experimental.stage.${buildStage}`)}</p>
                            {/if}
                            <ProgressBar value={progress} max={100} />
                        </div>
                    {:else}
                        {#if currentVersion}
                            <span>
                                {currentVersion}
                                {#if current.channel === 'experimental'}
                                    ({current.experimental?.label ?? $_('instances.channel.experimental')}{#if indevBuild?.sha}&nbsp;@{indevBuild.sha.slice(0, 7)}{/if})
                                {:else}
                                    ({optk_OS})
                                {/if}
                            </span>
                        {:else}
                            <span>{$_('home.label.no_version_found')}</span>
                        {/if}
                        <button type="button" onclick={LoadInstanceState} class="button-blue button-main"><i class="fa-solid fa-rotate"></i> {$_('home.button.reload')}</button>

                        {#if !currentVersion}
                            <!-- Empty instance: pick which build to install -->
                            <button type="button" onclick={() => showBuildPicker = true} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('home.button.install')}</button>
                        {:else if isIndev}
                            <button type="button" onclick={LaunchOpenTaiko} class="button-blue button-main"><i class="fa-solid fa-rocket"></i> {$_('home.button.launch')}</button>
                            {#if indevUpdateAvailable}
                                <button type="button" onclick={() => DownloadIndev(indevBuild ?? {})} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('home.button.update')}</button>
                            {:else}
                                <button type="button" onclick={() => DownloadIndev(indevBuild ?? {})} class="button-gray button-main"><i class="fa-solid fa-hammer"></i> {$_('home.button.rebuild')}</button>
                            {/if}
                        {:else if isPrerelease}
                            <button type="button" onclick={LaunchOpenTaiko} class="button-blue button-main"><i class="fa-solid fa-rocket"></i> {$_('home.button.launch')}</button>
                            <button type="button" onclick={() => showBuildPicker = true} class="button-gray button-main"><i class="fa-solid fa-download"></i> {$_('home.button.redownload')}</button>
                        {:else if latestVersion !== null && latestVersion !== currentVersion}
                            <button type="button" onclick={LaunchOpenTaiko} class="button-blue button-main"><i class="fa-solid fa-rocket"></i> {$_('home.button.launch')}</button>
                            <button type="button" onclick={DownloadStable} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('home.button.update')}</button>
                            {#if checkSkinCompatibility(latestVersion, currentVersion) === false}
                                <span class="text-red-500">{$_('home.warn.skin_update')}</span>
                            {/if}
                        {:else}
                            <button type="button" onclick={LaunchOpenTaiko} class="button-blue button-main"><i class="fa-solid fa-rocket"></i> {$_('home.button.launch')}</button>
                            <button type="button" onclick={DownloadStable} class="button-gray button-main"><i class="fa-solid fa-download"></i> {$_('home.button.redownload')}</button>
                        {/if}
                    {/if}
                </div>
            </div>

            <div class="p-4 space-y-4">
                <div class="flex gap-4 flex-wrap items-center">
                    {#if isIndev}
                        <span class="nowrap"><b>{$_('home.label.latest_experimental')}</b></span>
                        {#if indevLatestSha === null}
                            <div class="placeholder animate-pulse flex-1"></div>
                        {:else}
                            <span>{current.experimental?.label} @{indevLatestSha.slice(0, 7)}</span>
                            {#if indevUpdateAvailable}
                                <span class="text-yellow-500">{$_('home.experimental.update_available')}</span>
                            {:else}
                                <span class="opacity-70">{$_('home.experimental.up_to_date')}</span>
                            {/if}
                            <button type="button" onclick={CheckIndevUpdate} class="button-blue button-main"><i class="fa-solid fa-rotate"></i> {$_('common.reload')}</button>
                        {/if}
                    {:else}
                        <span class="nowrap"><b>{$_('home.label.latest_version')}</b></span>
                        {#if latestVersionErrorFound === true}
                            <span class="fetch-error"><b>{$_('common.fetch_error')}</b></span>
                            <button type="button" onclick={TryFetchingLatestVersion} class="button-red button-main"><i class="fa-solid fa-triangle-exclamation"></i> {$_('home.button.retry')}</button>
                        {:else if latestLoading}
                            <div class="placeholder animate-pulse flex-1"></div>
                        {:else}
                            <span>{latestVersion}</span>
                            <button type="button" onclick={TryFetchingLatestVersion} class="button-blue button-main"><i class="fa-solid fa-rotate"></i> {$_('common.reload')}</button>
                        {/if}
                    {/if}
                </div>
            </div>

            {#if downloadBusy && buildLog.length > 0}
                <div class="p-4">
                    <pre class="build-log" bind:this={buildLogElement}>{buildLog.join('\n')}</pre>
                </div>
            {/if}

            <hr>

            <div class="p-4 space-y-4">
                <div class="flex gap-4">
                    <p>
                        {#if current.channel === 'experimental'}
                            <b>{$_('home.hint.experimental')}</b>
                        {:else}
                            <b>{$_('home.hint.first_start')}</b>
                        {/if}
                        <br><b>{$_('home.label.current_os')}</b> {optk_OS}
                    </p>
                </div>
            </div>
        {/if}
    </section>

    <HubVersionCheck />
</div>

<!-- Skins / Characters / Puchicharas -->
{#if assetsAvailable}
<div class="w-full h-full" style:display={homeSubTab >= 1 && homeSubTab <= 3 ? null : 'none'}>
    <!-- Only sub-tabs 1..3 map to an asset type; any other value (Docs/Saves active
         while this stays mounted) must never reach AssetsTab as an out-of-range index -->
    <AssetsTab ShowTabs={false} currentAsset={homeSubTab >= 1 && homeSubTab <= 3 ? homeSubTab - 1 : 0} />
</div>
{/if}

<!-- Documentation of the selected instance (mounted only when viewed: the inlined docs
     can be large, so it is not kept alive in the background) -->
{#if docsAvailable && homeSubTab === 4}
<div class="w-full h-full">
    <DocsTab />
</div>
{/if}

<!-- Save import / export -->
{#if savesAvailable && homeSubTab === 5}
    <SavesTab />
{/if}

<BuildPickerModal Show={showBuildPicker} OnClose={() => showBuildPicker = false} OnPick={OnBuildPicked} />

<style>
    .nowrap {white-space: nowrap;}

    .progressbar {
        margin-top: 9px;
        width: 100%;
    }

    .build-log {
        max-height: 14rem;
        overflow-y: auto;
        font-size: 0.72rem;
        line-height: 1.25;
        padding: 0.75rem;
        border-radius: 0.4rem;
        background: rgba(0, 0, 0, 0.35);
        white-space: pre-wrap;
        word-break: break-all;
    }
</style>
