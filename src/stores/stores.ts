import { localStorageStore } from '@skeletonlabs/skeleton';
import type { Writable } from 'svelte/store';

export const settings: Writable<App.Settings> = localStorageStore('settings', {theme: 0,});