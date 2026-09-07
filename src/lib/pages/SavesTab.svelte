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
    import { save as saveDialog, open as openDialog, confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
    import { path } from '@tauri-apps/api';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import { getSQL } from '$lib/utils/sqljs.js';
    import { compareVersions } from '$lib/utils/versions.js';
    import { readDbVersion, listSaves, exportSave, importSaveInto, rebindSlot, createSave, deleteSave } from '$lib/utils/savesdb.js';
    import { activeInstance } from '$lib/stores/instances.js';

    const { TriggerError, TriggerSuccess, TriggerWarning } = getContext('toast');

    const DB_NAME = 'Saves.db3';

    let saves = $state([]);
    let currentVersion = $state(null);
    let loading = $state(false);
    let loadError = $state(null);
    let busy = $state(false);
    let loadedFor = null;   // bookkeeping only, never rendered
    let loadToken = 0; // guards against out-of-order reloads when switching instances


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

    const SlotLabel = (entry) => entry.slot === null
        ? get(_)('saves.slot_reserve')
        : get(_)('saves.slot_n', { values: { n: entry.slot + 1 } });

    // Runs a write against the instance DB and reloads the list without a layout shift
    const WithDb = async (action) => {
        if (busy || !instance) return null;
        const inst = instance;
        let db = null;
        busy = true;
        try {
            const p = await dbPath(inst);
            if (!(await exists(p))) {
                TriggerError(get(_)('saves.error.no_db'));
                return null;
            }
            const SQL = await getSQL();
            db = new SQL.Database(await readFile(p));
            const result = await action(db);
            if (result?.write !== false) await writeFile(p, db.export());
            await Load(inst, true);
            return result;
        } finally {
            db?.close();
            busy = false;
        }
    };

    const CreateSave = async () => {
        try {
            const result = await WithDb((db) => createSave(db, get(_)('saves.new_name'), crypto.randomUUID()));
            if (result) TriggerSuccess(get(_)('saves.success.created', { values: { name: result.name } }));
        } catch (error) {
            TriggerError(get(_)('saves.error.create', { values: { error } }));
        }
    };

    const DeleteSave = async (entry) => {
        // Only reserve saves can go: the game needs its 5 slots to stay filled
        if (entry.slot !== null) return;
        const confirmed = await confirmDialog(
            get(_)('saves.confirm_delete', { values: { name: entry.name } }),
            { title: get(_)('saves.confirm_delete_title'), kind: 'warning' }
        );
        if (!confirmed) return;
        try {
            const result = await WithDb((db) => deleteSave(db, entry.saveId));
            if (result?.deleted) TriggerSuccess(get(_)('saves.success.deleted', { values: { name: result.name } }));
        } catch (error) {
            TriggerError(get(_)('saves.error.delete', { values: { error } }));
        }
    };

    // Imports an archive into one chosen save. The destination is picked by the user
    // rather than derived from the archive's unique id, so a save exported on another
    // machine can be brought into any slot. The data is merged, never regressed.
    const Import = async (entry) => {
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

            // Confirm which save receives the import, so a mis-clicked row is caught
            const confirmed = await confirmDialog(
                get(_)('saves.confirm_import', {
                    values: {
                        slot: SlotLabel(entry),
                        current: entry.name,
                        imported: portable.save.PlayerName ?? '?'
                    }
                }),
                { title: get(_)('saves.confirm_title') }
            );
            if (!confirmed) return;

            busy = true;
            const p = await dbPath(inst);
            if (!(await exists(p))) {
                TriggerError(get(_)('saves.error.no_db'));
                return;
            }
            const SQL = await getSQL();
            db = new SQL.Database(await readFile(p));

            // Version gate: the game migrates older saves forward on load, but cannot
            // read one written by a newer version, so only a save NEWER than this
            // instance's database is refused.
            const dbVersion = readDbVersion(db);
            const cmp = compareVersions(stripV(portable.dbVersion ?? dbVersion), stripV(dbVersion));
            if (cmp !== null && cmp > 0) {
                TriggerError(get(_)('saves.error.newer', {
                    values: { imported: portable.dbVersion, current: dbVersion }
                }));
                return;
            }

            const result = importSaveInto(db, portable, entry.saveId, crypto.randomUUID());
            await writeFile(p, db.export());
            // Silent reload: refreshes the names without the layout shifting
            await Load(inst, true);
            TriggerSuccess(get(_)('saves.success.imported_into', {
                values: { name: result.name, slot: SlotLabel(entry) }
            }));
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
        if (busy) return;
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
    let instance = $derived($activeInstance);
    // Reload the save DB whenever the active instance changes (refresh on the spot).
    $effect(() => {
        if (instance && loadedFor !== instance.id) {
            loadedFor = instance.id;
            Load(instance);
        }
    });
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
                <div class="placeholder animate-pulse flex-1 max-w-xs"></div>
            {:else if loadError}
                <span class="text-red-500">{$_('saves.error.load')}</span>
            {:else if currentVersion}
                <span class="badge preset-filled-primary-500">{currentVersion}</span>
            {:else}
                <span class="opacity-70">{$_('saves.no_saves')}</span>
            {/if}
            <span class="flex-1"></span>
            <!-- Never disabled by `loading`: the tab must always be recoverable -->
            <button type="button" class="button-blue button-main" disabled={busy} onclick={() => Load(instance)}>
                <i class="fa-solid fa-rotate"></i> {$_('common.reload')}
            </button>
            <button type="button" class="button-green button-main" disabled={busy} onclick={CreateSave}>
                <i class="fa-solid fa-plus"></i> {$_('saves.create')}
            </button>
        </div>

        {#if loadError}
            <p class="text-sm opacity-70">{loadError}</p>
        {/if}

        {#if !loading && saves.length > 0}
        <div class="table-wrap">
            <table class="table table-hover">
                <thead>
                    <tr>
                        <th>{$_('saves.col.slot')}</th>
                        <th>{$_('saves.col.player')}</th>
                        <th>{$_('saves.col.uid')}</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {#each saves as entry (entry.saveId)}
                    <tr>
                        <td>
                            <select
                                class="select slot-select"
                                value={entry.slot === null ? 'reserve' : String(entry.slot)}
                                disabled={busy}
                                title={$_('saves.rebind_hint')}
                                onchange={(e) => Rebind(entry, e)}
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
                        <td>{entry.name}</td>
                        <td class="uid-cell" title={entry.saveUid}>{entry.saveUid ? entry.saveUid.slice(0, 8) : '-'}</td>
                        <td class="text-right whitespace-nowrap">
                            <button type="button" class="button-green button-main" disabled={busy} onclick={() => Export(entry)}>
                                <i class="fa-solid fa-file-export"></i> {$_('saves.export')}
                            </button>
                            <button type="button" class="button-blue button-main" disabled={busy} onclick={() => Import(entry)}>
                                <i class="fa-solid fa-file-import"></i> {$_('saves.import')}
                            </button>
                            <!-- Deleting is offered for reserve saves only: the game
                                 needs its 5 playable slots to stay filled -->
                            {#if entry.slot === null}
                                <button type="button" class="button-red button-main" disabled={busy} title={$_('saves.delete')} aria-label={$_('saves.delete')} onclick={() => DeleteSave(entry)}>
                                    <i class="fa-solid fa-xmark"></i>
                                </button>
                            {/if}
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
