<script lang="ts">
    import { getContext } from 'svelte';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';
    import { ipc } from '$lib/ipc';
    import { OpenFolder as OpenFolderPath } from '$lib/utils/path';
    import Modal from '$lib/components/Modal.svelte';
    import { busyInstanceIds } from '$lib/stores/activity';

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
    } from '$lib/stores/instances';
    import type { BuildOption, Instance, ToastContext } from '$lib/types';

    import BuildPickerModal from '$lib/components/BuildPickerModal.svelte';

    const { TriggerError, TriggerSuccess } = getContext<ToastContext>('toast');

    interface Props {
        Show?: boolean;
        OnClose?: () => void;
    }

    let { Show = false, OnClose = () => {} }: Props = $props();

    let newInstanceName = $state('');
    let showBuildPicker = $state(false);

    const channelLabel = (inst: Instance): string => {
        if (inst.channel === 'experimental') return inst.experimental?.label ?? $_('instances.channel.experimental');
        if (inst.channel === 'stable') return $_('instances.channel.stable');
        return $_('instances.channel.empty');
    };

    const DoAttach = async () => {
        try {
            // The backend shows the dialog and records the folder as an instance; the page
            // naming a folder itself would let it open any folder to itself
            const dir = await ipc.attachInstanceDir(get(_)('instances.attach'));
            if (!dir) return;
            const instance = await attachInstance(dir);
            await refreshInstanceVersion(instance.id);
            await linkGlobalSongs(get(instances).find((i) => i.id === instance.id));
            TriggerSuccess(get(_)('instances.success.attached', { values: { name: instance.name } }));
        } catch (error) {
            TriggerError(get(_)('instances.error.attach_failed', { values: { error: String(error) } }));
        }
    };

    // Creating an instance asks which build to install (Stable / Experimental);
    // the download starts automatically on the Home tab once the instance is created.
    const DoCreate = async (build: BuildOption) => {
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
            TriggerError(get(_)('instances.error.create_failed', { values: { error: String(error) } }));
        }
    };

    const DoDetach = async (inst: Instance) => {
        // An install still writing into the folder would lose track of its instance
        if (get(busyInstanceIds).has(inst.id)) return;
        await detachInstance(inst.id);
        TriggerSuccess(get(_)('instances.success.detached', { values: { name: inst.name } }));
    };

    const OpenFolder = (inst: Instance) => OpenFolderPath(inst.path);
</script>

{#if Show}
<Modal Label={$_('instances.title')} OnClose={OnClose}>
    <div class="p-6 space-y-4 modal-body">
        <div class="flex justify-between items-center">
            <h2 class="h3">{$_('instances.title')}</h2>
            <button type="button" class="btn-icon btn-icon-sm preset-tonal" onclick={OnClose} aria-label={$_('hof.close')} title={$_('hof.close')}><i class="fa-solid fa-xmark"></i></button>
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
                            <button class="button-blue button-main" title={$_('instances.open_folder')} aria-label={$_('instances.open_folder')} onclick={() => OpenFolder(inst)}>
                                <i class="fa-solid fa-folder-open"></i>
                            </button>
                            <button type="button" class="button-gray button-main disabled:opacity-50" title={$busyInstanceIds.has(inst.id) ? $_('jobs.instance_busy') : $_('instances.detach')} aria-label={$_('instances.detach')} disabled={$busyInstanceIds.has(inst.id)} onclick={() => DoDetach(inst)}>
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
                class="name-field"
                placeholder={$_('instances.create_name_placeholder')}
                aria-label={$_('instances.create_name_placeholder')}
                bind:value={newInstanceName}
            />
            <button class="button-green button-main" onclick={() => showBuildPicker = true}>
                <i class="fa-solid fa-plus"></i> {$_('instances.create')}
            </button>
        </div>
    </div>
</Modal>
{/if}

<BuildPickerModal Show={showBuildPicker} OnClose={() => showBuildPicker = false} OnPick={DoCreate} />

<style>
    .modal-body {
        width: min(1000px, calc(100vw - 2rem));
    }
    /* Theme colours rather than a fixed white field, which glared on dark themes */
    .name-field {
        font: inherit;
        padding: 0.4rem 0.75rem;
        border-radius: 0.375rem;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 40%, transparent);
        background-color: light-dark(var(--color-surface-50), var(--color-surface-900));
        color: light-dark(var(--color-surface-950), var(--color-surface-50));
    }
    .name-field:focus {outline: none; border-color: var(--color-primary-500);}
    .path-cell {
        max-width: 280px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
