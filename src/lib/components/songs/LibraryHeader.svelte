<script lang="ts">
    // Answers, permanently and without a tooltip: what am I browsing, how much of it do I
    // have, and what can I do right now. The scope switch replaces both the old List/Tree
    // toggle and the "show available" checkbox — one control, three honest states.
    import { _ } from 'svelte-i18n';
    import { formatSize } from '$lib/utils/library';
    import type { Scope } from '$lib/utils/library';

    interface Props {
        LibraryPath?: string | null;
        Installed?: number;
        Total?: number;
        PendingMb?: number;
        Outdated?: number;
        Scanning?: boolean;
        Found?: number;
        Busy?: boolean;
        /** Set while a bulk download runs. */
        Bulk?: { done: number; total: number; mbLeft: number; finishing?: boolean } | null;
        CurrentScope?: Scope;
        Search?: string;
        OnScope?: (scope: Scope) => void;
        OnSearch?: (value: string) => void;
        OnReload?: () => void;
        OnOpenLibrary?: () => void;
        OnGetAll?: () => void;
        OnCancelBulk?: () => void;
        /** Cancel was pressed and the run has yet to stop. */
        Cancelling?: boolean;
    }

    let {
        LibraryPath = null,
        Installed = 0,
        Total = 0,
        PendingMb = 0,
        Outdated = 0,
        Scanning = false,
        Found = 0,
        Busy = false,
        Bulk = null,
        CurrentScope = 'all',
        Search = '',
        OnScope,
        OnSearch,
        OnReload,
        OnOpenLibrary,
        OnGetAll,
        OnCancelBulk,
        Cancelling = false
    }: Props = $props();

    let percent = $derived(Total === 0 ? 0 : Math.round((Installed / Total) * 100));
    let missing = $derived(Math.max(0, Total - Installed));

    const SCOPES: { id: Scope; label: string }[] = [
        { id: 'all', label: 'songs.scope.all' },
        { id: 'installed', label: 'songs.scope.installed' },
        { id: 'updates', label: 'songs.scope.updates' }
    ];

    // Each search rebuilds the whole library view, so typing waits for a pause instead
    // of rebuilding it at every keystroke
    const SEARCH_DELAY_MS = 150;
    let searchTimer: ReturnType<typeof setTimeout> | null = null;
    const QueueSearch = (value: string) => {
        if (searchTimer !== null) clearTimeout(searchTimer);
        searchTimer = setTimeout(() => {
            searchTimer = null;
            OnSearch?.(value);
        }, SEARCH_DELAY_MS);
    };
    const ClearSearch = () => {
        if (searchTimer !== null) clearTimeout(searchTimer);
        searchTimer = null;
        OnSearch?.('');
    };
</script>

