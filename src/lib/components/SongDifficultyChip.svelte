<script lang="ts">
import { _ } from 'svelte-i18n';
import { get } from 'svelte/store';
import type { CourseName, SoundtrackEntry } from '$lib/types';

interface Props {
    SongInfo: SoundtrackEntry;
    Difficulty?: CourseName;
    OnCrownClick?: (song: SoundtrackEntry, difficulty: CourseName) => void;
}

let { SongInfo, Difficulty = "Easy", OnCrownClick = undefined }: Props = $props();

// Fractional levels: 10 and above use the Taiko "+" convention (10.5+ → "10+",
// 10.4 → "10"); below 10 the integer is shown.
const FormatLevel = (level: number): string => {
    const base = Math.floor(level);
    if (level >= 10 && level - base >= 0.5) return `${base}+`;
    return `${base}`;
};

const CHIP_COLORS: Record<CourseName, string> = {
    "Easy": "blue",
    "Normal": "green",
    "Hard": "yellow",
    "Oni": "red",
    "Edit": "purple",
    "Tower": "orange",
    "Dan": "blue"
};

let HoFRank = $derived(SongInfo.chartHoFRanks?.[Difficulty]);
let Level = $derived(SongInfo.chartDifficulties[Difficulty]);
let HoFCrownColorClass = $derived((HoFRank !== undefined && Level !== undefined)
    ? (HoFRank === 1 || Level >= 13)
        ? "text-yellow-500"
        : (HoFRank === 2 || Level >= 12)
            ? "text-zinc-400"
            : (HoFRank === 3 || Level >= 11)
                ? "text-amber-800"
                : "text-green-400"
    : "");
let ChipColor = $derived(CHIP_COLORS[Difficulty]);
let Prefix = $derived(["Easy", "Normal", "Hard", "Oni", "Edit"].includes(Difficulty) ? "★" : `${Difficulty} ★`);
let Maker = $derived(["Easy", "Normal", "Hard", "Oni", "Edit", "Tower"].includes(Difficulty) ? `${get(_)('songs.col.charter')}${SongInfo.chartMakers[Difficulty] ?? ''}` : undefined);
</script>


{#if Level !== undefined}
    <span class="badge bg-{ChipColor}-100 text-{ChipColor}-800 levelchip" title={Maker}>{Prefix}{FormatLevel(Level)}</span>
    {#if HoFRank !== undefined}
        <br /><br />
        <button class="hofrank" title="OpenTaiko Hall of Fame" onclick={() => OnCrownClick?.(SongInfo, Difficulty)}>
            <i class="fa-solid fa-crown {HoFCrownColorClass}"></i> <small class="text-black dark:text-white"><b>{HoFRank}</b></small>
        </button>
    {/if}
{/if}

<style>
    .levelchip {cursor: pointer;}
    .hofrank {text-decoration: none; background: none; border: none; cursor: pointer; padding: 0;}
</style>