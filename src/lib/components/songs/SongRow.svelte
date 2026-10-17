<script lang="ts">
    // One song. Installed and not-yet-downloaded charts use the same row so the library
    // reads as one thing; "available" is carried by three signals at once — reduced
    // opacity, a dotted outline and the literal word — because opacity alone reads as
    // "disabled" and fails contrast on the darker themes.
    import { _ } from 'svelte-i18n';
    import { FormatLevel } from '$lib/utils/levels';
    import { MAIN_COURSES, formatSize } from '$lib/utils/library';
    import type { LibraryEntry } from '$lib/utils/library';
    import type { CourseName, SoundtrackEntry } from '$lib/types';

    interface Props {
        Entry: LibraryEntry;
        Selected?: boolean;
        /** 0-100 while this chart downloads. */
        Progress?: number;
        /** Blocked while the library is being scanned or a bulk run owns the tab. */
        Disabled?: boolean;
        OnSelect?: (entry: LibraryEntry) => void;
        OnGet?: (entry: SoundtrackEntry) => void;
    }

    let { Entry, Selected = false, Progress, Disabled = false, OnSelect, OnGet }: Props = $props();

    let towerLevel = $derived(Entry.levels.Tower);
    let danLevel = $derived(Entry.levels.Dan);
</script>

<!-- The song's name is the row's button, reached with Tab and Enter. The row as a whole
     is a button too only for the mouse: as a real one it would hold the Get button inside
     it, which assistive technology cannot represent. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
    class="row"
    class:available={!Entry.installed}
    class:selected={Selected}
    onclick={() => OnSelect?.(Entry)}
