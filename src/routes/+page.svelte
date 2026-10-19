<script lang="ts">
    import { onMount } from 'svelte';
    import { Navigation } from '@skeletonlabs/skeleton-svelte';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    // Pages
    import HomeTab from '$lib/pages/HomeTab.svelte';
    import SongsTab from '$lib/pages/SongsTab.svelte';
    import InformationTab from '$lib/pages/InformationTab.svelte';
    import ToolsTab from '$lib/pages/ToolsTab.svelte';
    import SecretTab from '$lib/pages/SecretTab.svelte';
    import LinksTab from '$lib/pages/LinksTab.svelte';
    import ThemesTab from '$lib/pages/ThemesTab.svelte';

    // Components
    import InstanceBar from '$lib/components/InstanceBar.svelte';
    import InstanceManagerModal from '$lib/components/InstanceManagerModal.svelte';

    // Instances
    import {
        instances,
        instancesReady,
        loadInstances,
        refreshInstanceVersion,
        linkGlobalSongs
    } from '$lib/stores/instances';
    import { busyAreas, type ActivityArea } from '$lib/stores/activity';
    import { ipc } from '$lib/ipc';
    import { toaster } from '$lib/utils/toaster';

    // Operations a previous load of the page started (a reload, a crashed renderer) have
    // nobody left to show or end them. Here rather than in onMount, which runs after the
    // tabs mounted and may already have started one of their own.
    ipc.resetJobs().catch((error) => console.warn('Could not reset operations:', error));

    // Themes
    const themetarget = document.getElementById('themetarget');

    interface NavTile {
        value: number;
        icon: string;
        label: string;
        tooltip: string;
    }

    // Tiles whose page runs work in the background (installs, bulk downloads) show a
    // spinner, so leaving the page does not make that work look finished
    const tileArea: Partial<Record<number, ActivityArea>> = { 0: 'home', 1: 'songs' };

    // Navigation (tile ids are stable: ThemesTab compares against 7)
    let currentTile = $state(0);
    let showInstanceManager = $state(false);

    const mainTiles: NavTile[] = [
        { value: 0, icon: 'fa-solid fa-home', label: 'nav.home', tooltip: 'nav.tooltip.home' },
        { value: 1, icon: 'fa-solid fa-music', label: 'nav.songlist', tooltip: 'nav.tooltip.songlist' },
        { value: 3, icon: 'fa-solid fa-screwdriver-wrench', label: 'nav.tools', tooltip: 'nav.tooltip.tools' },
        { value: 4, icon: 'fa-solid fa-question', label: 'nav.secrets', tooltip: 'nav.tooltip.secrets' }
    ];
    const trailTiles: NavTile[] = [
        { value: 5, icon: 'fa-regular fa-file-lines', label: 'nav.information', tooltip: 'nav.tooltip.information' },
        { value: 6, icon: 'fa-solid fa-globe', label: 'nav.links', tooltip: 'nav.tooltip.links' },
        { value: 7, icon: 'fa-solid fa-gear', label: 'nav.themes', tooltip: 'nav.tooltip.themes' }
    ];

    // The Hub moved its data into a folder of its own at startup (Linux and macOS, from
    // Documents). Said once, so the songs and games are not simply gone from where the
    // user knew them; and a move that failed is said every start until it is sorted out,
    // since the library looks empty meanwhile.
    const ReportRelocation = async () => {
        const report = await ipc.takeRelocationReport().catch(() => null);
        if (!report) return;
        const t = get(_);
        const folder = report.newRoot;
        if (report.moved.length > 0) {
            toaster.info({ description: t('relocate.moved', { values: { count: report.moved.length, folder } }), duration: 20000 });
        }
        if (report.problems.length > 0) {
            toaster.error({
                description: t('relocate.failed', { values: { folder } }),
                duration: Infinity,
                meta: { details: report.problems.join('\n') }
            });
        }
    };

    onMount(async () => {
        try {
            await loadInstances();

            // Detect versions and (re-)link the shared Songs library for every instance
            for (const inst of get(instances)) {
                refreshInstanceVersion(inst.id);
                linkGlobalSongs(inst);
            }
            await ReportRelocation();
        } catch (error) {
            console.error('Instance initialization failed:', error);
        } finally {
            // The body starts hidden (app.html) to avoid a theme flash; always unhide
            themetarget?.setAttribute("style", "");
        }
    });
