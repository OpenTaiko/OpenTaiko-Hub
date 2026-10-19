<script lang="ts">
    // Everything the old 9-column table held, for one song at a time: jacket, preview,
    // per-course levels with their charter and Hall of Fame rank, download size, where it
    // lives on disk, and the artist links.
    //
    // The AudioPlayer is mounted only while a song is selected, so the tab holds zero
    // <audio> elements by default instead of one per visible row.
    import { _ } from 'svelte-i18n';
    import AudioPlayer from '$lib/components/AudioPlayer.svelte';
    import { FormatLevel } from '$lib/utils/levels';
    import { SafeUrl } from '$lib/utils/sanitize';
    import { ALL_COURSES, formatSize } from '$lib/utils/library';
    import type { LibraryEntry } from '$lib/utils/library';
    import type { ArtistLinks, CourseName, SongArtists, SoundtrackEntry } from '$lib/types';

    interface Props {
        Entry: LibraryEntry | null;
        Artists?: SongArtists | null;
        /** uniqueId → global Hall of Fame rank per course. */
        HoFRanks?: Partial<Record<CourseName, number>>;
        Progress?: number;
        Disabled?: boolean;
        JacketUrl?: (entry: SoundtrackEntry) => string | null;
        /** Art and audio read off the disk for an installed chart. */
        LocalJacket?: string | null;
        LocalAudio?: string | null;
        /** Reads the installed chart's audio on first play (see AudioPlayer). */
        LoadAudio?: () => Promise<string | null>;
        /** Offered for an installed chart, which can carry its own #PREIMAGE. */
        OnPickPreimage?: (entry: LibraryEntry) => void;
        OnGet?: (entry: SoundtrackEntry) => void;
        OnOpenFolder?: (relPath: string) => void;
        OnHoF?: (entry: SoundtrackEntry, course: CourseName) => void;
        /** Offered only for a chart the soundtrack does not own. */
        OnDelete?: (entry: LibraryEntry) => void;
    }

    let {
        Entry,
        Artists = null,
        HoFRanks = {},
        Progress,
        Disabled = false,
        JacketUrl,
        LocalJacket = null,
        LocalAudio = null,
        LoadAudio,
        OnPickPreimage,
        OnGet,
        OnOpenFolder,
        OnHoF,
        OnDelete
    }: Props = $props();

    // Dan courses and Tower courses ship no jacket art and no preview audio, so the
    // panel shows neither rather than an empty frame and a dead play button.
    let isDan = $derived(Entry?.levels.Dan !== undefined);
    let isTower = $derived(Entry?.levels.Tower !== undefined);
    // Disk first: an installed chart shows the art the game itself shows, and a custom
    // chart has no repository entry to fall back to.
    let jacket = $derived(
        isDan || isTower
            ? null
            : LocalJacket ?? (Entry?.catalog && JacketUrl ? JacketUrl(Entry.catalog) : null)
    );

    // Playable from disk once installed, and from the repository while it is only on
    // offer. A Dan course is a list of other songs, so it has nothing of its own to play.
    let playable = $derived(!isDan && Entry && (LocalAudio || LoadAudio || (Entry.catalog && !Entry.installed)));
    let courses = $derived(Entry ? ALL_COURSES.filter((c) => Entry.levels[c] !== undefined) : []);

    const LINKS: { key: keyof ArtistLinks; icon: string; label: string }[] = [
        { key: 'youtube', icon: 'fa-brands fa-youtube', label: 'YouTube' },
        { key: 'soundcloud', icon: 'fa-brands fa-soundcloud', label: 'SoundCloud' },
        { key: 'spotify', icon: 'fa-brands fa-spotify', label: 'Spotify' },
        { key: 'bandcamp', icon: 'fa-brands fa-bandcamp', label: 'Bandcamp' },
        { key: 'bilibili', icon: 'fa-solid fa-tv', label: 'Bilibili' },
        // The rest are brand names, the same in every language
        { key: 'other', icon: 'fa-solid fa-link', label: '' }
    ];
</script>

