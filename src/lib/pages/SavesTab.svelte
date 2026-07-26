<script>
    // Per-save export / import for the active instance's Saves.db3.
    //
    // Each save (a slot identified by its player name) is exported with all of its
    // attached data — scores, unlocks, coins, counters, dan titles, nameplates — into a
    // portable .optksave file. On import, if the archive's SaveUID matches a save already
    // present it is MERGED into it (progress is never regressed); otherwise it is ADDED
    // as a new save. Imports are only accepted when the archive is the same version as,
    // or newer than, the instance's current save database.
    import { getContext } from 'svelte';
    import { readFile, writeFile, exists } from '@tauri-apps/plugin-fs';
    import { save as saveDialog, open as openDialog } from '@tauri-apps/plugin-dialog';
    import { path } from '@tauri-apps/api';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import { getSQL } from '$lib/utils/sqljs.js';
    import { compareVersions } from '$lib/utils/versions.js';
    import { readDbVersion, listSaves, exportSave, importSave, rebindSlot } from '$lib/utils/savesdb.js';
    import { activeInstance } from '$lib/stores/instances.js';

    const { TriggerError, TriggerSuccess, TriggerWarning } = getContext('toast');

    const DB_NAME = 'Saves.db3';

    let saves = [];
    let currentVersion = null;
    let loading = false;
    let loadError = null;
    let busy = false;
    let loadedFor = null;
    let loadToken = 0; // guards against out-of-order reloads when switching instances

    $: instance = $activeInstance;
    // Reload the save DB whenever the active instance changes (refresh on the spot).
    $: if (instance && loadedFor !== instance.id) {
        loadedFor = instance.id;
        Load(instance);
    }

    const dbPath = (inst) => path.join(inst.path, DB_NAME);

    // No step of a load may hang forever: a stuck await would leave the tab spinning
    // with nothing to click, so every load is bounded and always ends in a state the
    // user can act on.
    const withTimeout = (promise, ms, label) => Promise.race([
        promise,
        new Promise((_, reject) => setTimeout(() => reject(new Error(`${label} timed out`)), ms))
    ]);

    // Loads a specific instance's DB. Captures a token so a slow reload for a previous
    // instance can never overwrite the current one's data. `silent` keeps the current
    // rows on screen while re-reading (no placeholder, no layout shift).
    const Load = async (inst, silent = false) => {
        if (!inst) return;
        const token = ++loadToken;
        if (!silent) {
            loading = true;
            saves = [];
            currentVersion = null;
        }
        loadError = null;
        let db = null;
        try {
            const p = await withTimeout(dbPath(inst), 15000, 'path resolution');
            if (await withTimeout(exists(p), 15000, 'file check')) {
                const SQL = await withTimeout(getSQL(), 30000, 'sql.js initialisation');
                const bytes = await withTimeout(readFile(p), 30000, 'reading Saves.db3');
                db = new SQL.Database(bytes);
                const version = readDbVersion(db);
                const list = listSaves(db);
                if (token === loadToken) {
                    currentVersion = version;
                    saves = list;
                }
            }
        } catch (error) {
            console.error('Failed to read the save database:', error);
            if (token === loadToken) loadError = String(error?.message ?? error);
        } finally {
            db?.close();
        }
        if (token === loadToken) loading = false;
    };

    const stripV = (v) => (v ?? '').replace(/^v/i, '');

    const Export = async (entry) => {
        if (busy || !instance) return;
        const inst = instance;
        let db = null;
        try {
            const p = await dbPath(inst);
            if (!(await exists(p))) {
                TriggerError(get(_)('saves.error.no_db'));
                return;
            }
            const SQL = await getSQL();
            db = new SQL.Database(await readFile(p));
            const portable = exportSave(db, entry.saveId);
            const suggested = `OpenTaiko-${entry.name}.optksave`.replace(/[\\/:*?"<>|]/g, '_');
            const dest = await saveDialog({
                defaultPath: suggested,
                filters: [{ name: 'OpenTaiko Save', extensions: ['optksave'] }]
            });
            if (!dest) return;
            busy = true;
            await writeFile(dest, new TextEncoder().encode(JSON.stringify(portable)));
            TriggerSuccess(get(_)('saves.success.export', { values: { name: entry.name } }));
        } catch (error) {
            TriggerError(get(_)('saves.error.export', { values: { error } }));
        } finally {
            db?.close();
            busy = false;
        }
    };

    const Import = async () => {
        if (busy || !instance) return;
        const inst = instance;
        let db = null;
        try {
            const src = await openDialog({
                multiple: false,
                filters: [{ name: 'OpenTaiko Save', extensions: ['optksave', 'json'] }]
            });
            if (!src) return;

            let portable;
            try {
                portable = JSON.parse(new TextDecoder().decode(await readFile(src)));
            } catch {
                TriggerError(get(_)('saves.error.invalid_file'));
                return;
            }
            if (portable?.format !== 'optk-save' || !portable.save) {
                TriggerError(get(_)('saves.error.invalid_file'));
                return;
            }

            busy = true;
            const p = await dbPath(inst);
            if (!(await exists(p))) {
                TriggerError(get(_)('saves.error.no_db'));
                return;
            }
            const SQL = await getSQL();
            db = new SQL.Database(await readFile(p));

            // Version gate: the archive must be same-or-newer than the instance DB
            const dbVersion = readDbVersion(db);
            const cmp = compareVersions(stripV(portable.dbVersion ?? dbVersion), stripV(dbVersion));
            if (cmp !== null && cmp < 0) {
                TriggerError(get(_)('saves.error.older', {
                    values: { imported: portable.dbVersion, current: dbVersion }
                }));
                return;
            }

            const result = importSave(db, portable, crypto.randomUUID());
            await writeFile(p, db.export());
            await Load(inst);

            if (result.mode === 'merge') {
                TriggerSuccess(get(_)('saves.success.merge', { values: { name: result.name } }));
            } else if (result.slot === null) {
                TriggerWarning(get(_)('saves.success.add_reserve', { values: { name: result.name } }));
            } else {
                TriggerSuccess(get(_)('saves.success.add', { values: { name: result.name, slot: result.slot } }));
            }
        } catch (error) {
            TriggerError(get(_)('saves.error.import', { values: { error } }));
        } finally {
            db?.close();
            busy = false;
        }
    };

    // Rebinds a save to another slot; an occupied target slot swaps the two saves
    // (e.g. moving P5 to Slot 1 puts the previous Slot 1 save into P5's old place).
    // The rows are updated in place (no reload) so the layout never shifts.
    const Rebind = async (entry, event) => {
        const targetSlot = Number(event.target.value);
        const fromSlot = entry.slot;
        let db = null;
        busy = true;
        try {
            const p = await dbPath(instance);
            if (!(await exists(p))) {
                TriggerError(get(_)('saves.error.no_db'));
                return;
            }
            const SQL = await getSQL();
            db = new SQL.Database(await readFile(p));
            const result = rebindSlot(db, entry.saveId, targetSlot);
            if (result.changed) {
                await writeFile(p, db.export());
                // Reflect the swap on the rows themselves, keeping their order stable
                saves = saves.map((s) => {
                    if (s.saveId === entry.saveId) return { ...s, slot: targetSlot };
                    if (result.swappedWithId !== null && s.saveId === result.swappedWithId) return { ...s, slot: fromSlot };
                    return s;
                });
                if (result.swapped) {
                    TriggerSuccess(get(_)('saves.success.rebind_swap', { values: { a: result.name, b: result.swappedWith } }));
                } else {
                    TriggerSuccess(get(_)('saves.success.rebind', { values: { name: result.name } }));
                }
            }
        } catch (error) {
            TriggerError(get(_)('saves.error.rebind', { values: { error } }));
            // Resync quietly so the selectors match the real layout, without the
            // loading placeholder that would shift the layout
            await Load(instance, true);
        } finally {
            db?.close();
            busy = false;
        }
    };
</script>

<section class="card w-full">
    <div class="p-4 space-y-4">
        <div>
            <h3 class="h3">{$_('saves.title')}</h3>
            <p class="opacity-70">{$_('saves.description')}</p>
        </div>

        <div class="flex gap-3 items-center flex-wrap">
            <span class="whitespace-nowrap"><b>{$_('saves.current_version')}</b></span>
            {#if loading}
                <div class="placeholder animate-pulse flex-1 max-w-xs" />
            {:else if loadError}
                <span class="text-red-500">{$_('saves.error.load')}</span>
            {:else if currentVersion}
                <span class="badge variant-filled-primary">{currentVersion}</span>
            {:else}
                <span class="opacity-70">{$_('saves.no_saves')}</span>
            {/if}
            <span class="flex-1"></span>
            <!-- Never disabled by `loading`: the tab must always be recoverable -->
            <button type="button" class="button-blue button-main" disabled={busy} on:click={() => Load(instance)}>
                <i class="fa-solid fa-rotate"></i> {$_('common.reload')}
            </button>
            <button type="button" class="button-blue button-main" disabled={busy} on:click={Import}>
                <i class="fa-solid fa-file-import"></i> {$_('saves.import')}
            </button>
        </div>

        {#if loadError}
            <p class="text-sm opacity-70">{loadError}</p>
        {/if}

        {#if !loading && saves.length > 0}
        <div class="table-container">
            <table class="table table-hover">
                <thead>
                    <tr>
                        <th>{$_('saves.col.player')}</th>
                        <th>{$_('saves.col.slot')}</th>
                        <th>{$_('saves.col.uid')}</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {#each saves as entry (entry.saveId)}
                    <tr>
                        <td>{entry.name}</td>
                        <td>
                            <select
                                class="select slot-select"
                                value={entry.slot === null ? 'reserve' : String(entry.slot)}
                                disabled={busy}
                                title={$_('saves.rebind_hint')}
                                on:change={(e) => Rebind(entry, e)}
                            >
                                <!-- Reserve is only a display state for reserve saves, never a
                                     target: the game needs all 5 slots to exist and stay unique -->
                                {#if entry.slot === null}
                                    <option value="reserve" disabled>{$_('saves.slot_reserve')}</option>
                                {/if}
                                {#each [0, 1, 2, 3, 4] as s}
                                    <option value={String(s)}>{$_('saves.slot_n', { values: { n: s + 1 } })}</option>
                                {/each}
                            </select>
                        </td>
                        <td class="uid-cell" title={entry.saveUid}>{entry.saveUid ? entry.saveUid.slice(0, 8) : '—'}</td>
                        <td class="text-right">
                            <button type="button" class="button-green button-main" disabled={busy} on:click={() => Export(entry)}>
                                <i class="fa-solid fa-file-export"></i> {$_('saves.export')}
                            </button>
                        </td>
                    </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        {/if}

        <p class="text-sm opacity-70"><i class="fa-solid fa-circle-info"></i> {$_('saves.hint')}</p>
    </div>
</section>

<style>
    .uid-cell {
        font-family: monospace;
        font-size: 0.8rem;
        opacity: 0.75;
    }
    .slot-select {
        width: auto;
        min-width: 7rem;
        padding: 0.25rem 0.5rem;
    }
</style>
