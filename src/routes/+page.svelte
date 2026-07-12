<script>
    import { onMount } from 'svelte';
    import { AppRail, AppRailTile, AppRailAnchor } from '@skeletonlabs/skeleton';
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
    } from '$lib/stores/instances.js';

    // Themes
    const themetarget = document.getElementById('themetarget');

    // Navigation
    let currentTile = 0;
    let showInstanceManager = false;

    onMount(async () => {
        try {
            await loadInstances();

            // Detect versions and (re-)link the shared Songs library for every instance
            for (const inst of get(instances)) {
                refreshInstanceVersion(inst.id);
                linkGlobalSongs(inst);
            }
        } catch (error) {
            console.error('Instance initialization failed:', error);
        } finally {
            // The body starts hidden (app.html) to avoid a theme flash; always unhide
            themetarget.setAttribute("style", "");
        }
    });
</script>

<div class="grid h-screen grid-rows-[auto_1fr_auto]">
    <!-- Grid Columns -->
    <div class="grid grid-cols-1 md:grid-cols-[auto_1fr]">
      <!-- Left Sidebar. -->
      <aside class="bg-yellow-500">
        <AppRail height="h-full w-[72px]">
            <!-- <svelte:fragment slot="lead">
                <AppRailAnchor href="/" >(icon)</AppRailAnchor>
            </svelte:fragment> -->
            <!-- --- -->
            <AppRailTile bind:group={currentTile} name="tile-1" value={0} title={$_('nav.tooltip.home')}>
                <svelte:fragment slot="lead"><i class="fa-solid fa-home"></i></svelte:fragment>
                <span>{$_('nav.home')}</span>
            </AppRailTile>
            <AppRailTile bind:group={currentTile} name="tile-2" value={1} title={$_('nav.tooltip.songlist')}>
                <svelte:fragment slot="lead"><i class="fa-solid fa-music"></i></svelte:fragment>
                <span>{$_('nav.songlist')}</span>
            </AppRailTile>
            <AppRailTile bind:group={currentTile} name="tile-4" value={3} title={$_('nav.tooltip.tools')}>
                <svelte:fragment slot="lead"><i class="fa-solid fa-screwdriver-wrench"></i></svelte:fragment>
                <span>{$_('nav.tools')}</span>
            </AppRailTile>
            <AppRailTile bind:group={currentTile} name="tile-5" value={4} title={$_('nav.tooltip.secrets')}>
                <svelte:fragment slot="lead"><i class="fa-solid fa-question"></i></svelte:fragment>
                <span>{$_('nav.secrets')}</span>
            </AppRailTile>
            <!-- Trail -->
            <svelte:fragment slot="trail">
                <AppRailTile bind:group={currentTile} name="tile-6" value={5} title={$_('nav.tooltip.information')}>
                    <svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
                    <span>{$_('nav.information')}</span>
                </AppRailTile>
                <AppRailTile bind:group={currentTile} name="tile-7" value={6} title={$_('nav.tooltip.links')}>
                    <svelte:fragment slot="lead"><i class="fa-solid fa-globe"></i></svelte:fragment>
                    <span>{$_('nav.links')}</span>
                </AppRailTile>
                <AppRailTile bind:group={currentTile} name="tile-8" value={7} title={$_('nav.tooltip.themes')}>
                    <svelte:fragment slot="lead"><i class="fa-solid fa-palette"></i></svelte:fragment>
                    <span>{$_('nav.themes')}</span>
                </AppRailTile>
				<AppRailAnchor href="https://github.com/OpenTaiko/OpenTaiko-Hub" target="_blank" title={$_('nav.tooltip.github')} class="sidebaricon">
					<i class="fa-brands fa-github text-2xl text-black dark:text-white"></i>
				</AppRailAnchor>
			</svelte:fragment>
        </AppRail>
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

        <!-- OpTk Hub Themes -->
        <ThemesTab
            bind:currentTile
        />
      </main>
    </div>
</div>

<InstanceManagerModal Show={showInstanceManager} OnClose={() => showInstanceManager = false} />

<style>
    /* Main CSS */
    main {overflow-y: auto;}
</style>
