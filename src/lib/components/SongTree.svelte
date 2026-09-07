<script>
    // Tree view of the whole scanned Songs library, grouped per genre folder, including
    // custom charts/songs that are not part of the official catalog.
    import { _ } from 'svelte-i18n';
    import SongTreeNode from '$lib/components/SongTreeNode.svelte';

    let { Songs = [], Genres = {}, CatalogById = new Map() } = $props();

    const makeNode = (name) => ({ name, title: null, children: new Map(), songs: [], count: 0 });

    const nodeFor = (root, relPath) => {
        if (!relPath) return root;
        let node = root;
        for (const part of relPath.split('/')) {
            if (!node.children.has(part)) node.children.set(part, makeNode(part));
            node = node.children.get(part);
        }
        return node;
    };

    const countSongs = (node) => {
        node.count = node.songs.length;
        for (const child of node.children.values()) {
            node.count += countSongs(child);
        }
        return node.count;
    };

    let root = $derived((() => {
        const rootNode = makeNode('');
        for (const [relPath, genre] of Object.entries(Genres)) {
            nodeFor(rootNode, relPath).title = genre.title ?? null;
        }
        for (const song of Songs) {
            const parentPath = song.relPath.includes('/')
                ? song.relPath.slice(0, song.relPath.lastIndexOf('/'))
                : '';
            const parent = nodeFor(rootNode, parentPath);
            parent.songs.push(song);
        }
        countSongs(rootNode);
        return rootNode;
    })());

    let topLevel = $derived([...root.children.values()].sort((a, b) => a.name.localeCompare(b.name)));
</script>

<div class="card p-4 space-y-1 tree-root">
    {#if root.count === 0 && root.children.size === 0}
        <p class="opacity-70">{$_('songs.tree.empty')}</p>
    {:else}
        {#each topLevel as node (node.name)}
            <SongTreeNode Node={node} {CatalogById} Depth={0} />
        {/each}
        {#each [...root.songs].sort((a, b) => a.relPath.localeCompare(b.relPath)) as song (song.relPath)}
            <div class="opacity-80 text-sm" title={song.relPath}><i class="fa-solid fa-music"></i> {song.title ?? song.relPath}</div>
        {/each}
    {/if}
</div>
