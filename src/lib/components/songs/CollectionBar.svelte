<script lang="ts">
    // One collection: a substantial bar carrying its own art, name, progress meter and
    // actions. Only the top level is an exclusive accordion — opening one closes the
    // others — which is what stops the tab rendering hundreds of rows at once.
    import CollectionBar from './CollectionBar.svelte';
    import SongRow from './SongRow.svelte';
    import SongRowHeader from './SongRowHeader.svelte';
    import { _ } from 'svelte-i18n';
    import { formatSize } from '$lib/utils/library';
    import type { LibraryEntry, LibraryFolder } from '$lib/utils/library';
    import type { SoundtrackEntry } from '$lib/types';

    interface Props {
        Node: LibraryFolder;
        Depth?: number;
        Open?: boolean;
        SelectedKey?: string | null;
        SelectedFolder?: string | null;
        Downloading?: Record<string, number>;
        Disabled?: boolean;
        /** A folder run needs exclusivity; single songs do not. */
        BulkDisabled?: boolean;
        /** Genre art lives in the soundtrack repo, so it looks the same whether or not
         *  the collection is installed. */
        ArtUrl?: (relPath: string) => string | null;
        OnToggle?: (relPath: string) => void;
        OnSelect?: (entry: LibraryEntry) => void;
        OnSelectFolder?: (folder: LibraryFolder) => void;
        OnGet?: (entry: SoundtrackEntry) => void;
        OnGetFolder?: (folder: LibraryFolder) => void;
        OnOpenFolder?: (relPath: string) => void;
    }

    let {
        Node,
        Depth = 0,
        Open = false,
        SelectedKey = null,
        SelectedFolder = null,
        Downloading = {},
        Disabled = false,
        BulkDisabled = false,
        ArtUrl,
        OnToggle,
        OnSelect,
        OnSelectFolder,
        OnGet,
        OnGetFolder,
        OnOpenFolder
    }: Props = $props();

    // Nested folders are ordinary disclosure: several may be open at once.
    let subOpen = $state<Record<string, boolean>>({});

    // One top-level collection can hold several hundred charts, and rendering them all
    // the moment it opens locks the window for seconds. Only the first page is built;
    // the rest arrives when asked for.
    const PAGE = 60;
    let shownCount = $state(PAGE);

    // A new folder, or a new filter, starts from the first page again.
    $effect(() => {
        void Node.relPath;
        void Node.entries.length;
        shownCount = PAGE;
    });

    let visibleEntries = $derived(Node.entries.slice(0, shownCount));
    let hiddenCount = $derived(Math.max(0, Node.entries.length - shownCount));

    let percent = $derived(Node.totalCount === 0 ? 0 : Math.round((Node.installedCount / Node.totalCount) * 100));
    let complete = $derived(Node.totalCount > 0 && Node.installedCount === Node.totalCount);
    let children = $derived([...Node.children.values()].sort((a, b) => a.name.localeCompare(b.name)));
    let art = $derived(ArtUrl?.(Node.relPath) ?? null);
    let isEmpty = $derived(Node.entries.length === 0 && Node.children.size === 0);

    const KIND_ICON: Record<string, string> = {
        chapter: 'fa-book',
        dojo: 'fa-torii-gate',
        collab: 'fa-handshake',
        event: 'fa-star',
        extra: 'fa-box',
        plain: 'fa-folder'
    };
</script>

