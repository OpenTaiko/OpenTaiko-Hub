<script>
    // Dialog to choose which OpenTaiko build to install into an instance:
    // Stable (latest GitHub release) or one of the available experimental builds
    // (grayed out when none exist).
    import { _ } from 'svelte-i18n';
    import { fetchLatestRelease, fetchExperimentalOptions } from '$lib/utils/builds.js';

    export let Show = false;
    export let OnClose = () => {};
    export let OnPick = (_build) => {};

    let loading = false;
    let latestTag = null;
    let experimentalOptions = [];
    let selected = 'stable';   // 'stable' | index into experimentalOptions
    let loadedFor = false;

    $: if (Show && !loadedFor) {
        loadedFor = true;
        LoadOptions();
    }
    $: if (!Show) loadedFor = false;

    const LoadOptions = async () => {
        loading = true;
        latestTag = null;
        experimentalOptions = [];
        selected = 'stable';
        try {
            const release = await fetchLatestRelease();
            latestTag = release.tag_name;
        } catch (error) {
            console.error('Failed to fetch the latest release:', error);
        }
        experimentalOptions = await fetchExperimentalOptions(latestTag);
        loading = false;
    };

    const Confirm = () => {
        if (selected === 'stable') {
            OnPick({ kind: 'stable' });
        } else {
            OnPick(experimentalOptions[selected]);
        }
        OnClose();
    };
</script>

{#if Show}
<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="modal-backdrop" on:click={OnClose}>
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
    <div class="card p-6 space-y-4 modal-card" on:click|stopPropagation>
        <h2 class="h3">{$_('instances.picker.title')}</h2>

        {#if loading}
            <div class="placeholder animate-pulse w-full h-16" />
        {:else}
            <label class="option-row" class:selected={selected === 'stable'}>
                <input type="radio" name="build" value="stable" bind:group={selected} />
                <div class="option-body">
                    <div class="option-head">
                        <span class="badge variant-filled-primary">{$_('instances.channel.stable')}</span>
                        <span>{latestTag ?? '—'}</span>
                    </div>
                    <p class="option-desc">{$_('instances.picker.stable_desc')}</p>
                </div>
            </label>

            {#if experimentalOptions.length > 0}
                {#each experimentalOptions as option, index}
                    <label class="option-row" class:selected={selected === index}>
                        <input type="radio" name="build" value={index} bind:group={selected} />
                        <div class="option-body">
                            <div class="option-head">
                                <span class="badge variant-filled-warning">{$_('instances.channel.experimental')}</span>
                                <span>{option.label}</span>
                            </div>
                            <p class="option-desc">
                                {option.kind === 'indev' ? $_('instances.picker.indev_desc') : $_('instances.picker.prerelease_desc')}
                            </p>
                        </div>
                    </label>
                {/each}
            {:else}
                <div class="option-row opacity-50 cursor-not-allowed">
                    <input type="radio" disabled />
                    <span class="badge variant-soft">{$_('instances.channel.experimental')}</span>
                    <span class="flex-1">{$_('home.experimental.unavailable')}</span>
                </div>
            {/if}
        {/if}

        <div class="flex gap-3 justify-end">
            <button type="button" class="button-gray button-main" on:click={OnClose}>{$_('common.cancel')}</button>
            <button type="button" class="button-green button-main" disabled={loading} on:click={Confirm}>
                <i class="fa-solid fa-download"></i> {$_('instances.picker.install')}
            </button>
        </div>
    </div>
</div>
{/if}

<style>
    .modal-backdrop {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.6);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 10000;
    }
    .modal-card {
        max-width: 480px;
        width: 92%;
    }
    .option-row {
        display: flex;
        align-items: center;
        gap: 0.7rem;
        padding: 0.6rem 0.8rem;
        border-radius: 0.4rem;
        border: 1px solid rgba(128, 128, 128, 0.3);
        cursor: pointer;
    }
    .option-row.selected {
        border-color: rgba(var(--color-primary-500) / 1);
        background: rgba(var(--color-primary-500) / 0.1);
    }
    .option-body {
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
        flex: 1;
        min-width: 0;
    }
    .option-head {
        display: flex;
        align-items: center;
        gap: 0.6rem;
    }
    .option-desc {
        font-size: 0.78rem;
        opacity: 0.7;
    }
</style>
