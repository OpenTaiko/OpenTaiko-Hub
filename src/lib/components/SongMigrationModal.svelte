<script>
    // Resolves an instance's local Songs folder into the shared library. New charts are
    // moved, byte-identical charts are dropped, and charts that clash with a different
    // version already in the library are resolved by the user — per chart (showing each
    // side's hash and last-edited date) or in bulk.
    import { onMount, getContext } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    const { TriggerError, TriggerSuccess } = getContext('toast');

    export let Candidate;              // { instance, srcPath, count }
    export let GlobalPath;
    export let CatalogById = new Map();
    export let OnClose = () => {};
    export let OnApplied = () => {};

    let loading = true;
    let applying = false;
    let plan = null;
    let conflicts = [];
    // srcRelPath -> 'keep_global' | 'use_instance'
    let choices = {};

    onMount(async () => {
        try {
            plan = await invoke('plan_migration', {
                srcSongs: Candidate.srcPath,
                destSongs: GlobalPath
            });
            conflicts = plan.items.filter((item) => item.status === 'conflict');
            // Default: keep the version already in the shared library
            choices = Object.fromEntries(conflicts.map((item) => [item.src.relPath, 'keep_global']));
        } catch (error) {
            TriggerError(get(_)('songs.migrate.error', { values: { error } }));
            OnClose();
        }
        loading = false;
    });

    const Label = (entry) => {
        const catalogEntry = entry.uniqueId ? CatalogById.get(entry.uniqueId) : null;
        return catalogEntry?.chartTitle ?? entry.title ?? entry.relPath.split('/').pop();
    };
    const ShortHash = (entry) => {
        if (!entry.md5s || entry.md5s.length === 0) return '—';
        const head = entry.md5s[0].slice(0, 10);
        return entry.md5s.length > 1 ? `${head} +${entry.md5s.length - 1}` : head;
    };
    const EditedDate = (entry) => {
        if (!entry.modified) return '—';
        return new Date(entry.modified * 1000).toLocaleString();
    };

    const SetAll = (action) => {
        choices = Object.fromEntries(conflicts.map((item) => [item.src.relPath, action]));
    };

    const Apply = async () => {
        applying = true;
        try {
            const decisions = plan.items.map((item) => {
                let action;
                if (item.status === 'new') action = 'move';
                else if (item.status === 'identical') action = 'keep_global';
                else action = choices[item.src.relPath] ?? 'keep_global';
                return {
                    srcRelPath: item.src.relPath,
                    action,
                    destRelPath: item.dest?.relPath ?? null
                };
            });

            const summary = await invoke('apply_migration', {
                srcSongs: Candidate.srcPath,
                destSongs: GlobalPath,
                decisions
            });
            TriggerSuccess(get(_)('songs.migrate.applied', {
                values: { moved: summary.moved, replaced: summary.replaced, discarded: summary.discarded }
            }));
            OnApplied();
        } catch (error) {
            TriggerError(get(_)('songs.migrate.error', { values: { error } }));
        }
        applying = false;
    };
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="modal-backdrop" on:click={OnClose}>
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
    <div class="card p-6 space-y-4 modal-card" on:click|stopPropagation>
        <div class="flex justify-between items-center">
            <h2 class="h3">{$_('songs.migrate.modal_title', { values: { instance: Candidate.instance.name } })}</h2>
            <button class="btn-icon btn-icon-sm variant-filled" on:click={OnClose} aria-label={$_('common.cancel')}>✕</button>
        </div>

        {#if loading}
            <div class="placeholder animate-pulse w-full h-24" />
        {:else if plan}
            <!-- Summary of the non-conflicting work -->
            <div class="space-y-1 text-sm">
                {#if plan.newCount > 0}
                    <p><i class="fa-solid fa-plus text-green-500"></i> {$_('songs.migrate.summary_new', { values: { count: plan.newCount } })}</p>
                {/if}
                {#if plan.identicalCount > 0}
                    <p><i class="fa-solid fa-equals opacity-60"></i> {$_('songs.migrate.summary_identical', { values: { count: plan.identicalCount } })}</p>
                {/if}
                {#if plan.newCount === 0 && plan.identicalCount === 0 && conflicts.length === 0}
                    <p class="opacity-70">{$_('songs.migrate.nothing')}</p>
                {/if}
            </div>

            {#if conflicts.length > 0}
                <hr />
                <div class="flex items-center justify-between flex-wrap gap-2">
                    <div>
                        <p class="font-bold">{$_('songs.migrate.conflicts_title', { values: { count: conflicts.length } })}</p>
                        <p class="text-sm opacity-70">{$_('songs.migrate.conflicts_hint')}</p>
                    </div>
                    <div class="flex gap-2">
                        <button type="button" class="button-blue button-main" on:click={() => SetAll('keep_global')}>{$_('songs.migrate.bulk_keep_global')}</button>
                        <button type="button" class="button-blue button-main" on:click={() => SetAll('use_instance')}>{$_('songs.migrate.bulk_use_instance')}</button>
                    </div>
                </div>

                <div class="conflict-list">
                    {#each conflicts as item (item.src.relPath)}
                    <div class="conflict-row">
                        <div class="conflict-title" title={item.src.relPath}>
                            <i class="fa-solid fa-triangle-exclamation text-yellow-500"></i>
                            <span>{Label(item.src)}</span>
                        </div>
                        <div class="conflict-options">
                            <label class="conflict-option" class:selected={choices[item.src.relPath] === 'keep_global'}>
                                <input type="radio" value="keep_global" bind:group={choices[item.src.relPath]} />
                                <div class="opt-body">
                                    <span class="badge variant-filled-primary">{$_('songs.migrate.col_global')}</span>
                                    <span class="meta">{$_('songs.migrate.hash')}: <code>{ShortHash(item.dest)}</code></span>
                                    <span class="meta">{$_('songs.migrate.edited')}: {EditedDate(item.dest)}</span>
                                </div>
                            </label>
                            <label class="conflict-option" class:selected={choices[item.src.relPath] === 'use_instance'}>
                                <input type="radio" value="use_instance" bind:group={choices[item.src.relPath]} />
                                <div class="opt-body">
                                    <span class="badge variant-filled-warning">{$_('songs.migrate.col_instance')}</span>
                                    <span class="meta">{$_('songs.migrate.hash')}: <code>{ShortHash(item.src)}</code></span>
                                    <span class="meta">{$_('songs.migrate.edited')}: {EditedDate(item.src)}</span>
                                </div>
                            </label>
                        </div>
                    </div>
                    {/each}
                </div>
            {/if}

            <div class="flex gap-3 justify-end">
                <button type="button" class="button-gray button-main" on:click={OnClose}>{$_('common.cancel')}</button>
                <button type="button" class="button-green button-main" disabled={applying} on:click={Apply}>
                    <i class="fa-solid fa-right-left"></i> {$_('songs.migrate.apply')}
                </button>
            </div>
        {/if}
    </div>
</div>

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
        max-width: 900px;
        width: 94%;
        max-height: 85vh;
        overflow-y: auto;
    }
    .conflict-list {
        display: flex;
        flex-direction: column;
        gap: 0.6rem;
    }
    .conflict-row {
        border: 1px solid rgba(128, 128, 128, 0.25);
        border-radius: 0.4rem;
        padding: 0.6rem 0.75rem;
    }
    .conflict-title {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        font-weight: 600;
        margin-bottom: 0.45rem;
    }
    .conflict-options {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 0.5rem;
    }
    @media (max-width: 640px) {
        .conflict-options { grid-template-columns: 1fr; }
    }
    .conflict-option {
        display: flex;
        align-items: flex-start;
        gap: 0.5rem;
        padding: 0.5rem 0.6rem;
        border: 1px solid rgba(128, 128, 128, 0.3);
        border-radius: 0.35rem;
        cursor: pointer;
    }
    .conflict-option.selected {
        border-color: rgba(var(--color-primary-500) / 1);
        background: rgba(var(--color-primary-500) / 0.08);
    }
    .opt-body {
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
        font-size: 0.8rem;
    }
    .meta {
        opacity: 0.8;
    }
    .meta code {
        font-size: 0.75rem;
    }
</style>