<section class="collection" class:nested={Depth > 0} class:untouched={!Node.existsLocally} class:picked={SelectedFolder === Node.relPath}>
    <div class="bar">
        <button
            type="button"
            class="open"
            aria-expanded={Open}
            onclick={() => { OnToggle?.(Node.relPath); OnSelectFolder?.(Node); }}
        >
            <i class="fa-solid fa-chevron-right chev" class:turned={Open}></i>

            <span class="art">
                {#if art}
                    <img src={art} alt="" loading="lazy" onerror={(e) => (e.currentTarget as HTMLImageElement).style.display = 'none'} />
                {/if}
                <i class="fa-solid {KIND_ICON[Node.kind] ?? 'fa-folder'} glyph"></i>
            </span>

            <span class="ident">
                <span class="label" title={Node.label}>{Node.label}</span>
                {#if Node.rawName !== Node.label}
                    <span class="raw" title={Node.rawName}>{Node.rawName}</span>
                {/if}
            </span>

            <span class="count" class:all={complete}>
                {$_('songs.tree.installed_of', { values: { installed: Node.installedCount, total: Node.totalCount } })}
            </span>
        </button>

        <div class="actions">
            {#if Node.pendingCount > 0 && OnGetFolder}
                <button
                    type="button"
                    class="button-green button-main"
                    disabled={BulkDisabled}
                    onclick={() => OnGetFolder?.(Node)}
                >
                    <i class="fa-solid fa-download"></i>
                    {$_('songs.tree.get_n', { values: { count: Node.pendingCount } })}
                    <span class="size">{formatSize(Node.pendingMb)}</span>
                </button>
            {/if}
            {#if Node.existsLocally && OnOpenFolder}
                <button
                    type="button"
                    class="button-blue button-main"
                    title={$_('songs.tree.open_folder_tip')}
                    aria-label={$_('songs.tree.open_folder_tip')}
                    onclick={() => OnOpenFolder?.(Node.relPath)}
                >
                    <i class="fa-solid fa-folder-open"></i>
                </button>
            {/if}
        </div>
    </div>

    <div class="track"><span class="fill" class:full={complete} style:width="{percent}%"></span></div>

    {#if Open}
        <div class="contents">
            {#if isEmpty}
                <p class="none">{$_('songs.tree.nothing_here')}</p>
            {/if}

            {#each children as child (child.name)}
                <CollectionBar
                    Node={child}
                    Depth={Depth + 1}
                    Open={subOpen[child.relPath] ?? false}
                    {SelectedKey}
                    {SelectedFolder}
                    {Downloading}
                    {Disabled}
                    {BulkDisabled}
                    {ArtUrl}
                    OnToggle={(p) => (subOpen[p] = !subOpen[p])}
                    {OnSelect}
                    {OnSelectFolder}
                    {OnGet}
                    {OnGetFolder}
                    {OnOpenFolder}
                />
            {/each}

            {#if visibleEntries.length > 0}<SongRowHeader />{/if}
            {#each visibleEntries as entry (entry.key)}
                <SongRow
                    Entry={entry}
                    Selected={SelectedKey === entry.key}
                    Progress={entry.catalog ? Downloading[entry.catalog.uniqueId] : undefined}
                    {Disabled}
                    {OnSelect}
                    {OnGet}
                />
            {/each}

            {#if hiddenCount > 0}
                <button type="button" class="more" onclick={() => shownCount += PAGE * 4}>
                    {$_('songs.tree.show_more', { values: { count: hiddenCount } })}
                </button>
            {/if}
        </div>
    {/if}
</section>

<style>
    .collection {
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 25%, transparent);
        border-radius: 0.5rem;
        overflow: hidden;
        background-color: color-mix(in oklab, var(--color-surface-500) 6%, transparent);
    }
    .collection.nested {margin: 0.25rem 0 0.25rem 1rem; background: none;}
    .collection.picked {border-color: color-mix(in oklab, var(--color-primary-500) 60%, transparent);}

    /* A collection with nothing on disk is dimmed like an available song, but never so
       far that its name or its Get button stops being readable. */
    .collection.untouched > .bar {opacity: 0.72;}
    .collection.untouched > .bar:hover {opacity: 1;}

    .bar {display: flex; align-items: center; gap: 0.5rem; padding: 0.45rem 0.6rem;}
    .bar:hover {background-color: color-mix(in oklab, var(--color-primary-500) 8%, transparent);}

    .open {
        flex: 1;
        min-width: 0;
        display: flex;
        align-items: center;
        gap: 0.6rem;
        background: none;
        border: 0;
        padding: 0;
        font: inherit;
        color: inherit;
        text-align: left;
        cursor: pointer;
    }

    .chev {width: 0.75rem; font-size: 0.75rem; opacity: 0.6; transition: transform 120ms;}
    .chev.turned {transform: rotate(90deg);}

    .art {
        position: relative;
        flex: none;
        width: 2.5rem;
        height: 2.5rem;
        border-radius: 0.375rem;
        overflow: hidden;
        display: grid;
        place-items: center;
        background-color: color-mix(in oklab, var(--color-surface-500) 22%, transparent);
    }
    .nested .art {width: 1.75rem; height: 1.75rem;}
    .art img {position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover;}
    .glyph {font-size: 0.9rem; opacity: 0.55;}

    .ident {display: flex; flex-direction: column; min-width: 0; flex: 1 1 auto; line-height: 1.3;}
    .label {font-weight: 700; font-size: 0.92rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;}
    /* The raw folder name stays visible and selectable: it is what the file explorer shows. */
    .raw {font-size: 0.7rem; opacity: 0.55; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; user-select: text;}

    /* The count sits beside the name and the track runs the full width below, so the
       name gets the room instead of competing with a fixed-width meter column. */
    .count {
        flex: none;
        margin-left: 0.75rem;
        font-size: 0.8rem;
        line-height: 1.5;
        font-variant-numeric: tabular-nums;
        opacity: 0.9;
        white-space: nowrap;
    }
    .count.all {opacity: 1; font-weight: 700;}
    .track {
        height: 0.3rem;
        margin: 0 0.6rem 0.4rem;
        border-radius: 999px;
        overflow: hidden;
        background-color: color-mix(in oklab, var(--color-surface-500) 30%, transparent);
    }
    .fill {display: block; height: 100%; background-color: var(--color-primary-500);}
    .fill.full {background-color: #16a34a;}

    .actions {flex: none; display: flex; gap: 0.25rem;}
    .size {opacity: 0.85; font-weight: 400;}

    .contents {
        display: flex;
        flex-direction: column;
        gap: 0.1rem;
        padding: 0.2rem 0.6rem 0.5rem;
    }
    .nested > .contents {padding-right: 0;}
    .none {font-size: 0.8rem; opacity: 0.6; padding: 0.3rem 0.5rem;}

    .more {
        align-self: flex-start;
        margin: 0.25rem 0 0 1.8rem;
        font: inherit;
        font-size: 0.78rem;
        font-weight: 700;
        padding: 0.25rem 0.6rem;
        border-radius: 0.375rem;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 35%, transparent);
        background: none;
        color: inherit;
        cursor: pointer;
    }
    .more:hover {background-color: color-mix(in oklab, var(--color-primary-500) 12%, transparent);}
</style>
