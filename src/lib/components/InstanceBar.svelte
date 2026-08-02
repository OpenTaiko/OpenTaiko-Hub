<script>
    import { _ } from 'svelte-i18n';
    import { openPath } from '@tauri-apps/plugin-opener';
    import { instances, activeInstanceId, activeInstance, setActiveInstance, instanceVersions } from '$lib/stores/instances.js';

    export let OnManage = () => {};

    const OpenActiveFolder = async () => {
        const inst = $activeInstance;
        if (!inst) return;
        try {
            await openPath(inst.path);
        } catch (error) {
            console.error('Failed to open the instance folder:', error);
        }
    };

    const channelKey = (inst) => {
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
            on:change={(e) => setActiveInstance(e.target.value)}
        >
            {#each $instances as inst (inst.id)}
                <option value={inst.id}>{inst.name}</option>
            {/each}
        </select>
        {#if $activeInstance}
            {@const key = channelKey($activeInstance)}
            <span
                class="badge whitespace-nowrap"
                class:variant-filled-primary={key === 'stable'}
                class:variant-filled-warning={key === 'experimental'}
                class:variant-soft={key === 'empty'}
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
            <button type="button" class="button-blue button-main" title={$_('instances.open_folder')} on:click={OpenActiveFolder}>
                <i class="fa-solid fa-folder-open"></i>
            </button>
        {/if}
    {/if}
    <button type="button" class="button-blue button-main whitespace-nowrap" on:click={OnManage}>
        <i class="fa-solid fa-gear"></i> {$_('instances.manage')}
    </button>
</div>
