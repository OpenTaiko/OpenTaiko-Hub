<script lang="ts">
    import { onMount } from 'svelte';
    import { getVersion } from '@tauri-apps/api/app';
    import { fetchCachedJson } from '$lib/utils/download';
    import { openUrl } from '@tauri-apps/plugin-opener';

    import { getContext } from 'svelte';
    import type { ToastContext } from '$lib/types';
    const { TriggerError, TriggerWarning } = getContext<ToastContext>('toast');

    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import { compareVersions } from '$lib/utils/versions';

    // This Hub's version and the latest release's, without the leading "v"
    let currentVersion = $state<string | null>(null);
    let latestVersion = $state<string | null>(null);
    let latestVersionErrorFound = $state(false);

    // Compared as versions: a string comparison called a newer local build outdated and
    // warned about it on every launch
    let outdated = $derived(
        currentVersion !== null && latestVersion !== null && (compareVersions(latestVersion, currentVersion) ?? 0) > 0
    );

    const repoOwner = 'OpenTaiko';
    const repoName = 'OpenTaiko-Hub';

    const TryFetchingLatestVersion = async () => {
        latestVersionErrorFound = false;
        try {
            const data = await fetchCachedJson<{ tag_name: string }>(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`);
            latestVersion = data.tag_name.replace(/^v/i, '');
            if (outdated) TriggerWarning(get(_)('hub.warn.outdated_toast'));
        } catch (err) {
            // Shown on this card with a Retry button. No toast: offline, the Home page
            // already reports the same failed connection.
            console.error('Hub update check failed:', err);
            latestVersionErrorFound = true;
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
        // Read first, so the check compares against it
        try {
            currentVersion = await getVersion();
        } catch (error) {
            console.error('Could not read the Hub version:', error);
        }
        await TryFetchingLatestVersion();
    });
</script>


<section class="card hubupdatemessage">
    <div class="p-4 space-y-4">
        <div class="flex gap-4">
            {#if latestVersionErrorFound === true}
                <span>{$_('hub.status.fetch_failed')}</span>
                <span class="fetch-error"><b>{$_('common.fetch_error')}</b></span>
                <button type="button" onclick={TryFetchingLatestVersion} class="button-blue button-main"><i class="fa-solid fa-rotate"></i> {$_('common.retry')}</button>
            {:else if latestVersion === null}
                <span>{$_('hub.status.checking')}</span>
            {:else if outdated}
                <span><b>{$_('hub.status.outdated')}</b></span>
                <button type="button" onclick={UpdateHub} class="button-green button-main"><i class="fa-solid fa-download"></i> {$_('hub.button.update', { values: { version: latestVersion } })}</button>
            {:else}
                <span><b>{$_('hub.status.up_to_date')}</b></span>
            {/if}
        </div>
    </div>
</section>

<div class="card p-4 hubversionnumber">OpenTaiko Hub {#if currentVersion}(v{currentVersion}){/if}</div>

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
