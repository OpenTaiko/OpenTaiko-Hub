<script lang="ts">
    // The panel for a selected collection: its box.def, its folder actions, and the drop
    // target for chart archives. A collection the soundtrack offers but that has never
    // been downloaded has no box.def to show, so it says so instead of showing an empty
    // editor that would write a file into a folder that does not exist.
    import { _ } from 'svelte-i18n';
    import BoxDefEditor from './BoxDefEditor.svelte';
    import { formatSize } from '$lib/utils/library';
    import type { LibraryFolder } from '$lib/utils/library';

    interface Props {
        Node: LibraryFolder;
        /** Raw box.def text, or null while it loads. */
        BoxText?: string | null;
        BoxFileExists?: boolean;
        Saving?: boolean;
        Busy?: boolean;
        /** True while an archive is being dragged over the panel. */
        DropActive?: boolean;
        Importing?: boolean;
        /** Official soundtrack collections are managed by the Hub, not edited by hand. */
        Official?: boolean;
        /** The library scan is still running, so an empty folder is not proof of anything. */
        Scanning?: boolean;
        OnSaveBox?: (text: string) => void;
        OnOpenFolder?: (relPath: string) => void;
        OnGetFolder?: (folder: LibraryFolder) => void;
        OnNewFolder?: (parentRel: string) => void;
        OnDelete?: (folder: LibraryFolder) => void;
        OnPickArchives?: (relPath: string) => void;
        OnPickJacket?: (relPath: string) => void;
        JacketUrl?: string | null;
    }

    let {
        Node,
        BoxText = null,
        BoxFileExists = false,
        Saving = false,
        Busy = false,
        DropActive = false,
        Importing = false,
        Official = false,
        Scanning = false,
        OnSaveBox,
        OnOpenFolder,
        OnGetFolder,
        OnNewFolder,
        OnDelete,
        OnPickArchives,
        OnPickJacket,
        JacketUrl = null
    }: Props = $props();

    let missing = $derived(Math.max(0, Node.totalCount - Node.installedCount));
</script>