</script>

<div class="grid h-screen grid-rows-[auto_1fr_auto]">
    <!-- Grid Columns -->
    <div class="grid grid-cols-1 md:grid-cols-[auto_1fr]">
      <!-- Left Sidebar. -->
      <aside>
        <!-- Fixed 80px rail (HubVersionCheck positions its version chip next to it) -->
        <Navigation layout="rail" class="h-screen w-20">
            <Navigation.Content>
                <Navigation.Menu>
                    {#each mainTiles as tile (tile.value)}
                        <Navigation.Trigger
                            title={$_(tile.tooltip)}
                            data-active={currentTile === tile.value ? '' : undefined}
                            onclick={() => currentTile = tile.value}
                        >
                            <span class="tile-icon">
                                <i class={tile.icon}></i>
                                {#if tileArea[tile.value] && $busyAreas.has(tileArea[tile.value]!)}
                                    <i class="fa-solid fa-spinner fa-spin tile-busy" aria-label={$_('jobs.busy')}></i>
                                {/if}
                            </span>
                            <Navigation.TriggerText>{$_(tile.label)}</Navigation.TriggerText>
                        </Navigation.Trigger>
                    {/each}
                </Navigation.Menu>
            </Navigation.Content>
            <!-- Trail -->
            <Navigation.Footer class="flex flex-col items-stretch gap-1">
                {#each trailTiles as tile (tile.value)}
                    <Navigation.Trigger
                        title={$_(tile.tooltip)}
                        data-active={currentTile === tile.value ? '' : undefined}
                        onclick={() => currentTile = tile.value}
                    >
                        <i class={tile.icon}></i>
                        <Navigation.TriggerText>{$_(tile.label)}</Navigation.TriggerText>
                    </Navigation.Trigger>
                {/each}
                <Navigation.TriggerAnchor href="https://github.com/OpenTaiko/OpenTaiko-Hub" target="_blank" title={$_('nav.tooltip.github')} class="sidebaricon">
                    <i class="fa-brands fa-github text-2xl"></i>
                </Navigation.TriggerAnchor>
            </Navigation.Footer>
        </Navigation>
      </aside>
      <!-- Main Content -->
      <main class="h-screen space-y-4 p-4">
        <!-- OpenTaiko instance management (Home only: the songs library is global) -->
        <div class="w-full h-full space-y-4" style:display={currentTile === 0 ? null : 'none'}>
            {#if $instancesReady}
                <InstanceBar OnManage={() => showInstanceManager = true} />
                <HomeTab />
            {/if}
        </div>

        <!-- Songs -->
        <div class="w-full h-full" style:display={currentTile === 1 ? null : 'none'}>
            {#if $instancesReady}
            <SongsTab />
            {/if}
        </div>

        <!-- Tools -->
        {#if currentTile === 3}
            <ToolsTab />
        {/if}

        <!-- Secrets -->
        {#if currentTile === 4}
            <SecretTab />
        {/if}

        <!-- Information -->
        {#if currentTile === 5}
            <InformationTab />
        {/if}

        <!-- Links -->
        {#if currentTile === 6}
            <LinksTab />
        {/if}

        <!-- Settings: theme, mode and language -->
        <ThemesTab {currentTile} />
      </main>
    </div>
</div>

<InstanceManagerModal Show={showInstanceManager} OnClose={() => showInstanceManager = false} />

<style>
    /* Main CSS */
    main {overflow-y: auto;}
    .tile-icon {position: relative; display: inline-block;}
    .tile-busy {position: absolute; top: -0.4rem; right: -0.9rem; font-size: 0.7rem;}
</style>
