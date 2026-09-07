<script>
    // A component may import itself for recursion (replaces <svelte:self>)
    import SongTreeNode from './SongTreeNode.svelte';
    import { _ } from 'svelte-i18n';

    /**
     * @typedef {Object} Props
     * @property {any} Node
     * @property {any} [CatalogById]
     * @property {number} [Depth]
     */

    /** @type {Props} */
    let { Node, CatalogById = new Map(), Depth = 0 } = $props();

    // Depth only seeds the initial state: folders start expanded at the top level
    // svelte-ignore state_referenced_locally
    let open = $state(Depth < 1);


    const DIFF_ORDER = ['Easy', 'Normal', 'Hard', 'Oni', 'Edit', 'Tower', 'Dan'];
    const DIFF_ABBR = { Easy: 'EZ', Normal: 'NM', Hard: 'HD', Oni: 'EX', Edit: 'EXEX', Tower: 'Tower', Dan: 'Dan' };

    const IsCatalog = (song) => !!(song.uniqueId && CatalogById.has(song.uniqueId));

    const SongLabel = (song) => {
        const catalogEntry = song.uniqueId ? CatalogById.get(song.uniqueId) : null;
        return catalogEntry?.chartTitle ?? song.title ?? song.relPath.split('/').pop();
    };

    const SongStatus = (song) => {
        const catalogEntry = song.uniqueId ? CatalogById.get(song.uniqueId) : null;
        if (!catalogEntry) return 'custom';
        return (song.tjaMd5s ?? []).includes(catalogEntry.tjaMD5) ? 'up_to_date' : 'outdated';
    };

    // Difficulty chips from the locally scanned chart, ordered Easy → Dan
    const DiffList = (song) => {
        const levels = {};
        for (const d of song.difficulties ?? []) levels[d.course] = d.level;
        return DIFF_ORDER
            .filter((course) => course in levels)
            .map((course) => ({ course, abbr: DIFF_ABBR[course] ?? course, level: levels[course] }));
    };

    // Fractional levels: 10 and above use the Taiko "+" convention (10.5+ → "10+",
    // 10.4 → "10"); below 10 the integer is shown.
    const FormatLevel = (level) => {
        const base = Math.floor(level);
        if (level >= 10 && level - base >= 0.5) return `${base}+`;
        return `${base}`;
    };
    let folders = $derived([...Node.children.values()].sort((a, b) => a.name.localeCompare(b.name)));
    let songs = $derived([...Node.songs].sort((a, b) => SongLabel(a).localeCompare(SongLabel(b))));
</script>

