<script lang="ts">
    // Per-save export / import for the active instance's Saves.db3.
    //
    // Each save (a slot identified by its player name) is exported with all of its
    // attached data — scores, unlocks, coins, counters, dan titles, nameplates — into a
    // portable .optksave file. On import, if the archive's SaveUID matches a save already
    // present it is MERGED into it (progress is never regressed); otherwise it is ADDED
    // as a new save. Imports are only accepted when the archive is the same version as,
    // or newer than, the instance's current save database.
    import { getContext, untrack } from 'svelte';
    import { DescribeError, ErrorText } from '$lib/utils/errors';
    import { readFile, exists } from '@tauri-apps/plugin-fs';
    import { confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
    import { ipc } from '$lib/ipc';
    import { path } from '@tauri-apps/api';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import { getSQL } from '$lib/utils/sqljs';
    import { compareVersions } from '$lib/utils/versions';
    import { readDbVersion, listSaves, exportSave, importSaveInto, rebindSlot, createSave, deleteSave } from '$lib/utils/savesdb';
    import { activeInstance } from '$lib/stores/instances';
    import { startJob } from '$lib/stores/activity';
    import type { Database } from 'sql.js';
    import type { Instance, PortableSave, SaveSummary, ToastContext } from '$lib/types';

    const { TriggerError, TriggerWarning, TriggerSuccess } = getContext<ToastContext>('toast');

    const DB_NAME = 'Saves.db3';

    let saves = $state<SaveSummary[]>([]);
    let currentVersion = $state<string | null>(null);
    let loading = $state(false);
    let loadError = $state<string | null>(null);
    let busy = $state(false);
    let loadToken = 0; // guards against out-of-order reloads when switching instances


    const dbPath = (inst: Instance): Promise<string> => path.join(inst.path, DB_NAME);

    // Name of the copy of Saves.db3 taken before a change, in <instance>/SavesBackup
    const BackupName = (): string => {
        const d = new Date();
        const p = (n: number, w = 2) => String(n).padStart(w, '0');
        return `Saves-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}`
            + `-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}-${p(d.getMilliseconds(), 3)}.db3`;
    };

    // Every change to Saves.db3 goes through here. The backend copies the current file
    // aside first and keeps the newest 20 copies, then writes the new content next to it
    // and renames it over, so a crash or a full disk mid-write leaves the previous save
    // intact instead of a truncated file. If the backup cannot be made, nothing is written.
    const WriteDb = async (inst: Instance, bytes: Uint8Array) => {
        // A job, so closing the window asks first instead of cutting the write off
        const job = startJob('home', inst.id);
        try {
            await ipc.writeSaves(inst.path, BackupName(), bytes);
        } finally {
            job.end();
        }
    };

    // The game keeps its save database open and writes it back when it exits, which
    // undid any change made meanwhile. Only advised against before; the backend refuses
    // the write as well.
    const GameRunning = async (inst: Instance): Promise<boolean> => {
        const running = await ipc.gameRunning(inst.path).catch(() => false);
        if (running) TriggerWarning(get(_)('saves.error.game_running'));
        return running;
    };

    // No step of a load may hang forever: a stuck await would leave the tab spinning
    // with nothing to click, so every load is bounded and always ends in a state the
    // user can act on.
    const withTimeout = <T,>(promise: Promise<T>, ms: number, label: string): Promise<T> => Promise.race([
        promise,
        // Not "timed out": that reads as a network failure (see DescribeError)
        new Promise<never>((_, reject) => setTimeout(() => reject(new Error(`${label} took too long`)), ms))
    ]);

    // Loads a specific instance's DB. Captures a token so a slow reload for a previous
    // instance can never overwrite the current one's data. `silent` keeps the current
    // rows on screen while re-reading (no placeholder, no layout shift).
    const Load = async (inst: Instance | null, silent = false) => {
        if (!inst) return;
        const token = ++loadToken;
        if (!silent) {
            loading = true;
            saves = [];
            currentVersion = null;
        }
        loadError = null;
        let db: Database | null = null;
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
            if (token === loadToken) loadError = error instanceof Error ? error.message : String(error);
        } finally {
            db?.close();
        }
        if (token === loadToken) loading = false;
    };

    const stripV = (v: string | null | undefined): string => (v ?? '').replace(/^v/i, '');

    const Export = async (entry: SaveSummary) => {
        if (busy || !instance) return;
        const inst = instance;
        let db: Database | null = null;
        // Held before anything is awaited, so a double click cannot start a second one
        busy = true;
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
            // The backend asks where to save and writes the file there
            const dest = await ipc.exportSave(suggested, get(_)('files.optk_save'), JSON.stringify(portable));
            if (!dest) return;
            TriggerSuccess(get(_)('saves.success.export', { values: { name: entry.name } }));
        } catch (error) {
            TriggerError(get(_)('saves.error.export', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        } finally {
            db?.close();
            busy = false;
        }
    };

    const SlotLabel = (entry: SaveSummary): string => entry.slot === null
        ? get(_)('saves.slot_reserve')
        : get(_)('saves.slot_n', { values: { n: entry.slot + 1 } });

    // Runs a write against the instance DB and reloads the list without a layout shift
    const WithDb = async <T,>(action: (db: Database) => T | Promise<T>): Promise<T | null> => {
        if (busy || !instance) return null;
        const inst = instance;
        let db: Database | null = null;
        // Held before anything is awaited: set after the game check, a double click ran
        // two writes, the second undoing the first while both said they succeeded
        busy = true;
        try {
            if (await GameRunning(inst)) return null;
            const p = await dbPath(inst);
            if (!(await exists(p))) {
                TriggerError(get(_)('saves.error.no_db'));
                return null;
            }
            const SQL = await getSQL();
            db = new SQL.Database(await readFile(p));
            const result = await action(db);
            // An action may return { write: false } to leave the file untouched
            const skipWrite = typeof result === 'object' && result !== null && 'write' in result && result.write === false;
            if (!skipWrite) await WriteDb(inst, db.export());
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
            TriggerError(get(_)('saves.error.create', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    const DeleteSave = async (entry: SaveSummary) => {
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
            TriggerError(get(_)('saves.error.delete', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        }
    };

    // Imports an archive into one chosen save. The destination is picked by the user
    // rather than derived from the archive's unique id, so a save exported on another
    // machine can be brought into any slot. The data is merged, never regressed.
    const Import = async (entry: SaveSummary) => {
        if (busy || !instance) return;
        const inst = instance;
        let db: Database | null = null;
        // Held before anything is awaited, so a double click cannot start a second import
        busy = true;
        try {
            if (await GameRunning(inst)) return;
            // Read by the backend: the file itself stays out of the page's reach
            const text = await ipc.pickSave(get(_)('files.optk_save'));
            if (text === null) return;

            let portable: Partial<PortableSave>;
            try {
                portable = JSON.parse(text) as Partial<PortableSave>;
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
                        imported: String(portable.save.PlayerName ?? '?')
                    }
                }),
                { title: get(_)('saves.confirm_title') }
            );
            if (!confirmed) return;

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
                    values: { imported: portable.dbVersion ?? '?', current: dbVersion }
                }));
                return;
            }

            const result = importSaveInto(db, portable, entry.saveId, crypto.randomUUID());
            await WriteDb(inst, db.export());
            // Silent reload: refreshes the names without the layout shifting
            await Load(inst, true);
            TriggerSuccess(get(_)('saves.success.imported_into', {
                values: { name: result.name, slot: SlotLabel(entry) }
            }));
        } catch (error) {
            TriggerError(get(_)('saves.error.import', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
        } finally {
            db?.close();
            busy = false;
        }
    };

    // Rebinds a save to another slot; an occupied target slot swaps the two saves
    // (e.g. moving P5 to Slot 1 puts the previous Slot 1 save into P5's old place).
    // The rows are updated in place (no reload) so the layout never shifts.
    const Rebind = async (entry: SaveSummary, event: Event) => {
        if (busy || !instance) return;
        const inst = instance;
        // Read before anything is awaited: the event no longer has a target afterwards
        const select = event.currentTarget as HTMLSelectElement;
        const targetSlot = Number(select.value);
        const fromSlot = entry.slot;
        let db: Database | null = null;
        // Held before anything is awaited, so a second change cannot start a second write
        busy = true;
        try {
            if (await GameRunning(inst)) {
                // Nothing changed, so the selector goes back to where the save really is
                select.value = entry.slot === null ? 'reserve' : String(entry.slot);
                return;
            }
            const p = await dbPath(inst);
            if (!(await exists(p))) {
                TriggerError(get(_)('saves.error.no_db'));
                return;
            }
            const SQL = await getSQL();
            db = new SQL.Database(await readFile(p));
            const result = rebindSlot(db, entry.saveId, targetSlot);
            if (result.changed) {
                await WriteDb(inst, db.export());
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
            TriggerError(get(_)('saves.error.rebind', { values: { error: DescribeError(error) } }), { details: ErrorText(error) });
            // Resync quietly so the selectors match the real layout, without the
            // loading placeholder that would shift the layout
            await Load(inst, true);
        } finally {
            db?.close();
            busy = false;
        }
    };
    let instance = $derived($activeInstance);
    // Reload the save DB whenever the active instance changes, and only then: the store
    // also changes when anything else in the instance list does. The id, derived, changes
    // only with the instance; the load itself is kept out of what the effect depends on.
    let instanceId = $derived(instance?.id ?? null);
    $effect(() => {
        if (instanceId === null) return;
        untrack(() => { if (instance) Load(instance); });
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

        <!-- The cause in plain words when it is a known one, else the error as it came:
             a timeout or a damaged file otherwise left nothing to go on -->
        {#if loadError}
            <p class="text-sm opacity-70 select-text">{DescribeError(loadError)}</p>
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
                                aria-label={$_('saves.rebind_hint')}
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
                                    <i class="fa-solid fa-trash"></i>
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