<header class="library card">
    <div class="top">
        <div class="ident">
            <h2>{$_('songs.library.title')}</h2>
            {#if LibraryPath}
                <button type="button" class="pathchip" title={LibraryPath} onclick={() => OnOpenLibrary?.()}>
                    <i class="fa-solid fa-folder-open"></i> <span>{LibraryPath}</span>
                </button>
            {/if}
        </div>

        <div class="tools">
            <button type="button" class="button-blue button-main" disabled={Scanning} onclick={() => OnReload?.()}>
                <i class="fa-solid fa-rotate"></i> {$_('common.reload')}
            </button>
            {#if missing > 0 || Outdated > 0}
                <button type="button" class="button-green button-main" disabled={Busy || Scanning} onclick={() => OnGetAll?.()}>
                    <i class="fa-solid fa-download"></i>
                    {$_('songs.library.get_everything', { values: { count: missing + Outdated } })}
                    <span class="size">{formatSize(PendingMb)}</span>
                </button>
            {/if}
        </div>
    </div>

    <!-- No progress bar here on purpose. It was a coloured strip with a label that kept
         losing contrast against whatever the theme set primary-500 to; the sentence
         carries the same number and cannot become unreadable. -->
    <p class="statusline">
        {#if Scanning}
            <i class="fa-solid fa-spinner fa-spin"></i>
            {$_('songs.scan.progress', { values: { count: Found } })}
        {:else if Bulk}
            <!-- Counted here rather than per song, and in place: the rows that show each
                 song's own progress are often inside collapsed folders -->
            <i class="fa-solid fa-spinner fa-spin"></i>
            <!-- After a cancel, only the folders of the songs that arrived are finished: said
                 as such, rather than reading as if the download had resumed -->
            {#if Bulk.finishing}
                {$_('bulk.finishing', { values: { count: Bulk.done } })}
            {:else}
                {$_('bulk.status', { values: { done: Bulk.done, total: Bulk.total, size: formatSize(Bulk.mbLeft) } })}
            {/if}
            <button type="button" class="button-gray button-main cancel" disabled={Cancelling} onclick={() => OnCancelBulk?.()}>
                <i class="fa-solid {Cancelling ? 'fa-spinner fa-spin' : 'fa-xmark'}"></i> {Cancelling ? $_('jobs.cancelling') : $_('common.cancel')}
            </button>
        {:else}
            <b class="pct">{percent}%</b>
            {$_('songs.library.summary', { values: { installed: Installed, total: Total } })}
            {#if Outdated > 0}
                <span class="warn">· {$_('songs.library.updates_waiting', { values: { count: Outdated } })}</span>
            {/if}
        {/if}
    </p>

    <div class="filters">
        <div class="scopes" role="tablist">
            {#each SCOPES as scope (scope.id)}
                <button
                    type="button"
                    role="tab"
                    class="scope"
                    aria-selected={CurrentScope === scope.id}
                    onclick={() => OnScope?.(scope.id)}
                >
                    {$_(scope.label)}
                    {#if scope.id === 'updates' && Outdated > 0}<span class="pill">{Outdated}</span>{/if}
                </button>
            {/each}
        </div>

        <div class="searchbox">
            <i class="fa-solid fa-magnifying-glass"></i>
            <input
                type="text"
                placeholder={$_('songs.library.search')}
                aria-label={$_('songs.library.search')}
                value={Search}
                oninput={(e) => QueueSearch(e.currentTarget.value)}
            >
            {#if Search}
                <button type="button" class="clear" aria-label={$_('common.clear')} title={$_('common.clear')} onclick={ClearSearch}>✕</button>
            {/if}
        </div>
    </div>
</header>

<style>
    .library {display: flex; flex-direction: column; gap: 0.6rem; padding: 0.75rem 0.9rem;}

    .top {display: flex; align-items: flex-start; gap: 1rem; flex-wrap: wrap;}
    .ident {min-width: 0; flex: 1;}
    .ident h2 {margin: 0 0 0.2rem;}

    .pathchip {
        display: inline-flex;
        align-items: center;
        gap: 0.35rem;
        max-width: 100%;
        font-size: 0.72rem;
        padding: 0.15rem 0.5rem;
        border-radius: 999px;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 35%, transparent);
        background: none;
        color: inherit;
        cursor: pointer;
        opacity: 0.9;
    }
    .pathchip:hover {opacity: 1;}
    .pathchip span {overflow: hidden; text-overflow: ellipsis; white-space: nowrap;}

    .tools {display: flex; gap: 0.4rem; flex-wrap: wrap;}
    .size {opacity: 0.85; font-weight: 400;}

    .statusline {font-size: 0.9rem; line-height: 1.5; color: inherit; margin: 0;}
    .statusline .cancel {margin-left: 0.5rem;}
    .warn {color: light-dark(#a1560a, #ffc46b); font-weight: 700;}
    .pct {font-variant-numeric: tabular-nums; margin-right: 0.2rem; color: inherit;}

    .filters {display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap;}

    .scopes {
        display: flex;
        gap: 0.15rem;
        padding: 0.15rem;
        border-radius: 0.5rem;
        background-color: color-mix(in oklab, var(--color-surface-500) 18%, transparent);
    }
    .scope {
        display: inline-flex;
        align-items: center;
        gap: 0.35rem;
        font: inherit;
        font-size: 0.78rem;
        font-weight: 700;
        padding: 0.25rem 0.7rem;
        border: 0;
        border-radius: 0.375rem;
        background: none;
        color: inherit;
        cursor: pointer;
        opacity: 0.7;
    }
    .scope:hover {opacity: 1;}
    /* The theme's --color-primary-contrast-500 is black, and primary-500 is a mid blue,
       so the label was black on blue and unreadable. White on primary-500 was no better
       where a theme's primary is light (about 1.6:1 on Seafoam), so the pill takes a dark
       shade of the theme's primary, on which white reads whatever the theme. */
    .scope[aria-selected='true'] {
        opacity: 1;
        background-color: var(--color-primary-900);
        color: #fff;
        text-shadow: 0 1px 2px rgb(0 0 0 / 0.55);
    }
    .pill {
        font-size: 0.66rem;
        padding: 0 0.3rem;
        border-radius: 999px;
        background-color: color-mix(in oklab, currentColor 22%, transparent);
    }

    .searchbox {
        flex: 1;
        min-width: 12rem;
        display: flex;
        align-items: center;
        gap: 0.4rem;
        padding: 0.25rem 0.6rem;
        border-radius: 0.5rem;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 35%, transparent);
        background-color: light-dark(var(--color-surface-50), var(--color-surface-900));
        color: light-dark(var(--color-surface-950), var(--color-surface-50));
    }
    .searchbox i {opacity: 0.5; font-size: 0.8rem;}
    .searchbox input {flex: 1; min-width: 0; border: 0; background: none; color: inherit; font: inherit; font-size: 0.85rem; outline: none;}
    .searchbox input::placeholder {color: inherit; opacity: 0.55;}
    .clear {background: none; border: 0; color: inherit; opacity: 0.6; cursor: pointer; font-size: 0.8rem;}
    .clear:hover {opacity: 1;}
</style>
