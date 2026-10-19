<script lang="ts">
    // Column labels above a list of song rows, so each level sits under the course it
    // belongs to. Laid out exactly like SongRow: a dot, the names, five level cells and
    // the state column. Charts with a Tower or Dan level use the first cell, whose own
    // tooltip says which.
    import { _ } from 'svelte-i18n';
    import { MAIN_COURSES } from '$lib/utils/library';

    const SHORT: Record<string, string> = { Easy: 'EZ', Normal: 'NM', Hard: 'HD', Oni: 'EX', Edit: 'EXEX' };
</script>

<div class="head" aria-hidden="true">
    <span class="mark"></span>
    <span class="names"></span>
    <span class="levels">
        {#each MAIN_COURSES as course (course)}
            <span class="cell" title={$_(`songs.course.${course.toLowerCase()}`)}>{SHORT[course] ?? course}</span>
        {/each}
    </span>
    <span class="state"></span>
</div>

<style>
    /* Mirrors SongRow's widths; a change there belongs here too */
    .head {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        padding: 0.1rem 0.5rem;
        border: 1px solid transparent;
        font-size: 0.62rem;
        font-weight: 700;
        letter-spacing: 0.02em;
        opacity: 0.6;
    }
    .mark {flex: none; width: 0.5rem;}
    .names {flex: 1;}
    .levels {flex: none; display: flex; gap: 0.15rem;}
    .cell {width: 2.1rem; text-align: center;}
    .state {flex: none; width: 9rem;}
</style>
