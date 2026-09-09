// Registry of managed OpenTaiko instances, persisted in the Hub preferences folder.
// See `Instance` in $lib/types for the record shape; channel === null means no build
// has been installed into the folder yet.
import { writable, derived, get } from 'svelte/store';
import { readTextFile, writeTextFile, mkdir, exists } from '@tauri-apps/plugin-fs';
import { join, basename } from '@tauri-apps/api/path';
import { invoke } from '@tauri-apps/api/core';
import { GetPreferencesPath, GetRootPath, GetGlobalSongsPath, GetLegacyInstancePath } from '../utils/path';
import { isVersionInSeries } from '../utils/versions';
import type { Instance, InstanceRegistry } from '$lib/types';

const REGISTRY_FILE = 'instances.json';

// Only the 0.6.0.x series keeps its Favorite / Recent / Search boxes as box.def folders
// inside the instance's own Songs folder, so only those instances need that folder in
// TJAPath. Every other version (0.6.1 experimental today, the official 0.6.1, and
// anything later such as a direct 0.6.0 to 0.7.0 update) reads the shared library alone.
// Matching the series rather than a "below 0.6.1" range keeps this exact.
const LOCAL_BOXES_SERIES: [number, number, number] = [0, 6, 0];

export const instances = writable<Instance[]>([]);
export const activeInstanceId = writable<string | null>(null);
export const activeInstance = derived(
    [instances, activeInstanceId],
    ([$instances, $id]) => $instances.find((inst) => inst.id === $id) ?? null
);
// id → version string read from the game binary (null = no build detected)
export const instanceVersions = writable<Record<string, string | null>>({});
export const instancesReady = writable(false);

const registryPath = async (): Promise<string> => join(await GetPreferencesPath(), REGISTRY_FILE);

