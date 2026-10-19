// Long operations in progress (installs, builds, bulk downloads), shared by the pages
// that run them and the parts of the UI that need to know: the navigation badge, the
// instance manager, and the backend, which asks before the window closes on one.
import { writable, derived, get } from 'svelte/store';
import { ipc } from '$lib/ipc';
import { _ } from 'svelte-i18n';

/** Which navigation tile shows the operation as running. */
export type ActivityArea = 'home' | 'songs';

interface Activity {
    id: string;
    area: ActivityArea;
    /** The instance the operation writes into, if any. */
    instanceId: string | null;
    /** Replaces the game's files (an install): nothing else may write into the instance
     *  meanwhile, and it may not start while anything else does. */
    exclusive: boolean;
}

const activities = writable<Activity[]>([]);

export const busyAreas = derived(activities, (list) => new Set(list.map((a) => a.area)));
export const busyInstanceIds = derived(activities, (list) => new Set(list.flatMap((a) => (a.instanceId ? [a.instanceId] : []))));

export interface Job {
    /** Passed to the backend commands of this operation, so they can be cancelled. */
    id: string;
    cancel: () => void;
    /** Always call once the operation is over, including when it failed. */
    end: () => void;
}

/**
 * True when what already runs in `instanceId` rules out a new operation there. An install
 * and a skin download used to write into the same instance at once, the install
 * replacing System/ while the skins were being written into it.
 */
export const InstanceInUse = (instanceId: string, exclusive: boolean): boolean =>
    get(activities).some((a) => a.instanceId === instanceId && (exclusive || a.exclusive));

export const startJob = (area: ActivityArea, instanceId: string | null = null, exclusive = false): Job => {
    const id = crypto.randomUUID();
    activities.update((list) => [...list, { id, area, instanceId, exclusive }]);
    const t = get(_);
    ipc.beginJob(id, {
        title: t('jobs.close_title'),
        message: t('jobs.close_message'),
        close: t('jobs.close'),
        keep: t('jobs.keep')
    }).catch((error) => console.warn('Could not register the operation:', error));
    let ended = false;
    return {
        id,
        cancel: () => {
            ipc.cancelJob(id).catch((error) => console.warn('Could not cancel:', error));
        },
        end: () => {
            if (ended) return;
            ended = true;
            activities.update((list) => list.filter((a) => a.id !== id));
            ipc.endJob(id).catch(() => {});
        }
    };
};

/** True for the error a command returns when its operation was cancelled. */
export const isCancelled = (error: unknown): boolean =>
    (error instanceof Error ? error.message : String(error)) === 'Cancelled';
