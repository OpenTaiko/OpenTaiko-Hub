<script lang="ts">
    import { ipc } from '$lib/ipc';
    import { formatBytes } from '$lib/utils/format.svelte';
    import { InstallReleaseZip, IsInstallComplete, WriteVersionJson as WriteVersionFile } from '$lib/services/install';
    import { BuildAndInstall, MissingDotnetSdk, type BuildStage } from '$lib/services/indev';
    import { getContext } from 'svelte';
    import { Tabs } from '@skeletonlabs/skeleton-svelte';
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { listen } from '@tauri-apps/api/event';
    import { onMount } from 'svelte';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import type { BuildOption, GitHubRelease, HubOS, IndevBuildInfo, Instance, ToastContext } from '$lib/types';
    const { TriggerError, TriggerWarning, TriggerSuccess } = getContext<ToastContext>('toast');

    import { GetOS, GetBuildTarget, GetRootPath, OpenFolder } from "$lib/utils/path";
    import { startJob, isCancelled, InstanceInUse, busyInstanceIds, type Job } from "$lib/stores/activity";
    import { download } from '$lib/utils/download';
    import { DescribeError, ErrorText } from '$lib/utils/errors';
    import { openUrl } from '@tauri-apps/plugin-opener';
    import { confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
    import { isVersionAtLeast, isSkinCompatible } from "$lib/utils/versions";
    import {
        IndevVersion,
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
        linkGlobalSongs,
        createInstance
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
    // Whether a Skins / Characters / Puchicharas tab was opened yet (see the markup)
    let assetsOpened = $state(false);
    $effect(() => {
        if (homeSubTab >= 1 && homeSubTab <= 3) assetsOpened = true;
    });

    // Latest stable release
    let latestVersion = $state<string | null>(null);
    let latestRelease: GitHubRelease | null = null;
    let latestLoading = $state(true);
    let latestVersionErrorFound = $state(false);

    // Active instance state
    let versionLoading = $state(false);
    let versionLoadedFor: string | null = null;       // bookkeeping only, never rendered
    let indevLatestSha = $state<string | null>(null);

    // Download / build state. One install runs at a time, Hub-wide; busyInstance says
    // which instance it writes into, so the progress is shown under that instance only.
    let downloadBusy = $state(false);
    let busyInstance = $state<Instance | null>(null);
    let busyJob: Job | null = null;
    // False once the install starts writing into the instance folder: stopping halfway
    // through that would leave a mix of two builds
    let cancellable = $state(false);
    // undefined renders an indeterminate bar
    let progress = $state<number | undefined>(0);
    // Bytes received and the current rate, shown when the server sends no size and a
    // percentage is therefore impossible. Without them the bar looks frozen.
    let transferred = $state(0);
    let transferRate = $state(0);
    let buildStage = $state<BuildStage | null>(null);
    let buildLog = $state<string[]>([]);
    let buildLogElement = $state<HTMLPreElement | null>(null);
    // Keeps the build output on screen after a failure. Without it the panel is tied to
    // downloadBusy and vanishes at the exact moment it becomes worth reading.
    let buildFailed = $state(false);

    // The output belongs to one build of one instance. Anything that starts another run,
    // or switches to another instance, has to drop it: the panel is part of the shared
    // instance card, so a kept log would otherwise render under a different instance and
    // beneath an unrelated download, where it reads as that download's output.
    const resetBuildOutput = () => {
        buildLog = [];
        buildFailed = false;
    };

    /** How much compiler output is kept on screen and written to the log on a failure. */
    const BUILD_LOG_LINES = 200;


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

    // An instance created through the build picker carries its chosen build; start it.
    // Not while something else writes into that instance (skins being downloaded): the
    // install was then refused, and the chosen build forgotten.
    let pendingStartedFor: string | null = null;      // bookkeeping only, never rendered
    $effect(() => {
        if (current?.pendingInstall && !downloadBusy && !$busyInstanceIds.has(current.id) && pendingStartedFor !== current.id) {
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
        // Switching instances, or reloading, drops a finished build's output so it cannot
        // be read as belonging to the instance now on screen. A run still in flight keeps
        // its log, which is the one case where the output is still the live one.
        if (!downloadBusy) resetBuildOutput();
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

    const TryFetchingLatestVersion = async () => {
        try {
            latestVersionErrorFound = false;
            latestLoading = true;
            latestRelease = await fetchLatestRelease();
            latestVersion = latestRelease.tag_name; // Latest tag version number
        } catch (err) {
            latestVersionErrorFound = true;
            TriggerError(get(_)('home.error.fetch_release', { values: { error: DescribeError(err) } }), { details: ErrorText(err) });
        }
        latestLoading = false;
    }

    const StartPendingInstall = async (instance: Instance) => {
        const build = instance.pendingInstall;
        await updateInstance(instance.id, { pendingInstall: null });
        if (!build) return;
        // The instance is passed on: re-reading the selected one after the await above
        // installed into whichever instance the user had switched to meanwhile
        if (build.kind === 'stable') {
            await DownloadStable(instance);
        } else if (build.kind === 'indev') {
            await DownloadIndev(build, instance);
        } else if (build.kind === 'prerelease') {
            await DownloadPrerelease(build, instance);
        }
    };

    // First run: one button goes straight from nothing to an install, instead of sending
    // the user through Manage, Create and the picker. The picked build is installed into
    // a new instance as soon as it exists, as from the instance manager.
    let hubRoot = $state<string | null>(null);
    GetRootPath().then((root) => hubRoot = root).catch(() => {});

    const OnFirstInstall = async (build: BuildOption) => {
        try {
            const instance = await createInstance(null);
            await updateInstance(instance.id, {
                channel: build.kind === 'stable' ? 'stable' : 'experimental',
                pendingInstall: build
            });
        } catch (error) {
            TriggerError(get(_)('instances.error.create_failed', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
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

    const WriteVersionJson = (instancePath: string, version: string | null) => WriteVersionFile(instancePath, version, optk_OS);

    const VerifyInstall = async (instancePath: string) => {
        if (!(await IsInstallComplete(instancePath, optk_OS))) {
            throw new Error(get(_)('home.error.incomplete_build'));
        }
    };

    // Downloads a release zip and merges its content into the instance folder
    const DownloadAndInstallZip = async (url: string, instancePath: string, job: Job): Promise<void> => {
        await InstallReleaseZip(url, instancePath, job.id, {
            onProgress: (percent) => progress = percent,
            onStage: (stage) => {
                if (stage === 'extract') TriggerSuccess(get(_)('home.success.unzipping'));
                if (stage === 'install') {
                    TriggerSuccess(get(_)('home.success.installing'));
                    cancellable = false;
                }
            },
            // progressTotal rather than a running sum of chunk sizes: a retry restarts
            // the transfer, and a sum kept across attempts would count the re-downloaded
            // bytes twice and drive the bar past 100%.
            // A failure throws, and the install reports it once (it used to be announced
            // here and then again by the install)
            download: (zipUrl, dest, onProgress) => download(
                zipUrl,
                dest,
                (pr) => onProgress(pr.total ? 100 * (pr.progressTotal / pr.total) : undefined),
                undefined,
                job.id
            )
        });
        await VerifyInstall(instancePath);
    };

    // What a release install needs to know, found once the busy flag is held
    interface ReleaseToInstall {
        url: string;
        /** Recorded in version.json. */
        version: string | null;
        /** What the instance record becomes once the install succeeded. */
        patch: Partial<Instance>;
    }

    // Claims the Hub-wide install slot for `instance`. Returns null when it is taken.
    const BeginInstall = (instance: Instance): Job | null => {
        if (downloadBusy === true) {
            TriggerWarning(get(_)('home.error.already_downloading'));
            return null;
        }
        if (InstanceInUse(instance.id, true)) {
            TriggerWarning(get(_)('home.error.instance_busy'));
            return null;
        }
        // Claimed before the first await, so a second click cannot start a second install
        downloadBusy = true;
        busyInstance = instance;
        busyJob = startJob('home', instance.id, true);
        cancellable = true;
        cancelling = false;
        progress = undefined;
        // A previous experimental build's output must not sit under this download.
        resetBuildOutput();
        return busyJob;
    };

    const EndInstall = () => {
        busyJob?.end();
        busyJob = null;
        busyInstance = null;
        cancellable = false;
        downloadBusy = false;
    };

    // The work stops when the backend next checks, which can take a moment: the button
    // says so instead of looking as if the click did nothing
    let cancelling = $state(false);
    const CancelInstall = () => {
        if (!cancellable || cancelling) return;
        cancelling = true;
        busyJob?.cancel();
    };

    const OpenLogsAction = () => ({
        label: get(_)('toast.open_logs'),
        run: () => { ipc.logDirectory().then((dir) => { if (dir) OpenFolder(dir); }).catch(() => {}); }
    });

    // A failed install says why in plain words when the cause is known, and keeps the raw
    // error for the copy button; a cancelled one is not an error at all
    const ReportInstallError = (key: string, err: unknown) => {
        if (isCancelled(err)) {
            TriggerWarning(get(_)('jobs.install_cancelled'));
            return;
        }
        // Nothing failed: the release simply has no build for this system
        if (err instanceof NoBuildForSystem) {
            TriggerError(err.message);
            return;
        }
        // A build writes its whole output to the log, which says more than any message
        TriggerError(get(_)(key, { values: { error: DescribeError(err) } }), {
            details: ErrorText(err),
            action: key === 'home.error.build_failed' ? OpenLogsAction() : undefined
        });
    };

    // Stable and prerelease installs only differ in which release they read and how the
    // instance is recorded afterwards, so both go through here.
    const InstallRelease = async (resolve: () => Promise<ReleaseToInstall | null>, target?: Instance) => {
        // Capture the target so switching instances mid-download cannot retarget it
        const instance = target ?? current;
        if (!instance) return;
        const job = BeginInstall(instance);
        if (!job) return;

        try {
            const release = await resolve();
            if (!release) return;
            progress = 0;

            await DownloadAndInstallZip(release.url, instance.path, job);
            await WriteVersionJson(instance.path, release.version);
            const updated = await updateInstance(instance.id, release.patch);
            await linkGlobalSongs(updated);
            await refreshInstanceVersion(instance.id);
            TriggerSuccess(get(_)('home.success.installed'));
        } catch (err) {
            ReportInstallError('home.error.download_failed', err);
        } finally {
            EndInstall();
        }
    };

    // The release's download for this system, or an error saying there is none
    // Releases are published for Windows and Linux x64 only (no macOS builds yet), so a
    // Mac or an ARM machine finds nothing to download. Experimental InDev builds are
    // compiled from source for the machine itself and are not affected.
    class NoBuildForSystem extends Error {}

    const FindAsset = async (release: GitHubRelease) => {
        const target = await GetBuildTarget();
        const asset = release.assets?.find((asset) => asset.name.endsWith(target.assetSuffix));
        if (!asset) {
            throw new NoBuildForSystem(get(_)('errors.no_build_for_system', { values: { system: target.label } }));
        }
        return asset;
    };

    const DownloadStable = (target?: Instance) => InstallRelease(async () => {
        if (!latestRelease) await TryFetchingLatestVersion();
        // The lookup has already said why it failed
        if (!latestRelease) return null;
        const asset = await FindAsset(latestRelease);
        return { url: asset.browser_download_url, version: latestVersion, patch: { channel: 'stable', experimental: null } };
    }, target);

    // An update that breaks the installed skins, and a reinstall of the same version,
    // used to start on a single click; the first was only flagged by red text beside it
    const UpdateStable = async () => {
        if (isSkinCompatible(latestVersion, currentVersion) === false) {
            const go = await confirmDialog(get(_)('home.warn.skin_update_confirm'), { title: get(_)('home.button.update'), kind: 'warning' });
            if (!go) return;
        }
        await DownloadStable();
    };

    // Started offline, the Hub does not know the latest release and offers Redownload for
    // whatever the instance has; it then installed the latest release, a newer one perhaps,
    // with no skin warning. That release is looked up first, and when it is a newer one
    // nothing is installed: the overview now offers Update, with its warning.
    const RedownloadStable = async () => {
        if (!latestRelease) await TryFetchingLatestVersion();
        // The lookup has already said why it failed
        if (!latestRelease || latestVersion !== currentVersion) return;
        const go = await confirmDialog(get(_)('home.warn.redownload_confirm', { values: { version: latestVersion ?? '' } }), { title: get(_)('home.button.redownload') });
        if (go) await DownloadStable();
    };

    const DownloadPrerelease = (option: { tag: string; label: string }, target?: Instance) => InstallRelease(async () => {
        const asset = await FindAsset(await fetchReleaseByTag(option.tag));
        return {
            url: asset.browser_download_url,
            version: option.tag,
            patch: { channel: 'experimental', experimental: { kind: 'prerelease', tag: option.tag, label: option.label } }
        };
    }, target);

    // Builds the experimental branch from source and installs it into the instance folder
    // (services/indev.ts), showing its progress and its compiler output here.
    const DownloadIndev = async (option: { branch?: string; label?: string }, target?: Instance) => {
        // Capture the target so switching instances mid-download cannot retarget it
        const instance = target ?? current;
        if (!instance) return;
        const job = BeginInstall(instance);
        if (!job) return;
        const branch = option.branch ?? INDEV_BRANCH;

        // MSBuild emits output far faster than the UI can paint it. Rebuilding a
        // 200-element array per line, each rebuild waking the auto-scroll effect, is
        // enough to stall the webview for tens of seconds on a full build, so lines are
        // collected and folded in on a timer. A timer and not an animation frame, because
        // animation frames stop firing while the window is hidden and a build running in
        // the background would then display nothing at all.
        let pending: string[] = [];
        let flushTimer: ReturnType<typeof setTimeout> | null = null;
        const flushBuildLog = () => {
            if (flushTimer !== null) {
                clearTimeout(flushTimer);
                flushTimer = null;
            }
            if (pending.length === 0) return;
            buildLog = [...buildLog, ...pending].slice(-BUILD_LOG_LINES);
            pending = [];
        };

        try {
            const sha = await BuildAndInstall(instance.path, branch, optk_OS, job.id, {
                onStage: (stage) => {
                    buildStage = stage;
                    // The files are being moved into the instance: stopping now would
                    // leave it half old, half new
                    if (stage === 'install') {
                        cancellable = false;
                        flushBuildLog();
                    }
                },
                onProgress: (percent) => progress = percent,
                onTransfer: (bytes, perSecond) => {
                    transferred = bytes;
                    transferRate = perSecond;
                },
                onOutput: (lines) => {
                    pending.push(...lines);
                    // Bounded in case the flush is throttled while the window is hidden.
                    if (pending.length > BUILD_LOG_LINES * 2) {
                        pending = pending.slice(-BUILD_LOG_LINES);
                    }
                    if (flushTimer === null) flushTimer = setTimeout(flushBuildLog, 100);
                }
            });

            // Record build metadata for update checks and link the shared library
            await WriteVersionJson(instance.path, IndevVersion(branch));
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
            // Whatever is still buffered belongs in the report: the last lines are the
            // ones that say why
            flushBuildLog();
            if (err instanceof MissingDotnetSdk) {
                TriggerError(get(_)('home.experimental.requires_dotnet'), {
                    action: {
                        label: get(_)('jobs.dotnet_page'),
                        run: () => { openUrl('https://dotnet.microsoft.com/download/dotnet/8.0').catch(() => {}); }
                    }
                });
                return;
            }
            // Getting this far can take half an hour, and the compiler output is the only
            // record of why it failed. It lives in component state that the next run
            // clears, so write it to the log file first: console.error is mirrored there
            // by the root layout. Without this the user is left with an exit code.
            if (buildLog.length > 0 && !isCancelled(err)) {
                buildFailed = true;
                console.error(
                    `Experimental build failed (${String(err)}). Last ${buildLog.length} lines of build output:\n`
                    + buildLog.join('\n')
                );
            }
            // The stage that threw decides the wording, and buildStage is not cleared
            // until the finally below. An install that fails after a good build is not a
            // build failure, and saying so would send the user to the wrong output.
            ReportInstallError(buildStage === 'build' ? 'home.error.build_failed' : 'home.error.download_failed', err);
        } finally {
            // No-op on the paths above that already flushed; it is here so no timer can
            // outlive the run and write into the log of the next one.
            flushBuildLog();
            EndInstall();
            buildStage = null;
        }
    };

    // The game takes a moment to show its window, and a second click in that time started
    // a second copy. The button shows the launch and ignores clicks until it is under way.
    let launching = $state(false);
    const LAUNCH_SETTLE_MS = 3000;

    const LaunchOpenTaiko = async () => {
        if (!current || launching) return;
        launching = true;
        try {
            await ipc.launchGame(current.path);
            setTimeout(() => launching = false, LAUNCH_SETTLE_MS);
        } catch (error) {
            launching = false;
            TriggerError(get(_)('home.error.launch', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    }

    // The backend watches a launched game and reports how it ended. A game that dies
    // immediately used to fail silently, so users blamed the Hub for its crash.
    interface GameExit {
        code: number | null;
        /** Set instead of `code` when the process was killed by a signal (Unix). */
        signal: number | null;
        ranForSecs: number;
        stderr: string;
    }

    onMount(() => {
        const pending = listen<GameExit>('game-exited', (event) => {
            const { code, signal, ranForSecs, stderr } = event.payload;
            // Exit code 0 is a successful quit no matter how briefly it ran; a game that
            // exits immediately because it is already running says nothing is wrong.
            // A signal death carries no exit code, and is how many games end at teardown
            // on Linux, but only counts as a normal quit once a real session happened —
            // otherwise a crash on startup would pass silently.
            const playedASession = ranForSecs >= 5;
            if (code === 0 || (playedASession && code === null)) return;

            const reason = signal !== null && signal !== undefined
                ? get(_)('home.error.game_signal', { values: { signal } })
                : get(_)('home.error.game_code', { values: { code: code ?? '?' } });
            // What the game printed goes to the copied details and the log rather than
            // into the message, where it was unreadable in a toast
            const report = { details: stderr || reason, action: OpenLogsAction() };

            // A launch that collapses immediately and a crash mid-session are different
            // problems, so they are reported differently.
            if (playedASession) {
                TriggerError(get(_)('home.error.game_crashed', {
                    values: { mins: Math.max(1, Math.round(ranForSecs / 60)), reason, detail: '' }
                }), report);
            } else {
                TriggerError(get(_)('home.error.game_exited', { values: { reason, detail: '' } }), report);
            }
        });
        return () => { pending.then((unlisten) => unlisten()); };
    });

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
    <!-- The file is twice this height, for high-DPI screens -->
    <img src={optkLogoUrl} alt="OpenTaiko" class="mx-auto h-32 w-auto" />

    <section class="card w-full">
        {#if !current}
            <div class="p-4 space-y-3 text-center">
                <p><b>{$_('home.first_run_text')}</b></p>
                <button type="button" class="button-green button-main" onclick={() => showBuildPicker = true}>
                    <i class="fa-solid fa-download"></i> {$_('home.first_run_install')}
                </button>
                {#if hubRoot}
                    <p class="text-sm opacity-70">{$_('home.first_run_folder', { values: { folder: hubRoot } })}</p>
                {/if}
            </div>
        {:else}
            <div class="p-4 space-y-4">
                <div class="flex gap-4 flex-wrap items-center">
                    <span class="nowrap"><b>{$_('home.label.current_version')}</b></span>
                    {#if versionLoading}
                        <div class="placeholder animate-pulse flex-1"></div>
                    {:else if downloadBusy === true && busyInstance?.id === current.id}
                        <div class="progressbar">
                            {#if buildStage}
                                <p class="text-sm mb-1">
                                    <i class="fa-solid fa-flask"></i> {$_(`home.experimental.stage.${buildStage}`)}
                                    {#if buildStage === 'download' && transferred > 0}
                                        <span class="opacity-70">&middot; {formatBytes(transferred)}{#if transferRate > 0}&nbsp;&middot;&nbsp;{formatBytes(transferRate)}/s{/if}</span>
                                    {/if}
                                </p>
                            {/if}
                            <ProgressBar value={progress} max={100} />
                        </div>
                        {#if cancellable}
                            <button type="button" onclick={CancelInstall} class="button-gray button-main" disabled={cancelling}>
                                <i class="fa-solid {cancelling ? 'fa-spinner fa-spin' : 'fa-xmark'}"></i> {cancelling ? $_('jobs.cancelling') : $_('common.cancel')}
                            </button>
                        {/if}
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
                            <button type="button" onclick={LaunchOpenTaiko} disabled={launching} class="button-blue button-main"><i class="fa-solid {launching ? 'fa-spinner fa-spin' : 'fa-rocket'}"></i> {$_('home.button.launch')}</button>
                            {#if indevUpdateAvailable}
                                <button type="button" onclick={() => DownloadIndev(indevBuild ?? {})} class="button-green button-main"><i class="fa-solid fa-rotate"></i> {$_('home.button.update')}</button>
                            {:else}
                                <button type="button" onclick={() => DownloadIndev(indevBuild ?? {})} class="button-gray button-main"><i class="fa-solid fa-hammer"></i> {$_('home.button.rebuild')}</button>
                            {/if}
                        {:else if isPrerelease}
                            <button type="button" onclick={LaunchOpenTaiko} disabled={launching} class="button-blue button-main"><i class="fa-solid {launching ? 'fa-spinner fa-spin' : 'fa-rocket'}"></i> {$_('home.button.launch')}</button>
                            <!-- Opens the build picker, so it is named for that rather than "Redownload" -->
                            <button type="button" onclick={() => showBuildPicker = true} class="button-gray button-main"><i class="fa-solid fa-code-branch"></i> {$_('home.button.change_build')}</button>
                        {:else if latestVersion !== null && latestVersion !== currentVersion}
                            <button type="button" onclick={LaunchOpenTaiko} disabled={launching} class="button-blue button-main"><i class="fa-solid {launching ? 'fa-spinner fa-spin' : 'fa-rocket'}"></i> {$_('home.button.launch')}</button>
                            <button type="button" onclick={UpdateStable} class="button-green button-main"><i class="fa-solid fa-rotate"></i> {$_('home.button.update')}</button>
                            {#if isSkinCompatible(latestVersion, currentVersion) === false}
                                <span class="text-red-500">{$_('home.warn.skin_update')}</span>
                            {/if}
                        {:else}
                            <button type="button" onclick={LaunchOpenTaiko} disabled={launching} class="button-blue button-main"><i class="fa-solid {launching ? 'fa-spinner fa-spin' : 'fa-rocket'}"></i> {$_('home.button.launch')}</button>
                            <button type="button" onclick={RedownloadStable} class="button-gray button-main"><i class="fa-solid fa-download"></i> {$_('home.button.redownload')}</button>
                        {/if}
                        {#if downloadBusy && busyInstance}
                            <!-- The install runs for another instance; say where its progress is -->
                            <span class="opacity-70 text-sm"><i class="fa-solid fa-spinner fa-spin"></i> {$_('jobs.installing_into', { values: { name: busyInstance.name } })}</span>
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
                                <span class="update-available">{$_('home.experimental.update_available')}</span>
                            {:else}
                                <span class="opacity-70">{$_('home.experimental.up_to_date')}</span>
                            {/if}
                            <button type="button" onclick={CheckIndevUpdate} class="button-blue button-main"><i class="fa-solid fa-rotate"></i> {$_('home.button.check_updates')}</button>
                        {/if}
                    {:else}
                        <span class="nowrap"><b>{$_('home.label.latest_version')}</b></span>
                        {#if latestVersionErrorFound === true}
                            <span class="fetch-error"><b>{$_('common.fetch_error')}</b></span>
                            <button type="button" onclick={TryFetchingLatestVersion} class="button-blue button-main"><i class="fa-solid fa-rotate"></i> {$_('home.button.retry')}</button>
                        {:else if latestLoading}
                            <div class="placeholder animate-pulse flex-1"></div>
                        {:else}
                            <span>{latestVersion}</span>
                            <button type="button" onclick={TryFetchingLatestVersion} class="button-blue button-main"><i class="fa-solid fa-rotate"></i> {$_('home.button.check_updates')}</button>
                        {/if}
                    {/if}
                </div>
            </div>

            {#if ((downloadBusy && busyInstance?.id === current.id) || buildFailed) && buildLog.length > 0}
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
<!-- Mounted the first time one of its tabs is opened, then kept, so its downloads go on
     while another tab is shown: mounted from the start, it fetched the catalog and
     scanned the instance at every launch, whether the tab was opened or not -->
{#if assetsAvailable && assetsOpened}
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

<BuildPickerModal Show={showBuildPicker} OnClose={() => showBuildPicker = false} OnPick={current ? OnBuildPicked : OnFirstInstall} />

<style>
    .update-available {color: light-dark(#a16207, #eab308);}
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
