<script lang="ts">
    // Drop-in for Skeleton v2's <ProgressBar value max>, built on the v5 Progress
    // component. Keeps the v2 convention that an undefined value shows an
    // indeterminate bar (v5 expects null for that).
    import { Progress } from '@skeletonlabs/skeleton-svelte';
    import type { ComponentProps } from 'svelte';

    interface Props extends Omit<ComponentProps<typeof Progress>, 'value' | 'max' | 'children'> {
        /** Percentage of `max`; undefined or NaN renders an indeterminate bar. */
        value?: number | null;
        max?: number;
    }

    let { value = undefined, max = 100, ...rest }: Props = $props();

    let progressValue = $derived(
        value === undefined || value === null || Number.isNaN(value)
            ? null
            : Math.min(Math.max(value, 0), max)
    );
</script>

<Progress value={progressValue} {max} {...rest}>
    <Progress.Track>
        <Progress.Range />
    </Progress.Track>
</Progress>