<div class="tree-node">
    <button type="button" class="tree-row tree-folder" onclick={() => open = !open}>
        <i class="fa-solid fa-chevron-right chevron" class:open></i>
        <i class="fa-solid {open ? 'fa-folder-open' : 'fa-folder'} folder-icon"></i>
        <span class="folder-name">{Node.title ?? Node.name}</span>
        {#if Node.title && Node.title !== Node.name}
            <span class="folder-alt">({Node.name})</span>
        {/if}
        <span class="badge preset-tonal song-count">{$_('songs.tree.songs_count', { values: { count: Node.count } })}</span>
    </button>
    {#if open}
        <div class="tree-children">
            {#each folders as child (child.name)}
                <SongTreeNode Node={child} {CatalogById} Depth={Depth + 1} />
            {/each}
            {#each songs as song (song.relPath)}
                {@const status = SongStatus(song)}
                <div class="tree-row tree-song" title={song.relPath}>
                    <i class="fa-solid fa-music song-icon"></i>
                    <span class="song-title">{SongLabel(song)}</span>
                    {#if IsCatalog(song)}
                        <span class="badge preset-tonal-primary source-chip">{$_('songs.tree.soundtrack')}</span>
                        {#if status === 'up_to_date'}
                            <span class="badge preset-tonal-success">{$_('songs.status.up_to_date')}</span>
                        {:else}
                            <span class="badge preset-tonal-warning">{$_('songs.status.outdated')}</span>
                        {/if}
                    {:else}
                        <span class="badge preset-tonal-secondary source-chip">{$_('songs.tree.custom')}</span>
                    {/if}
                    <span class="diffs">
                        {#each DiffList(song) as diff (diff.course)}
                            <span class="diff-chip diff-{diff.course}" title={diff.course}>{diff.abbr}&nbsp;{FormatLevel(diff.level)}</span>
                            {#if diff.course === 'Tower'}
                                {#if song.side === 'Ex'}
                                    <span class="diff-chip side-spicy">{$_('songs.tree.spicy')}</span>
                                {:else}
                                    <span class="diff-chip side-sweet">{$_('songs.tree.sweet')}</span>
                                {/if}
                            {/if}
                        {/each}
                    </span>
                </div>
            {/each}
        </div>
    {/if}
</div>

<style>
    .tree-row {
        display: flex;
        align-items: center;
        gap: 0.45rem;
        padding: 0.18rem 0.4rem;
        border-radius: 0.3rem;
        width: 100%;
        text-align: left;
        position: relative;
    }
    .tree-row:hover {
        background: rgba(128, 128, 128, 0.15);
    }
    .tree-folder {
        cursor: pointer;
        user-select: none;
    }
    .chevron {
        font-size: 0.65rem;
        opacity: 0.6;
        transition: transform 0.12s ease;
        width: 0.8rem;
    }
    .chevron.open {
        transform: rotate(90deg);
    }
    .folder-icon {
        color: #eab308;
    }
    .folder-name {
        font-weight: 600;
    }
    .folder-alt {
        opacity: 0.55;
        font-size: 0.8rem;
    }
    .song-count {
        font-size: 0.7rem;
    }
    /* Indent guides: children hang off a vertical rail under their parent */
    .tree-children {
        margin-left: 0.95rem;
        padding-left: 0.85rem;
        border-left: 1px solid rgba(128, 128, 128, 0.35);
    }
    .tree-row::before {
        content: '';
        position: absolute;
        left: -0.85rem;
        top: 50%;
        width: 0.6rem;
        height: 1px;
        background: rgba(128, 128, 128, 0.35);
    }
    /* No connector tick for top-level rows (they have no rail to attach to) */
    :global(.tree-root) > .tree-node > .tree-row::before {
        display: none;
    }
    .tree-song {
        opacity: 0.95;
    }
    .song-icon {
        opacity: 0.6;
        font-size: 0.8rem;
        width: 0.8rem;
    }
    .song-title {
        /* Title yields first: it shrinks and truncates so the chips never wrap */
        flex: 1 1 auto;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
    .source-chip {
        font-size: 0.68rem;
        white-space: nowrap;
        flex-shrink: 0;
    }
    .diffs {
        display: inline-flex;
        gap: 0.25rem;
        flex-wrap: nowrap;
        flex-shrink: 0;
        margin-left: auto;
    }
    .diff-chip {
        font-size: 0.66rem;
        font-weight: 700;
        padding: 0.05rem 0.35rem;
        border-radius: 0.25rem;
        white-space: nowrap;
    }
    /* Same difficulty colours as the list view's SongDifficultyChip
       (Easy=blue, Normal=green, Hard=yellow, Oni=red, Edit=purple, Tower=orange, Dan=blue) */
    .diff-Easy   { background: #dbeafe; color: #1e40af; }
    .diff-Normal { background: #dcfce7; color: #166534; }
    .diff-Hard   { background: #fef9c3; color: #854d0e; }
    .diff-Oni    { background: #fee2e2; color: #991b1b; }
    .diff-Edit   { background: #f3e8ff; color: #6b21a8; }
    .diff-Tower  { background: #ffedd5; color: #9a3412; }
    .diff-Dan    { background: #dbeafe; color: #1e40af; }
    /* Tower side: Spicy (Ex, red) / Sweet (green) */
    .side-spicy { background: #fee2e2; color: #991b1b; }
    .side-sweet { background: #dcfce7; color: #166534; }
</style>
