<script lang="ts">
    // Dialog to choose which OpenTaiko build to install into an instance:
    // Stable (latest GitHub release) or one of the available experimental builds
    // (grayed out when none exist).
    import { _ } from 'svelte-i18n';
    import Modal from '$lib/components/Modal.svelte';
    import { fetchLatestRelease, fetchExperimentalOptions } from '$lib/utils/builds';
    import type { BuildOption } from '$lib/types';

    interface Props {
        Show?: boolean;
        OnClose?: () => void;
        OnPick?: (build: BuildOption) => void;
    }

    let { Show = false, OnClose = () => {}, OnPick = () => {} }: Props = $props();

    let loading = $state(false);
    let latestTag = $state<string | null>(null);
    // The release list could not be read: shown as such rather than as a release without a
    // version, and nothing can be installed until it is read
    let fetchFailed = $state(false);
    let experimentalOptions = $state<Exclude<BuildOption, { kind: 'stable' }>[]>([]);
    let selected = $state<'stable' | number>('stable');   // 'stable' | index into experimentalOptions
    let loadedFor = false;             // bookkeeping only, never rendered


    const LoadOptions = async () => {
        loading = true;
        latestTag = null;
        fetchFailed = false;
        experimentalOptions = [];
        selected = 'stable';
        try {
            const release = await fetchLatestRelease();
            latestTag = release.tag_name;
        } catch (error) {
            console.error('Failed to fetch the latest release:', error);
            fetchFailed = true;
        }
        experimentalOptions = (await fetchExperimentalOptions(latestTag)).filter((option) => option.kind !== 'stable');
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
    // Fetch the options each time the dialog opens
    $effect(() => {
        if (Show) {
            if (!loadedFor) {
                loadedFor = true;
                LoadOptions();
            }
        } else {
            loadedFor = false;
        }
    });
</script>

{#if Show}
<Modal Label={$_('instances.picker.title')} OnClose={OnClose}>
    <div class="p-6 space-y-4 modal-body">
        <h2 class="h3">{$_('instances.picker.title')}</h2>

        {#if loading}
            <div class="placeholder animate-pulse w-full h-16"></div>
        {:else if fetchFailed}
            <div class="flex items-center gap-3 flex-wrap">
                <i class="fa-solid fa-triangle-exclamation text-red-500"></i>
                <span class="flex-1"><b>{$_('common.fetch_error')}</b></span>
                <button type="button" class="button-blue button-main" onclick={LoadOptions}>
                    <i class="fa-solid fa-rotate"></i> {$_('common.retry')}
                </button>
            </div>
        {:else}
            <label class="option-row" class:selected={selected === 'stable'}>
                <input type="radio" name="build" value="stable" bind:group={selected} />
                <div class="option-body">
                    <div class="option-head">
                        <span class="badge preset-filled-primary-500">{$_('instances.channel.stable')}</span>
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
                                <span class="badge preset-filled-warning-500">{$_('instances.channel.experimental')}</span>
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
                    <span class="badge preset-tonal">{$_('instances.channel.experimental')}</span>
                    <span class="flex-1">{$_('home.experimental.unavailable')}</span>
                </div>
            {/if}
        {/if}

        <div class="flex gap-3 justify-end">
            <button type="button" class="button-gray button-main" onclick={OnClose}>{$_('common.cancel')}</button>
            <button type="button" class="button-green button-main" disabled={loading || fetchFailed} onclick={Confirm}>
                <i class="fa-solid fa-download"></i> {$_('instances.picker.install')}
            </button>
        </div>
    </div>
</Modal>
{/if}

<style>
    .modal-body {
        width: min(480px, calc(100vw - 2rem));
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
        border-color: var(--color-primary-500);
        background: color-mix(in oklab, var(--color-primary-500) 10%, transparent);
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
