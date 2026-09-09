<script lang="ts">
    import { onMount } from 'svelte';
    import { getVersion } from '@tauri-apps/api/app';
    import { fetch } from "@tauri-apps/plugin-http";
    import { openUrl } from '@tauri-apps/plugin-opener';

    import { getContext } from 'svelte';
    import type { ToastContext } from '$lib/types';
    const { TriggerError, TriggerWarning } = getContext<ToastContext>('toast');

    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    // Version Number
    let VersionHub = $state("Loading...")

    async function getHubVersion() {
        const appVersion = await getVersion();
        VersionHub = "v" + appVersion;
        console.log(VersionHub);
    }

    // Check version number
    const repoOwner = 'OpenTaiko';//'OpenTaiko';
    const repoName = 'OpenTaiko-Hub';//OpenTaiko-Dev-Mirror';
    let latestVersion = $state('Loading...');
    let latestVersionErrorFound = $state(false);


    const TryFetchingLatestVersion = async () => {
        try {
            latestVersionErrorFound = false;
            const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`);
            if (!response.ok) {
                throw new Error(`HTTP error status: ${response.status}`);
            }
            const data = (await response.json()) as { tag_name: string };
            latestVersion = data.tag_name; // Latest tag version number

            if (VersionHub == latestVersion) {
                console.log("Hub version is up to date!");
            }
            else if (VersionHub != latestVersion) {
                console.log("Hub version is out of date, or newer than the current version on GitHub.");
                TriggerWarning(get(_)('hub.warn.outdated_toast'));
            }
        } catch (err) {
            latestVersionErrorFound = true;
            TriggerError(get(_)('hub.error.fetch', { values: { error: String(err) } }));
        }
    }

    // window.open is a no-op inside the webview, the opener plugin has to be used to
    // hand the URL to the system browser
    const UpdateHub = async () => {
        try {
            await openUrl("https://github.com/OpenTaiko/OpenTaiko-Hub/releases/latest");
        } catch (err) {
            TriggerError(get(_)('hub.error.fetch', { values: { error: String(err) } }));
        }
    }

    onMount(async () => {
        getHubVersion()
        await TryFetchingLatestVersion();
    });
</script>


<section class="card hubupdatemessage">
    <div class="p-4 space-y-4">
        <div class="flex gap-4">
            {#if latestVersionErrorFound === true}
                <span>{$_('hub.status.fetch_failed')}</span>
                <span class="fetch-error"><b>{$_('common.fetch_error')}</b></span>
                <button type="button" onclick={TryFetchingLatestVersion} class="button-red button-main"><i class="fa-solid fa-triangle-exclamation"></i> {$_('common.retry')}</button>
            {:else if latestVersion === "Loading..."}
                <span>{$_('hub.status.checking')}</span>
            {:else if VersionHub != latestVersion}
                <span><b>{$_('hub.status.outdated')}</b></span>
                <button type="button" onclick={UpdateHub} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('hub.button.update', { values: { version: latestVersion.slice(1) } })}</button>
            {:else}
                <span><b>{$_('hub.status.up_to_date')}</b></span>
            {/if}
        </div>
    </div>
</section>

<div class="card p-4 hubversionnumber">OpenTaiko Hub ({VersionHub})</div>

<style>
    .hubversionnumber {
        font-weight: bold;
        position: fixed;
        bottom: 16px;
        left: 96px;   /* 80px navigation rail + 16px gap */
    }
    .hubupdatemessage {
        position: fixed;
        bottom: 16px;
        right: 16px;
    }
</style>