const normalizePath = (p: string | null | undefined): string => {
    const normalized = (p ?? '').replace(/\//g, '\\').replace(/[\\]+$/, '');
    // Only Windows paths are case-insensitive; Linux paths must keep their case
    const isWindowsPath = /^[a-z]:\\/i.test(normalized) || normalized.startsWith('\\\\');
    return isWindowsPath ? normalized.toLowerCase() : normalized;
};

const persist = async (): Promise<void> => {
    const prefsDir = await GetPreferencesPath();
    await mkdir(prefsDir, { recursive: true });
    const data: InstanceRegistry = {
        version: 1,
        activeId: get(activeInstanceId),
        instances: get(instances)
    };
    await writeTextFile(await registryPath(), JSON.stringify(data, null, 2));
};

export interface ResolveInstancesInput {
    storedList: unknown;
    storedActiveId: string | null;
    legacyPath: string | null;
    pathExists: (path: string) => Promise<boolean>;
    makeInstance: (path: string) => Instance;
}

export interface ResolvedInstances {
    list: Instance[];
    activeId: string | null;
    adopted: boolean;
}

// Decides the instance list and the selected instance from what was stored plus what
// is on disk. Kept pure (all IO passed in) so the upgrade paths can be tested.
//
// The registry lives in the shared app config folder, so it can already exist (written
// by another Hub build using the same identifier) while this install's own game folder
// has never been added. Adoption therefore runs on every load, not only when the
// registry is missing, which is what made an existing OpenTaiko install invisible after
// updating the Hub.
export const resolveInstances = async ({ storedList, storedActiveId, legacyPath, pathExists, makeInstance }: ResolveInstancesInput): Promise<ResolvedInstances> => {
    let list: Instance[] = Array.isArray(storedList) ? (storedList as Instance[]) : [];

    let adopted: Instance | null = null;
    const known = new Set(list.map((inst) => normalizePath(inst.path)));
    if (legacyPath && !known.has(normalizePath(legacyPath)) && (await pathExists(legacyPath))) {
        adopted = makeInstance(legacyPath);
        list = [...list, adopted];
    }

    // A freshly adopted folder is this install's own game, so select it. Otherwise keep
    // the stored selection, falling back to an instance that still exists on disk.
    let active: Instance | null = adopted ?? list.find((inst) => inst.id === storedActiveId) ?? null;
    if (!active && list.length > 0) {
        for (const inst of list) {
            if (await pathExists(inst.path)) {
                active = inst;
                break;
            }
        }
        active ??= list[0];
    }

    return { list, activeId: active?.id ?? null, adopted: !!adopted };
};

export const loadInstances = async (): Promise<void> => {
    let storedList: unknown = [];
    let storedActiveId: string | null = null;
    let registryFound = false;
    try {
        const content = await readTextFile(await registryPath());
        const data = JSON.parse(content) as Partial<InstanceRegistry>;
        storedList = Array.isArray(data.instances) ? data.instances : [];
        storedActiveId = data.activeId ?? null;
        registryFound = true;
    } catch {
        // No registry yet: first run of 0.2, or a fresh install
    }

    let legacyPath: string | null = null;
    try {
        legacyPath = await GetLegacyInstancePath();
    } catch (error) {
        console.error('Legacy instance detection failed:', error);
    }

    const resolved = await resolveInstances({
        storedList,
        storedActiveId,
        legacyPath,
        pathExists: async (p) => {
            try {
                return await exists(p);
            } catch {
                return false;
            }
        },
        makeInstance: (path) => ({
            id: crypto.randomUUID(),
            name: 'OpenTaiko',
            path,
            channel: 'stable',
            experimental: null,
            createdAt: new Date().toISOString()
        })
    });

    instances.set(resolved.list);
    activeInstanceId.set(resolved.activeId);
    if (!registryFound || resolved.adopted || resolved.activeId !== storedActiveId) await persist();
    instancesReady.set(true);
};

export const setActiveInstance = async (id: string): Promise<void> => {
    activeInstanceId.set(id);
    await persist();
};

export const updateInstance = async (id: string, patch: Partial<Instance>): Promise<Instance | null> => {
    instances.update((list) => list.map((inst) => (inst.id === id ? { ...inst, ...patch } : inst)));
    await persist();
    return get(instances).find((inst) => inst.id === id) ?? null;
};

export const createInstance = async (name: string | null | undefined): Promise<Instance> => {
    const root = await GetRootPath();
    const folderName = `OpenTaiko-${crypto.randomUUID()}`;
    const path = await join(root, folderName);
    await mkdir(path, { recursive: true });
    const instance: Instance = {
        id: crypto.randomUUID(),
        name: name?.trim() || folderName,
        path,
        channel: null,
        experimental: null,
        createdAt: new Date().toISOString()
    };
    instances.update((list) => [...list, instance]);
    activeInstanceId.set(instance.id);
    await persist();
    return instance;
};

export const attachInstance = async (path: string, name: string | null = null): Promise<Instance> => {
    const existing = get(instances).find((inst) => normalizePath(inst.path) === normalizePath(path));
    if (existing) {
        activeInstanceId.set(existing.id);
        await persist();
        return existing;
    }
    const instance: Instance = {
        id: crypto.randomUUID(),
        name: name?.trim() || (await basename(path)),
        path,
        channel: null,
        experimental: null,
        createdAt: new Date().toISOString()
    };
    instances.update((list) => [...list, instance]);
    activeInstanceId.set(instance.id);
    await persist();
    return instance;
};

// Removes the instance from the registry only; no files are deleted.
export const detachInstance = async (id: string): Promise<void> => {
    instances.update((list) => list.filter((inst) => inst.id !== id));
    if (get(activeInstanceId) === id) {
        activeInstanceId.set(get(instances)[0]?.id ?? null);
    }
    await persist();
};

export const refreshInstanceVersion = async (id: string): Promise<string | null> => {
    const instance = get(instances).find((inst) => inst.id === id);
    if (!instance) return null;
    let version: string | null = null;
    try {
        version = await invoke<string | null>('get_game_version', { instanceDir: instance.path });
    } catch (error) {
        console.error('get_game_version failed:', error);
    }
    if (!version) {
        // Fallback: version.json written by pre-0.2 Hub versions
        try {
            const raw = await readTextFile(await join(instance.path, 'version.json'));
            version = (JSON.parse(raw) as { version?: string }).version ?? null;
        } catch {
            version = null;
        }
    }
    instanceVersions.update((map) => ({ ...map, [id]: version }));
    // A build without recorded channel information can only be a stable/manual install
    if (version && !instance.channel) {
        await updateInstance(id, { channel: 'stable' });
    }
    return version;
};

// Points the instance's Config.ini ([System] TJAPath) at the shared Songs library.
// Only 0.6.0.x instances also keep their own Songs folder in TJAPath, because that is
// where their Favorite / Recent / Search boxes live (see LOCAL_BOXES_SERIES).
export const linkGlobalSongs = async (instance: Instance | null | undefined): Promise<void> => {
    if (!instance) return;
    try {
        let version = get(instanceVersions)[instance.id] ?? null;
        if (!version) version = await refreshInstanceVersion(instance.id);
        // An unreadable version keeps the local folder: it never removes something a
        // 0.6.0 install needs, and since installs no longer ship charts locally the
        // extra path cannot introduce duplicates. The link is redone on every startup.
        const includeLocal = version === null
            ? true
            : isVersionInSeries(version, ...LOCAL_BOXES_SERIES);

        const globalSongs = await GetGlobalSongsPath();
        await mkdir(globalSongs, { recursive: true });
        await invoke('ensure_config_tjapath', {
            instanceDir: instance.path,
            globalSongsPath: globalSongs,
            includeLocal
        });
    } catch (error) {
        console.error('Failed to link the global Songs folder:', error);
    }
};
