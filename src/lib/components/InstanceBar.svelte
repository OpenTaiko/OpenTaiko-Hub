<script lang="ts">
    import { _ } from 'svelte-i18n';
    import { openPath } from '@tauri-apps/plugin-opener';
    import { instances, activeInstanceId, activeInstance, setActiveInstance, instanceVersions } from '$lib/stores/instances';
    import type { Instance } from '$lib/types';

    interface Props {
        OnManage?: () => void;
    }

    let { OnManage = () => {} }: Props = $props();

    const OpenActiveFolder = async () => {
        const inst = $activeInstance;
        if (!inst) return;
        try {
            await openPath(inst.path);
        } catch (error) {
            console.error('Failed to open the instance folder:', error);
        }
    };

    const channelKey = (inst: Instance | null): 'stable' | 'experimental' | 'empty' => {
        if (!inst?.channel) return 'empty';
        return inst.channel;
    };
</script>

<div class="card p-3 flex items-center gap-3 flex-wrap">
    <span class="whitespace-nowrap"><i class="fa-solid fa-hard-drive"></i> <b>{$_('instances.label')}</b></span>
    {#if $instances.length === 0}
        <span class="opacity-70 flex-1">{$_('instances.none')}</span>
    {:else}
        <select
            class="select flex-1 max-w-md"
            value={$activeInstanceId}
            onchange={(e) => setActiveInstance(e.currentTarget.value)}
        >
            {#each $instances as inst (inst.id)}
                <option value={inst.id}>{inst.name}</option>
            {/each}
        </select>
        {#if $activeInstance}
            {@const key = channelKey($activeInstance)}
            <span
                class="badge whitespace-nowrap"
                class:preset-filled-primary-500={key === 'stable'}
                class:preset-filled-warning-500={key === 'experimental'}
                class:preset-tonal={key === 'empty'}
            >
                {#if key === 'experimental' && $activeInstance.experimental?.label}
                    {$activeInstance.experimental.label}
                {:else}
                    {$_(`instances.channel.${key}`)}
                {/if}
            </span>
            {#if $instanceVersions[$activeInstance.id]}
                <span class="opacity-70 text-sm whitespace-nowrap">{$instanceVersions[$activeInstance.id]}</span>
            {/if}
            <button type="button" class="button-blue button-main" title={$_('instances.open_folder')} onclick={OpenActiveFolder}>
                <i class="fa-solid fa-folder-open"></i>
            </button>
        {/if}
    {/if}
    <button type="button" class="button-blue button-main whitespace-nowrap" onclick={OnManage}>
        <i class="fa-solid fa-gear"></i> {$_('instances.manage')}
    </button>
</div>
