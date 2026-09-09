<script lang="ts">
    // Tree view of the whole scanned Songs library, grouped per genre folder, including
    // custom charts/songs that are not part of the official catalog.
    import { _ } from 'svelte-i18n';
    import SongTreeNode from '$lib/components/SongTreeNode.svelte';
    import type { ScannedGenre, ScannedSong, SongTreeFolder, SoundtrackEntry } from '$lib/types';

    interface Props {
        Songs?: ScannedSong[];
        /** Scanned genre folders keyed by relative path. */
        Genres?: Record<string, ScannedGenre>;
        CatalogById?: Map<string, SoundtrackEntry>;
    }

    let { Songs = [], Genres = {}, CatalogById = new Map() }: Props = $props();

    const makeNode = (name: string): SongTreeFolder => ({ name, title: null, children: new Map(), songs: [], count: 0 });

    const nodeFor = (root: SongTreeFolder, relPath: string): SongTreeFolder => {
        if (!relPath) return root;
        let node = root;
        for (const part of relPath.split('/')) {
            let child = node.children.get(part);
            if (!child) {
                child = makeNode(part);
                node.children.set(part, child);
            }
            node = child;
        }
        return node;
    };

    const countSongs = (node: SongTreeFolder): number => {
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
