<script lang="ts">
    import { _ } from 'svelte-i18n';
    import { isSkinCompatible } from '$lib/utils/versions';

    interface Props {
        LatestVersion?: string;
        CurrentVersion?: string;
        /** "Error" marks an incompatible version in red, anything else in yellow. */
        Strictness?: 'Error' | 'Warning';
    }

    let { LatestVersion = "Unknown", CurrentVersion = "Unknown", Strictness = "Error" }: Props = $props();
</script>

    {#if isSkinCompatible(CurrentVersion, LatestVersion)}
        <p class="badge bg-green-100 text-green-800">{LatestVersion}</p>
    {:else}
        {#if Strictness === "Error"}
            <!-- Error -->
            <p class="badge bg-red-100 text-red-800" title={$_('version_chip.incompatible')}>{LatestVersion}</p>
        {:else}
            <!-- Warning -->
            <p class="badge bg-yellow-100 text-yellow-800" title={$_('version_chip.outdated')}>{LatestVersion}</p>
        {/if}
    {/if}

    <style>

    </style>