>
    <span class="mark" class:outdated={Entry.status === 'outdated'} class:custom={Entry.status === 'custom'}></span>

    <button type="button" class="names" aria-pressed={Selected} onclick={(e) => { e.stopPropagation(); OnSelect?.(Entry); }}>
        <span class="title" title={Entry.label}>{Entry.label}</span>
        {#if Entry.subtitle}<span class="sub" title={Entry.subtitle}>{Entry.subtitle}</span>{/if}
    </button>

    <!-- Fixed cells under the column labels (SongRowHeader), so a level always sits under
         its course. Each cell also names its course, for a tooltip and a screen reader. -->
    <span class="levels">
        {#if towerLevel !== undefined || danLevel !== undefined}
            <!-- One chip, coloured by kind: the words did not survive the column width. -->
            <span
                class="cell {towerLevel !== undefined ? 'c-Tower' : 'c-Dan'}"
                title="{towerLevel !== undefined ? $_('songs.course.tower') : $_('songs.course.dan')} {FormatLevel((towerLevel ?? danLevel) as number)}"
                aria-label="{towerLevel !== undefined ? $_('songs.course.tower') : $_('songs.course.dan')} {FormatLevel((towerLevel ?? danLevel) as number)}"
            >{FormatLevel((towerLevel ?? danLevel) as number)}</span>
        {:else}
            {#each MAIN_COURSES as course (course)}
                {@const level = Entry.levels[course as CourseName]}
                {#if level === undefined}
                    <span class="cell none" aria-hidden="true">–</span>
                {:else}
                    {@const label = `${$_(`songs.course.${course.toLowerCase()}`)} ${FormatLevel(level)}`}
                    <span class="cell c-{course}" title={label} aria-label={label}>{FormatLevel(level)}</span>
                {/if}
            {/each}
        {/if}
    </span>

    <span class="state">
        {#if Progress !== undefined}
            <span class="bar" title="{Math.round(Progress)}%"><span class="fill" style:width="{Progress}%"></span></span>
        {:else if Entry.status === 'available' && Entry.catalog}
            <button
                type="button"
                class="button-green button-main get"
                disabled={Disabled}
                onclick={(e) => { e.stopPropagation(); OnGet?.(Entry.catalog as SoundtrackEntry); }}
            >
                <i class="fa-solid fa-download"></i> {formatSize(Entry.downloadMb)}
            </button>
        {:else if Entry.status === 'outdated' && Entry.catalog}
            <button
                type="button"
                class="button-green button-main get"
                disabled={Disabled}
                onclick={(e) => { e.stopPropagation(); OnGet?.(Entry.catalog as SoundtrackEntry); }}
            >
                <i class="fa-solid fa-rotate"></i> {$_('songs.button.update')}
            </button>
        {:else if Entry.status === 'custom'}
            <span class="word">{$_('songs.tree.custom')}</span>
        {:else}
            <span class="word done"><i class="fa-solid fa-check"></i> {$_('songs.status.installed_short')}</span>
        {/if}
    </span>
</div>

<style>
    .row {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        padding: 0.3rem 0.5rem;
        border-radius: 0.375rem;
        border: 1px solid transparent;
        cursor: pointer;
        font-size: 0.85rem;
    }
    .row:hover {background-color: color-mix(in oklab, var(--color-primary-500) 12%, transparent);}
    .row.selected {
        background-color: color-mix(in oklab, var(--color-primary-500) 22%, transparent);
        border-color: color-mix(in oklab, var(--color-primary-500) 55%, transparent);
    }

    /* Not downloaded: dimmed, dotted, and labelled — three signals, not one. */
    .row.available {opacity: 0.62; border-style: dashed; border-color: color-mix(in oklab, var(--color-surface-500) 45%, transparent);}
    .row.available:hover {opacity: 1;}

    .mark {
        flex: none;
        width: 0.5rem;
        height: 0.5rem;
        border-radius: 999px;
        background-color: color-mix(in oklab, var(--color-surface-500) 55%, transparent);
    }
    .row:not(.available) .mark {background-color: #16a34a;}
    /* These must out-specify the rule above, or every installed chart reads as green. */
    .row:not(.available) .mark.outdated {background-color: #d97706;}
    .row:not(.available) .mark.custom {background-color: var(--color-primary-500);}

    .names {
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        line-height: 1.2;
        /* A button that looks like the text it was */
        padding: 0;
        border: 0;
        background: none;
        color: inherit;
        font: inherit;
        text-align: left;
        cursor: pointer;
    }
    .names:focus-visible {outline: 2px solid var(--color-primary-500); outline-offset: 2px; border-radius: 0.2rem;}
    .title {white-space: nowrap; overflow: hidden; text-overflow: ellipsis;}
    .sub {font-size: 0.72rem; opacity: 0.65; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;}

    .levels {flex: none; display: flex; gap: 0.15rem; align-items: center;}

    .cell {
        width: 2.1rem;
        text-align: center;
        padding: 0.1rem 0;
        border-radius: 0.25rem;
        font-size: 0.72rem;
        font-weight: 700;
        font-variant-numeric: tabular-nums;
    }
    .cell.none {opacity: 0.3; font-weight: 400;}

    /* Course colours follow the game's own difficulty colours, softened per mode so they
       stay legible on every theme instead of being fixed light-mode pastels. */
    .c-Easy   {background-color: light-dark(#cfe9fd, #1d3f5c); color: light-dark(#0b3c63, #cfe9fd);}
    .c-Normal {background-color: light-dark(#d4f5dd, #1d4a2c); color: light-dark(#10401f, #d4f5dd);}
    .c-Hard   {background-color: light-dark(#fdeec2, #4d3c0c); color: light-dark(#4a3705, #fdeec2);}
    .c-Oni    {background-color: light-dark(#fbd5d8, #5a1220); color: light-dark(#6b0b18, #fbd5d8);}
    .c-Edit   {background-color: light-dark(#e6dcfb, #36225c); color: light-dark(#331263, #e6dcfb);}

    .c-Tower {background-color: light-dark(#fddfb8, #5c3a0a); color: light-dark(#6b3405, #ffd9a6);}
    .c-Dan   {background-color: light-dark(#ccd5f5, #1b2a63); color: light-dark(#17265e, #ccd5f5);}

    /* Fixed so a row never reflows as it moves Get -> progress -> Installed. */
    .state {flex: none; width: 7rem; display: flex; justify-content: flex-end; align-items: center;}
    .get {white-space: nowrap;}
    .word {font-size: 0.72rem; opacity: 0.7; white-space: nowrap;}
    .word.done {color: #16a34a; opacity: 1;}

    .bar {
        display: block;
        width: 100%;
        height: 0.45rem;
        border-radius: 999px;
        overflow: hidden;
        background-color: color-mix(in oklab, var(--color-surface-500) 35%, transparent);
    }
    .fill {display: block; height: 100%; background-color: var(--color-primary-500);}
</style>
