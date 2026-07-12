// Registry of managed OpenTaiko instances, persisted in the Hub preferences folder.
// Each instance: { id, name, path, channel: 'stable' | 'experimental' | null,
//                  experimental: null | { kind: 'indev', branch, label, sha, builtAt }
//                               | { kind: 'prerelease', tag, label },
//                  createdAt }
// channel === null means no build has been installed into the folder yet.
import { writable, derived, get } from 'svelte/store';
import { readTextFile, writeTextFile, mkdir, exists } from '@tauri-apps/plugin-fs';
import { join, basename } from '@tauri-apps/api/path';
import { invoke } from '@tauri-apps/api/core';
import { GetPreferencesPath, GetRootPath, GetGlobalSongsPath, GetLegacyInstancePath } from '../utils/path.js';

const REGISTRY_FILE = 'instances.json';

export const instances = writable([]);
export const activeInstanceId = writable(null);
export const activeInstance = derived(
    [instances, activeInstanceId],
    ([$instances, $id]) => $instances.find((inst) => inst.id === $id) ?? null
);
// id → version string read from the game binary (null = no build detected)
export const instanceVersions = writable({});
export const instancesReady = writable(false);

const registryPath = async () => join(await GetPreferencesPath(), REGISTRY_FILE);

const normalizePath = (p) => {
    const normalized = (p ?? '').replace(/\//g, '\\').replace(/[\\]+$/, '');
    // Only Windows paths are case-insensitive; Linux paths must keep their case
    const isWindowsPath = /^[a-z]:\\/i.test(normalized) || normalized.startsWith('\\\\');
    return isWindowsPath ? normalized.toLowerCase() : normalized;
};

const persist = async () => {
    const prefsDir = await GetPreferencesPath();
    await mkdir(prefsDir, { recursive: true });
    const data = {
        version: 1,
        activeId: get(activeInstanceId),
        instances: get(instances)
    };
    await writeTextFile(await registryPath(), JSON.stringify(data, null, 2));
};

export const loadInstances = async () => {
    try {
        const content = await readTextFile(await registryPath());
        const data = JSON.parse(content);
        const list = Array.isArray(data.instances) ? data.instances : [];
        instances.set(list);
        const active = list.find((inst) => inst.id === data.activeId) ?? list[0] ?? null;
        activeInstanceId.set(active?.id ?? null);
    } catch {
        // First run of 0.2 (or fresh install): adopt the legacy single-instance folder
        const list = [];
        try {
            const legacyPath = await GetLegacyInstancePath();
            if (await exists(legacyPath)) {
                list.push({
                    id: crypto.randomUUID(),
                    name: 'OpenTaiko',
                    path: legacyPath,
                    channel: 'stable',
                    experimental: null,
                    createdAt: new Date().toISOString()
                });
            }
        } catch (error) {
            console.error('Legacy instance detection failed:', error);
        }
        instances.set(list);
        activeInstanceId.set(list[0]?.id ?? null);
        await persist();
    }
    instancesReady.set(true);
};

export const setActiveInstance = async (id) => {
    activeInstanceId.set(id);
    await persist();
};

export const updateInstance = async (id, patch) => {
    instances.update((list) => list.map((inst) => (inst.id === id ? { ...inst, ...patch } : inst)));
    await persist();
    return get(instances).find((inst) => inst.id === id) ?? null;
};

export const createInstance = async (name) => {
    const root = await GetRootPath();
    const folderName = `OpenTaiko-${crypto.randomUUID()}`;
    const path = await join(root, folderName);
    await mkdir(path, { recursive: true });
    const instance = {
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

export const attachInstance = async (path, name = null) => {
    const existing = get(instances).find((inst) => normalizePath(inst.path) === normalizePath(path));
    if (existing) {
        activeInstanceId.set(existing.id);
        await persist();
        return existing;
    }
    const instance = {
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
export const detachInstance = async (id) => {
    instances.update((list) => list.filter((inst) => inst.id !== id));
    if (get(activeInstanceId) === id) {
        activeInstanceId.set(get(instances)[0]?.id ?? null);
    }
    await persist();
};

export const refreshInstanceVersion = async (id) => {
    const instance = get(instances).find((inst) => inst.id === id);
    if (!instance) return null;
    let version = null;
    try {
        version = await invoke('get_game_version', { instanceDir: instance.path });
    } catch (error) {
        console.error('get_game_version failed:', error);
    }
    if (!version) {
        // Fallback: version.json written by pre-0.2 Hub versions
        try {
            const raw = await readTextFile(await join(instance.path, 'version.json'));
            version = JSON.parse(raw).version ?? null;
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
// Every instance is linked; experimental builds use ONLY the shared library (their
// bundled Songs folder is dropped from TJAPath), while stable installs keep their
// local Songs entry alongside it.
export const linkGlobalSongs = async (instance) => {
    if (!instance) return;
    try {
        const globalSongs = await GetGlobalSongsPath();
        await mkdir(globalSongs, { recursive: true });
        await invoke('ensure_config_tjapath', {
            instanceDir: instance.path,
            globalSongsPath: globalSongs,
            includeLocal: instance.channel !== 'experimental'
        });
    } catch (error) {
        console.error('Failed to link the global Songs folder:', error);
    }
};
