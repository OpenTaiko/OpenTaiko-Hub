<script lang="ts">
    import { _ } from 'svelte-i18n';
    import { skinFit } from '$lib/utils/versions';

    interface Props {
        LatestVersion?: string;
        CurrentVersion?: string;
        /** "Error" marks an incompatible version in red, "Warning" in yellow, "None" not at all. */
        Strictness?: 'Error' | 'Warning' | 'None';
        /** The latest game release, when known: from 0.7.0.0, a version the instance does not
         *  have yet is marked in yellow while its game is behind it (skinFit). */
        LatestGameVersion?: string | null;
        /** The instance already has this version. */
        Installed?: boolean;
    }

    let { LatestVersion = "Unknown", CurrentVersion = "Unknown", Strictness = "Error", LatestGameVersion = null, Installed = true }: Props = $props();

    let fit = $derived(skinFit(LatestVersion, CurrentVersion, LatestGameVersion, Installed));
</script>

    {#if fit === 'fits'}
        <p class="badge bg-green-100 text-green-800">{LatestVersion}</p>
    {:else if fit === 'update_game' && Strictness !== "None"}
        <p class="badge bg-yellow-100 text-yellow-800" title={$_('version_chip.update_game')}>{LatestVersion}</p>
    {:else}
        {#if Strictness === "Error"}
            <!-- Error -->
            <p class="badge bg-red-100 text-red-800" title={$_('version_chip.incompatible')}>{LatestVersion}</p>
        {:else if Strictness === "None"}
            <p class="badge preset-tonal">{LatestVersion}</p>
        {:else}
            <!-- Warning -->
            <p class="badge bg-yellow-100 text-yellow-800" title={$_('version_chip.outdated')}>{LatestVersion}</p>
        {/if}
    {/if}

    <style>

    </style>
