<script>
    import { _ } from 'svelte-i18n';

    export let Node;
    export let CatalogById = new Map();
    export let Depth = 0;

    let open = Depth < 1;

    $: folders = [...Node.children.values()].sort((a, b) => a.name.localeCompare(b.name));
    $: songs = [...Node.songs].sort((a, b) => SongLabel(a).localeCompare(SongLabel(b)));

    const SongLabel = (song) => {
        const catalogEntry = song.uniqueId ? CatalogById.get(song.uniqueId) : null;
        return catalogEntry?.chartTitle ?? song.title ?? song.relPath.split('/').pop();
    };

    const SongStatus = (song) => {
        const catalogEntry = song.uniqueId ? CatalogById.get(song.uniqueId) : null;
        if (!catalogEntry) return 'custom';
        return (song.tjaMd5s ?? []).includes(catalogEntry.tjaMD5) ? 'up_to_date' : 'outdated';
    };
</script>

<div class="tree-node">
    <button type="button" class="tree-row tree-folder" on:click={() => open = !open}>
        <i class="fa-solid fa-chevron-right chevron" class:open></i>
        <i class="fa-solid {open ? 'fa-folder-open' : 'fa-folder'} folder-icon"></i>
        <span class="folder-name">{Node.title ?? Node.name}</span>
        {#if Node.title && Node.title !== Node.name}
            <span class="folder-alt">({Node.name})</span>
        {/if}
        <span class="badge variant-soft song-count">{$_('songs.tree.songs_count', { values: { count: Node.count } })}</span>
    </button>
    {#if open}
        <div class="tree-children">
            {#each folders as child (child.name)}
                <svelte:self Node={child} {CatalogById} Depth={Depth + 1} />
            {/each}
            {#each songs as song (song.relPath)}
                {@const status = SongStatus(song)}
                <div class="tree-row tree-song" title={song.relPath}>
                    <i class="fa-solid fa-music song-icon"></i>
                    <span class="song-title">{SongLabel(song)}</span>
                    {#if status === 'custom'}
                        <span class="badge variant-soft-secondary">{$_('songs.tree.custom')}</span>
                    {:else if status === 'up_to_date'}
                        <span class="badge variant-soft-success">{$_('songs.status.up_to_date')}</span>
                    {:else}
                        <span class="badge variant-soft-warning">{$_('songs.status.outdated')}</span>
                    {/if}
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
    :global(.tree-root > .tree-node) > .tree-row::before {
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
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