<aside class="detail card">
    {#if !Entry}
        <!-- Nothing selected is the tab's chance to explain itself, rather than a blank. -->
        <div class="intro">
            <h3>{$_('songs.detail.help_title')}</h3>
            <ul class="legend">
                <li><span class="dot installed"></span>{$_('songs.detail.help_installed')}</li>
                <li><span class="dot available"></span>{$_('songs.detail.help_available')}</li>
                <li><span class="dot outdated"></span>{$_('songs.detail.help_outdated')}</li>
                <li><span class="dot custom"></span>{$_('songs.detail.help_custom')}</li>
            </ul>
            <p class="hint">{$_('songs.detail.help_hint')}</p>
        </div>
    {:else}
        <div class="head">
            {#if !isDan && !isTower}
                <div class="jacket">
                    {#if jacket}
                        <img src={jacket} alt="" loading="lazy" onerror={(e) => (e.currentTarget as HTMLImageElement).style.display = 'none'} />
                    {:else}
                        <i class="fa-solid fa-music"></i>
                    {/if}
                    <!-- Custom charts only: setting #PREIMAGE rewrites the .tja, which
                         would change a soundtrack chart's hash and strand it on
                         "Update" until a re-download undid the jacket anyway. -->
                    {#if Entry.status === 'custom' && OnPickPreimage}
                        <button
                            type="button"
                            class="setjacket"
                            title={$_('songs.detail.set_jacket')}
                            aria-label={$_('songs.detail.set_jacket')}
                            onclick={() => OnPickPreimage?.(Entry)}
                        >
                            <i class="fa-solid fa-camera"></i>
                        </button>
                    {/if}
                </div>
            {/if}
            <div class="names">
                <h3>{Entry.label}</h3>
                {#if Entry.subtitle}<p class="sub">{Entry.subtitle}</p>{/if}
                <p class="status">
                    {#if Entry.status === 'available'}
                        {$_('songs.status.not_downloaded')} · {formatSize(Entry.downloadMb)}
                    {:else if Entry.status === 'outdated'}
                        {$_('songs.status.outdated')}
                    {:else if Entry.status === 'custom'}
                        {$_('songs.tree.custom')}
                    {:else}
                        {$_('songs.status.up_to_date')}
                    {/if}
                </p>
            </div>
        </div>

        {#if playable}
            <!-- A custom song has no catalog entry: it plays from disk under its own name -->
            <AudioPlayer songInfo={Entry.catalog} Src={LocalAudio} LoadSrc={LoadAudio} Title={Entry.label} Subtitle={Entry.subtitle} />
        {/if}

        <div class="actions">
            {#if Progress !== undefined}
                <span class="bar"><span class="fill" style:width="{Progress}%"></span></span>
            {:else if Entry.catalog && (Entry.status === 'available' || Entry.status === 'outdated')}
                <button
                    type="button"
                    class="button-green button-main"
                    disabled={Disabled}
                    onclick={() => OnGet?.(Entry.catalog as SoundtrackEntry)}
                >
                    <i class="fa-solid {Entry.status === 'outdated' ? 'fa-rotate' : 'fa-download'}"></i>
                    {Entry.status === 'outdated'
                        ? $_('songs.button.update')
                        : $_('songs.detail.get_size', { values: { size: formatSize(Entry.downloadMb) } })}
                </button>
            {/if}
            {#if Entry.installed && OnOpenFolder}
                <button type="button" class="button-blue button-main" onclick={() => OnOpenFolder?.(Entry.relPath)}>
                    <i class="fa-solid fa-folder-open"></i> {$_('songs.detail.open_song_folder')}
                </button>
            {/if}
            {#if Entry.status === 'custom' && OnDelete}
                <button type="button" class="button-red button-main" disabled={Disabled} onclick={() => OnDelete?.(Entry)}>
                    <i class="fa-solid fa-trash"></i> {$_('songs.folder.delete')}
                </button>
            {/if}
        </div>

        {#if courses.length}
            <div class="courses">
                {#each courses as course (course)}
                    {@const rank = HoFRanks[course]}
                    <div class="course">
                        <span class="cname">{$_(`songs.course.${course.toLowerCase()}`)}</span>
                        <span class="clevel">{FormatLevel(Entry.levels[course] as number)}</span>
                        <span class="cmaker">{Entry.catalog?.chartMakers?.[course] ?? ''}</span>
                        {#if rank !== undefined && Entry.catalog}
                            <button type="button" class="crown" title={$_('hof.crown')} onclick={() => OnHoF?.(Entry.catalog as SoundtrackEntry, course)}>
                                <i class="fa-solid fa-crown"></i> {rank}
                            </button>
                        {/if}
                    </div>
                {/each}
            </div>
        {/if}

        {#if Artists?.artists?.length}
            <div class="artists">
                {#each Artists.artists as artist (artist.artist)}
                    <div class="artist">
                        <span class="who">{artist.artist}</span>
                        <span class="links">
                            {#each LINKS as link (link.key)}
                                {@const url = SafeUrl(artist[link.key])}
                                {#if url}
                                    <a href={url} target="_blank" rel="noreferrer" title={link.label || $_('songs.detail.website')} aria-label={link.label || $_('songs.detail.website')}>
                                        <i class={link.icon}></i>
                                    </a>
                                {/if}
                            {/each}
                        </span>
                    </div>
                {/each}
            </div>
        {/if}

        <p class="path" title={Entry.relPath}>{Entry.relPath}</p>
    {/if}
</aside>

<style>
    /* <main> is the tab's scroll container, so the panel stays put while the song list
       scrolls past it, and keeps its own scrollbar when a song has a lot of detail. */
    .detail {
        position: sticky;
        top: 0;
        max-height: calc(100dvh - 5rem);
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
        padding: 0.85rem;
        overflow-y: auto;
        min-height: 0;
    }

    /* Under ~1000px the panel sits below the list, where sticking would trap it. */
    @media (max-width: 1000px) {
        .detail {position: static; max-height: none;}
    }

    .intro h3 {margin-bottom: 0.5rem;}
    .legend {list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.35rem;}
    .legend li {display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; margin: 0;}
    .dot {width: 0.55rem; height: 0.55rem; border-radius: 999px; flex: none;}
    .dot.installed {background-color: #16a34a;}
    .dot.available {background-color: color-mix(in oklab, var(--color-surface-500) 55%, transparent);}
    .dot.outdated {background-color: #d97706;}
    .dot.custom {background-color: var(--color-primary-500);}
    .hint {font-size: 0.78rem; opacity: 0.7; margin-top: 0.6rem;}

    .head {display: flex; gap: 0.7rem; align-items: flex-start; min-width: 0;}
    .jacket {
        position: relative;
        flex: none;
        width: 5rem;
        height: 5rem;
        border-radius: 0.5rem;
        overflow: hidden;
        display: grid;
        place-items: center;
        background-color: color-mix(in oklab, var(--color-surface-500) 22%, transparent);
    }
    .jacket img {width: 100%; height: 100%; object-fit: cover;}

    /* Sits on the art rather than beside it: the panel's button row is for actions on
       the chart, and this one only ever means "replace this picture". */
    .setjacket {
        position: absolute;
        right: 0.2rem;
        bottom: 0.2rem;
        width: 1.5rem;
        height: 1.5rem;
        border: 0;
        border-radius: 0.3rem;
        cursor: pointer;
        font-size: 0.7rem;
        color: #fff;
        background-color: rgb(0 0 0 / 0.55);
        opacity: 0;
    }
    .jacket:hover .setjacket, .setjacket:focus-visible {opacity: 1;}
    .names {min-width: 0; flex: 1;}
    .names h3 {margin: 0; font-size: 1rem; overflow-wrap: anywhere;}
    .sub {font-size: 0.78rem; opacity: 0.7; margin: 0.1rem 0 0; overflow-wrap: anywhere; line-height: 1.35;}
    .status {font-size: 0.75rem; opacity: 0.8; margin: 0.35rem 0 0; overflow-wrap: anywhere;}

    .actions {display: flex; gap: 0.4rem; flex-wrap: wrap;}
    .bar {
        flex: 1;
        height: 0.5rem;
        border-radius: 999px;
        overflow: hidden;
        background-color: color-mix(in oklab, var(--color-surface-500) 35%, transparent);
    }
    .fill {display: block; height: 100%; background-color: var(--color-primary-500);}

    .courses {display: flex; flex-direction: column; gap: 0.2rem;}
    .course {
        display: grid;
        grid-template-columns: 4.5rem 2.5rem 1fr auto;
        align-items: center;
        gap: 0.4rem;
        font-size: 0.78rem;
        padding: 0.15rem 0;
        border-bottom: 1px solid color-mix(in oklab, var(--color-surface-500) 18%, transparent);
    }
    .cname {font-weight: 700; overflow-wrap: anywhere;}
    .clevel {font-variant-numeric: tabular-nums; text-align: right;}
    .cmaker {opacity: 0.7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;}
    .crown {background: none; border: 0; padding: 0; cursor: pointer; color: light-dark(#a16207, #eab308); font-size: 0.75rem; font-weight: 700;}

    .artists {display: flex; flex-direction: column; gap: 0.35rem;}
    .artist {display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; flex-wrap: wrap;}
    .who {font-weight: 700; overflow-wrap: anywhere;}
    .links {display: flex; gap: 0.45rem;}
    .links a {text-decoration: none;}

    .path {
        font-size: 0.7rem;
        opacity: 0.55;
        user-select: text;
        overflow-wrap: anywhere;
        margin: auto 0 0;
    }
</style>
