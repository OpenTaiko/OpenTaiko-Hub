// Single toast queue for the whole app (Skeleton's <Toast.Group> is rendered once in
// the root layout and reads from this instance).
import { createToaster } from '@skeletonlabs/skeleton-svelte';

export const toaster = createToaster({ placement: 'bottom' });