<aside class="detail card">
    <div class="head">
        <h3>{Node.label}</h3>
        <p class="raw">{Node.rawName}</p>
        <p class="counts">
            {$_('songs.tree.installed_of', { values: { installed: Node.installedCount, total: Node.totalCount } })}
            {#if Node.pendingMb > 0}· {formatSize(Node.pendingMb)}{/if}
        </p>
    </div>

    {#if !Node.onDisk}
        <!-- Nothing on disk: there is no box.def, and nothing to edit or delete. While
             the scan is still running the Hub does not yet know that, so it says so. -->
        <p class="notice">
            {Scanning ? $_('songs.folder.loading') : $_('songs.folder.not_downloaded')}
        </p>
        {#if missing > 0 && OnGetFolder}
            <button type="button" class="button-green button-main" disabled={Busy} onclick={() => OnGetFolder?.(Node)}>
                <i class="fa-solid fa-download"></i>
                {$_('songs.tree.get_n', { values: { count: missing } })}
                <span class="dim">{formatSize(Node.pendingMb)}</span>
            </button>
        {/if}
    {:else}
        <div class="actions">
            <button type="button" class="button-blue button-main" onclick={() => OnOpenFolder?.(Node.relPath)}>
                <i class="fa-solid fa-folder-open"></i> {$_('songs.detail.open_song_folder')}
            </button>
            {#if Node.canAddFolder}
                <button type="button" class="button-blue button-main" onclick={() => OnNewFolder?.(Node.relPath)}>
                    <i class="fa-solid fa-folder-plus"></i> {$_('songs.folder.new_subfolder')}
                </button>
            {/if}
            {#if !Official && OnDelete}
                <button type="button" class="button-red button-main" disabled={Busy} onclick={() => OnDelete?.(Node)}>
                    <i class="fa-solid fa-trash"></i> {$_('songs.folder.delete')}
                </button>
            {/if}
        </div>

        {#if !Node.official}
            <div class="jacket">
                <div class="thumb">
                    {#if JacketUrl}
                        <img src={JacketUrl} alt="" onerror={(e) => (e.currentTarget as HTMLImageElement).style.display = 'none'} />
                    {:else}
                        <i class="fa-solid fa-image"></i>
                    {/if}
                </div>
                <div class="jacketside">
                    <p class="jackettitle">{$_('songs.box.jacket')}</p>
                    <p class="dim">{$_('songs.box.jacket_hint')}</p>
                    <button type="button" class="button-blue button-main" onclick={() => OnPickJacket?.(Node.relPath)}>
                        <i class="fa-solid fa-upload"></i> {$_('songs.box.jacket_pick')}
                    </button>
                </div>
            </div>
        {/if}

        {#if Node.canAddCharts}
            <!-- Archives land here. The zone is always visible so it can be discovered
                 without having to drag something first to find out it exists. -->
            <div class="drop" class:hot={DropActive} class:busy={Importing}>
                <i class="fa-solid fa-file-zipper"></i>
                <p>{Importing ? $_('songs.folder.importing') : $_('songs.folder.drop_hint')}</p>
                <button type="button" class="button-blue button-main" disabled={Importing} onclick={() => OnPickArchives?.(Node.relPath)}>
                    {$_('songs.folder.browse_zip')}
                </button>
            </div>
        {/if}

        <BoxDefEditor
            Text={BoxText}
            FileExists={BoxFileExists}
            {Saving}
            ReadOnly={Official}
            OnSave={(text) => OnSaveBox?.(text)}
        />
    {/if}

    <p class="path" title={Node.relPath}>{Node.relPath}</p>
</aside>

<style>
    .detail {
        position: sticky;
        top: 0;
        max-height: calc(100dvh - 5rem);
        display: flex;
        flex-direction: column;
        gap: 0.7rem;
        padding: 0.85rem;
        overflow-y: auto;
        min-height: 0;
    }
    @media (max-width: 1000px) {
        .detail {position: static; max-height: none;}
    }

    .head h3 {margin: 0; font-size: 1rem; overflow-wrap: anywhere;}
    .raw {font-size: 0.72rem; opacity: 0.6; margin: 0.15rem 0 0; user-select: text; overflow-wrap: anywhere;}
    .counts {font-size: 0.78rem; opacity: 0.85; margin: 0.3rem 0 0; font-variant-numeric: tabular-nums;}

    .notice {
        font-size: 0.85rem;
        padding: 0.6rem 0.7rem;
        border-radius: 0.4rem;
        background-color: color-mix(in oklab, var(--color-surface-500) 15%, transparent);
        margin: 0;
    }

    .actions {display: flex; gap: 0.35rem; flex-wrap: wrap;}
    .dim {opacity: 0.7; font-size: 0.75rem;}

    .drop {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.4rem;
        text-align: center;
        padding: 0.9rem 0.7rem;
        border: 2px dashed color-mix(in oklab, var(--color-surface-500) 45%, transparent);
        border-radius: 0.5rem;
        font-size: 0.78rem;
    }
    .drop i {font-size: 1.3rem; opacity: 0.55;}
    .drop p {margin: 0;}
    .drop.hot {
        border-color: var(--color-primary-500);
        background-color: color-mix(in oklab, var(--color-primary-500) 12%, transparent);
    }
    .drop.busy {opacity: 0.7;}

    .jacket {display: flex; gap: 0.6rem; align-items: flex-start;}
    .thumb {
        flex: none;
        width: 4rem;
        height: 4rem;
        border-radius: 0.4rem;
        overflow: hidden;
        display: grid;
        place-items: center;
        background-color: color-mix(in oklab, var(--color-surface-500) 20%, transparent);
    }
    .thumb img {width: 100%; height: 100%; object-fit: cover;}
    .jacketside {display: flex; flex-direction: column; gap: 0.25rem; align-items: flex-start; min-width: 0;}
    .jackettitle {margin: 0; font-size: 0.8rem; font-weight: 700;}

    .path {
        font-size: 0.7rem;
        opacity: 0.55;
        user-select: text;
        overflow-wrap: anywhere;
        margin: auto 0 0;
    }
</style>
