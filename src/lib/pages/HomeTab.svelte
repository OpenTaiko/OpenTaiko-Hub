<script lang="ts">
    import { getContext } from 'svelte';
    import { Tabs } from '@skeletonlabs/skeleton-svelte';
    import ProgressBar from '$lib/components/ProgressBar.svelte';
    import { writeTextFile, mkdir, exists, remove } from '@tauri-apps/plugin-fs';
    import { path } from '@tauri-apps/api';
    import { invoke, Channel } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';
    import { onMount } from 'svelte';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import type { BuildOption, GitHubRelease, HubOS, IndevBuildInfo, Instance, ToastContext } from '$lib/types';
    const { TriggerError, TriggerWarning, TriggerSuccess, backoffDownload } = getContext<ToastContext>('toast');

    import { GetOS, GetTmpPath, GetBuildPath } from "$lib/utils/path";
    import { startJob, isCancelled, type Job } from "$lib/stores/activity";
    import { DescribeError, ErrorText } from "$lib/utils/errors";
    import { openUrl } from '@tauri-apps/plugin-opener';
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

    type BuildStage = 'check_sdk' | 'fetch' | 'download' | 'extract' | 'build' | 'install';

    /** How much compiler output is kept on screen and written to the log on a failure. */
    const BUILD_LOG_LINES = 200;

    /**
     * Digits and a unit only, so it needs no translation. Carries a KB step because this
     * is what tells a slow transfer apart from a stuck one, and a connection under
     * ~50 KB/s — exactly the case worth reporting — would otherwise read "0.0 MB/s".
     */
    const formatBytes = (n: number): string => {
        if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(2)} GB`;
        if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(1)} MB`;
        return `${Math.round(n / 1024)} KB`;
    };

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
    const DownloadAndInstallZip = async (url: string, instancePath: string, job: Job): Promise<boolean> => {
        const tmpDir = await GetTmpPath(crypto.randomUUID());
        await mkdir(tmpDir, { recursive: true });
        const zipPath = await path.join(tmpDir, 'OpenTaiko.zip');

        try {
            const success = await backoffDownload(
                url,
                zipPath,
                (pr) => {
                    // progressTotal rather than a running sum of chunk sizes: a retry
                    // restarts the transfer, and a sum kept across attempts would count
                    // the re-downloaded bytes twice and drive the bar past 100%.
                    progress = pr.total ? 100 * (pr.progressTotal / pr.total) : undefined;
                },
                undefined,
                job.id
            );
            if (!success) {
                return false;
            }

            TriggerSuccess(get(_)('home.success.unzipping'));
            progress = undefined;

            const extractProgress = new Channel<number>();
            extractProgress.onmessage = (percent) => progress = percent;
            const sourceFolder = await invoke<string>('unzip_and_get_first_folder', { zipPath, job: job.id, onProgress: extractProgress });

            TriggerSuccess(get(_)('home.success.installing'));
            progress = undefined;
            cancellable = false;

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
            TriggerError(get(_)('home.error.already_downloading'));
            return null;
        }
        // Claimed before the first await, so a second click cannot start a second install
        downloadBusy = true;
        busyInstance = instance;
        busyJob = startJob('home', instance.id);
        cancellable = true;
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

    const CancelInstall = () => {
        if (cancellable) busyJob?.cancel();
    };

    // A failed install says why in plain words when the cause is known, and keeps the raw
    // error for the copy button; a cancelled one is not an error at all
    const ReportInstallError = (key: string, err: unknown) => {
        if (isCancelled(err)) {
            TriggerWarning(get(_)('jobs.install_cancelled'));
            return;
        }
        TriggerError(get(_)(key, { values: { error: DescribeError(err) } }), { details: ErrorText(err) });
    };

    // Stable and prerelease installs only differ in which release they read and how the
    // instance is recorded afterwards, so both go through here.
    const InstallRelease = async (resolve: () => Promise<ReleaseToInstall>) => {
        if (!current) return;
        // Capture the target so switching instances mid-download cannot retarget it
        const instance = current;
        const job = BeginInstall(instance);
        if (!job) return;

        try {
            const release = await resolve();
            progress = 0;

            const installed = await DownloadAndInstallZip(release.url, instance.path, job);
            if (installed) {
                await WriteVersionJson(instance.path, release.version);
                const updated = await updateInstance(instance.id, release.patch);
                await linkGlobalSongs(updated);
                await refreshInstanceVersion(instance.id);
                TriggerSuccess(get(_)('home.success.installed'));
            }
        } catch (err) {
            ReportInstallError('home.error.download_failed', err);
        } finally {
            EndInstall();
        }
    };

    const DownloadStable = () => InstallRelease(async () => {
        if (!latestRelease) await TryFetchingLatestVersion();
        const asset = latestRelease?.assets?.find((asset) => asset.name.endsWith(`${optk_OS}.x64.zip`));
        if (!asset) {
            throw new Error('Desired asset not found in the latest release');
        }
        return { url: asset.browser_download_url, version: latestVersion, patch: { channel: 'stable', experimental: null } };
    });

    const DownloadPrerelease = (option: { tag: string; label: string }) => InstallRelease(async () => {
        const release = await fetchReleaseByTag(option.tag);
        const asset = release.assets?.find((asset) => asset.name.endsWith(`${optk_OS}.x64.zip`));
        if (!asset) {
            throw new Error('Desired asset not found in the prerelease');
        }
        return {
            url: asset.browser_download_url,
            version: option.tag,
            patch: { channel: 'experimental', experimental: { kind: 'prerelease', tag: option.tag, label: option.label } }
        };
    });

    // Fetches the experimental branch source at its current head commit, builds it with
    // the .NET SDK and installs the publish output into the instance folder.
    const DownloadIndev = async (option: { branch?: string; label?: string }) => {
        if (!current) return;
        // Capture the target so switching instances mid-download cannot retarget it
        const instance = current;
        const job = BeginInstall(instance);
        if (!job) return;
        let tmpDir: string | null = null;

        // MSBuild emits output far faster than the UI can paint it. Rebuilding a
        // 200-element array per line, each rebuild waking the auto-scroll effect, is
        // enough to stall the webview for tens of seconds on a full build, so lines are
        // collected and folded in on a timer. A timer and not an animation frame, because
        // animation frames stop firing while the window is hidden and a build running in
        // the background would then display nothing at all.
        //
        // Declared out here rather than beside the build step so the failure path can
        // drain whatever is still buffered: the last lines are the ones that say why.
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
            // 1. The build requires the .NET SDK 8
            buildStage = 'check_sdk';
            progress = undefined;
            const sdkLines: string[] = [];
            // Output arrives in batches of lines
            const sdkChannel = new Channel<string[]>();
            sdkChannel.onmessage = (lines) => sdkLines.push(...lines);
            let sdkExitCode = -1;
            try {
                sdkExitCode = await invoke<number>('run_dotnet', {
                    args: ['--list-sdks'],
                    cwd: null,
                    job: job.id,
                    onOutput: sdkChannel
                });
            } catch (error) {
                if (isCancelled(error)) throw error;
                sdkExitCode = -1;
            }
            if (sdkExitCode !== 0 || !sdkLines.some((line) => line.trim().startsWith('8.'))) {
                TriggerError(get(_)('home.experimental.requires_dotnet'), {
                    action: {
                        label: get(_)('jobs.dotnet_page'),
                        run: () => { openUrl('https://dotnet.microsoft.com/download/dotnet/8.0').catch(() => {}); }
                    }
                });
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
            transferred = 0;
            transferRate = 0;
            tmpDir = await GetBuildPath();
            await mkdir(tmpDir, { recursive: true });
            const srcZip = await path.join(tmpDir, 'src.zip');
            const success = await backoffDownload(
                `https://codeload.github.com/${repoOwner}/${repoName}/zip/${sha}`,
                srcZip,
                (pr) => {
                    // codeload builds the archive on the fly and streams it chunked, so it
                    // sends no Content-Length and `total` is 0. A percentage is impossible;
                    // bytes and rate are the only way to tell this stage from a hang.
                    // progressTotal is the plugin's own running count, so it also resets
                    // correctly when a retry restarts the transfer.
                    transferred = pr.progressTotal;
                    transferRate = pr.transferSpeed;
                    progress = pr.total ? 100 * (pr.progressTotal / pr.total) : undefined;
                },
                3,
                job.id
            );
            if (!success) {
                throw new Error('Source download failed');
            }

            // 4. Extract
            buildStage = 'extract';
            progress = 0;
            const extractProgress = new Channel<number>();
            extractProgress.onmessage = (percent) => progress = percent;
            const extracted = await invoke<string>('unzip_and_get_first_folder', { zipPath: srcZip, job: job.id, onProgress: extractProgress });
            await remove(srcZip);
            // The archive unpacks into `OpenTaiko-<40 character commit>`. Every character
            // of that name counts against the Windows path limit for the whole build, so
            // it is moved to a one-letter folder before the compiler sees it.
            const srcFolder = await path.join(tmpDir, 's');
            await invoke('merge_move_dir', { src: extracted, dest: srcFolder });

            // 5. Build (same command as the game's build scripts)
            buildStage = 'build';
            progress = undefined;
            const rid = optk_OS === 'Win' ? 'win-x64' : 'linux-x64';
            const logChannel = new Channel<string[]>();
            logChannel.onmessage = (lines) => {
                pending.push(...lines);
                // Bounded in case the flush is throttled while the window is hidden.
                if (pending.length > BUILD_LOG_LINES * 2) {
                    pending = pending.slice(-BUILD_LOG_LINES);
                }
                if (flushTimer === null) flushTimer = setTimeout(flushBuildLog, 100);
            };
            // Same command as the game's build scripts, except for where the output goes.
            // By default the game's files are copied to
            // OpenTaiko\bin\Release\net8.0\<rid>\publish, 45 characters deeper than the
            // source they came from, which alone pushes the skin folders past the Windows
            // path limit. Both output folders sit directly in the build folder instead.
            const publishDir = await path.join(tmpDir, 'p');
            const outDir = await path.join(tmpDir, 'b') + path.sep();
            const exitCode = await invoke<number>('run_dotnet', {
                args: ['publish', 'OpenTaiko/OpenTaiko.csproj', '--configuration', 'Release', '--self-contained', '-p:PublishSingleFile=true', '--runtime', rid, '--output', publishDir, `-p:OutDir=${outDir}`],
                cwd: srcFolder,
                job: job.id,
                onOutput: logChannel
            });
            flushBuildLog();
            if (exitCode !== 0) {
                throw new Error(`dotnet publish exited with code ${exitCode}`);
            }

            // 6. Install the publish output (and local docs when the branch ships them)
            buildStage = 'install';
            cancellable = false;
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
            // Whatever is still buffered belongs in the report, and a failure that came
            // from the invoke itself rather than a non-zero exit never reached the flush
            // after the build step.
            flushBuildLog();
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
            if (tmpDir) {
                try { await remove(tmpDir, { recursive: true }); } catch {}
            }
            EndInstall();
            buildStage = null;
        }
    };

    const LaunchOpenTaiko = async () => {
        if (!current) return;
        try {
            const appPath = await path.join(current.path, "OpenTaiko");
            await invoke('execute_external_app', { path: appPath });
        } catch (error) {
            TriggerError(get(_)('home.error.launch', { values: { error: String(error) } }));
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

    /** Toast messages render as HTML, so text coming from the game must not be markup. */
    const escapeHtml = (text: string) =>
        text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

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
            const detail = stderr ? escapeHtml(stderr.split('\n').slice(-3).join(' ')) : '';

            // A launch that collapses immediately and a crash mid-session are different
            // problems, so they are reported differently.
            if (playedASession) {
                TriggerError(get(_)('home.error.game_crashed', {
                    values: { mins: Math.max(1, Math.round(ranForSecs / 60)), reason, detail }
                }));
            } else {
                TriggerError(get(_)('home.error.game_exited', { values: { reason, detail } }));
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
                            <button type="button" onclick={CancelInstall} class="button-gray button-main"><i class="fa-solid fa-xmark"></i> {$_('common.cancel')}</button>
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
