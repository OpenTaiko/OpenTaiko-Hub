<script>
    import { getContext } from 'svelte';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';
    import { open } from '@tauri-apps/plugin-dialog';
    import { openPath } from '@tauri-apps/plugin-opener';

    import {
        instances,
        activeInstanceId,
        instanceVersions,
        setActiveInstance,
        createInstance,
        attachInstance,
        detachInstance,
        refreshInstanceVersion,
        updateInstance,
        linkGlobalSongs
    } from '$lib/stores/instances.js';

    import BuildPickerModal from '$lib/components/BuildPickerModal.svelte';

    const { TriggerError, TriggerSuccess } = getContext('toast');

    /**
     * @typedef {Object} Props
     * @property {boolean} [Show]
     * @property {any} [OnClose]
     */

    /** @type {Props} */
    let { Show = false, OnClose = () => {} } = $props();

    let newInstanceName = $state('');
    let showBuildPicker = $state(false);

    const channelLabel = (inst) => {
        if (inst.channel === 'experimental') return inst.experimental?.label ?? $_('instances.channel.experimental');
        if (inst.channel === 'stable') return $_('instances.channel.stable');
        return $_('instances.channel.empty');
    };

    const DoAttach = async () => {
        try {
            const dir = await open({ directory: true, multiple: false, title: get(_)('instances.attach') });
            if (!dir) return;
            const instance = await attachInstance(dir);
            await refreshInstanceVersion(instance.id);
            await linkGlobalSongs(get(instances).find((i) => i.id === instance.id));
            TriggerSuccess(get(_)('instances.success.attached', { values: { name: instance.name } }));
        } catch (error) {
            TriggerError(get(_)('instances.error.attach_failed', { values: { error } }));
        }
    };

    // Creating an instance asks which build to install (Stable / Experimental);
    // the download starts automatically on the Home tab once the instance is created.
    const DoCreate = async (build) => {
        try {
            const instance = await createInstance(newInstanceName);
            newInstanceName = '';
            await updateInstance(instance.id, {
                channel: build.kind === 'stable' ? 'stable' : 'experimental',
                pendingInstall: build
            });
            TriggerSuccess(get(_)('instances.success.created', { values: { name: instance.name } }));
            OnClose();
        } catch (error) {
            TriggerError(get(_)('instances.error.create_failed', { values: { error } }));
        }
    };

    const DoDetach = async (inst) => {
        await detachInstance(inst.id);
        TriggerSuccess(get(_)('instances.success.detached', { values: { name: inst.name } }));
    };

    const OpenFolder = async (inst) => {
        try {
            await openPath(inst.path);
        } catch (error) {
            TriggerError(get(_)('home.error.launch', { values: { error } }));
        }
    };
</script>

{#if Show}
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="modal-backdrop" onclick={OnClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="card p-6 space-y-4 modal-card" onclick={(e) => e.stopPropagation()}>
        <div class="flex justify-between items-center">
            <h2 class="h3">{$_('instances.title')}</h2>
            <button class="button-red button-main" onclick={OnClose}>{$_('hof.close')}</button>
        </div>

        {#if $instances.length === 0}
            <p class="opacity-70">{$_('instances.none')}</p>
        {:else}
        <div class="table-wrap">
            <table class="table table-hover">
                <thead>
                    <tr>
                        <th></th>
                        <th>{$_('instances.col.name')}</th>
                        <th>{$_('instances.col.channel')}</th>
                        <th>{$_('instances.col.version')}</th>
                        <th>{$_('instances.col.path')}</th>
                        <th>{$_('instances.col.actions')}</th>
                    </tr>
                </thead>
                <tbody>
                    {#each $instances as inst (inst.id)}
                    <tr>
                        <td>
                            {#if inst.id === $activeInstanceId}
                                <span class="badge preset-filled-success-500">{$_('instances.active')}</span>
                            {:else}
                                <button class="button-blue button-main" onclick={() => setActiveInstance(inst.id)}>{$_('instances.set_active')}</button>
                            {/if}
                        </td>
                        <td>{inst.name}</td>
                        <td>{channelLabel(inst)}</td>
                        <td>{$instanceVersions[inst.id] ?? '—'}</td>
                        <td class="path-cell" title={inst.path}>{inst.path}</td>
                        <td class="whitespace-nowrap">
                            <button class="button-blue button-main" title={$_('instances.open_folder')} onclick={() => OpenFolder(inst)}>
                                <i class="fa-solid fa-folder-open"></i>
                            </button>
                            <button class="button-red button-main" title={$_('instances.detach')} onclick={() => DoDetach(inst)}>
                                <i class="fa-solid fa-link-slash"></i>
                            </button>
                        </td>
                    </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        {/if}

        <p class="text-sm opacity-70">{$_('instances.detach_note')}</p>

        <hr />

        <div class="flex gap-3 flex-wrap items-center">
            <button class="button-blue button-main" onclick={DoAttach}>
                <i class="fa-solid fa-folder-plus"></i> {$_('instances.attach')}
            </button>
            <span class="opacity-50">|</span>
            <input
                class="rounded-md px-3 py-2 bg-white text-blue-950"
                placeholder={$_('instances.create_name_placeholder')}
                bind:value={newInstanceName}
            />
            <button class="button-green button-main" onclick={() => showBuildPicker = true}>
                <i class="fa-solid fa-plus"></i> {$_('instances.create')}
            </button>
        </div>
    </div>
</div>
{/if}

<BuildPickerModal Show={showBuildPicker} OnClose={() => showBuildPicker = false} OnPick={DoCreate} />

<style>
    .modal-backdrop {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.6);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
    }
    .modal-card {
        max-width: 1000px;
        width: 95%;
        max-height: 85vh;
        overflow-y: auto;
    }
    .path-cell {
        max-width: 280px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
