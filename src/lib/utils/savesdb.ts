// Per-save export / import for OpenTaiko's Saves.db3, operating on a sql.js Database.
//
// A save spreads across the `saves` row plus child tables keyed by SaveId. Export pulls
// one save (by SaveId) into a portable JSON document. Import MERGES a document into the
// save the user picked (one import button per slot), so an archive from another machine
// works even though its SaveUID matches nothing locally. Everything runs against the
// passed-in sql.js Database so the exact same schema the game uses is honored.
//
// Merging is idempotent by design: every field is combined with a rule that yields the
// same result when applied twice (highest score, highest counter, union of unlocks), so
// re-importing the same archive can never inflate or otherwise alter the save.
import type { Database, SqlValue } from 'sql.js';
import type { PortableSave, SaveRow, SaveSummary } from '$lib/types';

export const DEFAULT_DB_VERSION = 'v0.6.0.0';

// saves columns carried by an export (SaveId is auto, CurrentSlot is instance-local)
const SAVE_COLS = [
    'PlayerName', 'PlayerNameplateTitle', 'PlayerDanTitle', 'PlayerDanGold', 'PlayerDanType',
    'PlayerNameplateType', 'PlayerPuchichara', 'PlayerCharacter', 'PlayerCharacterName',
    'CurrentMedals', 'TotalEarnedMedals', 'TotalPlaycount', 'AIBattleModePlaycount',
    'AIBattleModeWins', 'PlayerNameplateRarityInt', 'PlayerNameplateId', 'SelectedHitsounds', 'SaveUID'
];

// Cumulative counters merged with MAX so importing never regresses progress
const COUNTER_COLS = new Set([
    'CurrentMedals', 'TotalEarnedMedals', 'TotalPlaycount', 'AIBattleModePlaycount', 'AIBattleModeWins'
]);

const BEST_PLAY_COLS = [
    'ChartUniqueId', 'ChartGenre', 'Charter', 'Artist', 'PlayMods', 'ChartDifficulty', 'ChartLevel',
    'ClearStatus', 'ScoreRank', 'HighScore', 'TowerBestFloor',
    'DanExam1', 'DanExam2', 'DanExam3', 'DanExam4', 'DanExam5', 'DanExam6', 'DanExam7',
    'PlayCount', 'HighScoreGoodCount', 'HighScoreOkCount', 'HighScoreBadCount',
    'HighScoreMaxCombo', 'HighScoreRollCount', 'HighScoreADLibCount', 'HighScoreBoomCount'
];
// best_play fields taken from whichever record has the higher HighScore
const HIGHSCORE_FIELDS = [
    'HighScore', 'ScoreRank', 'HighScoreGoodCount', 'HighScoreOkCount', 'HighScoreBadCount',
    'HighScoreMaxCombo', 'HighScoreRollCount', 'HighScoreADLibCount', 'HighScoreBoomCount',
    'DanExam1', 'DanExam2', 'DanExam3', 'DanExam4', 'DanExam5', 'DanExam6', 'DanExam7'
];

type Params = SqlValue[];

const rows = (db: Database, sql: string, params: Params = []): SaveRow[] => {
    const stmt = db.prepare(sql);
    try {
        stmt.bind(params);
        const out: SaveRow[] = [];
        // sql.js rows may also hold blobs, which no save table uses
        while (stmt.step()) out.push(stmt.getAsObject() as SaveRow);
        return out;
    } finally {
        stmt.free();
    }
};
const one = (db: Database, sql: string, params: Params = []): SaveRow | null => rows(db, sql, params)[0] ?? null;

const tableExists = (db: Database, name: string): boolean =>
    !!one(db, "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", [name]);
const columns = (db: Database, table: string): string[] => rows(db, `PRAGMA table_info(${table})`).map((r) => String(r.name));

const slotOf = (value: SqlValue | undefined): number | null =>
    value === null || value === undefined ? null : Number(value);

export const readDbVersion = (db: Database): string => {
    try {
        const r = one(db, 'SELECT SupportedVersion FROM opentaiko_version');
        if (r && r.SupportedVersion) return String(r.SupportedVersion);
    } catch {
        // table absent → the game assumes the oldest version
    }
    return DEFAULT_DB_VERSION;
};

// Lists every save (active slots first, then reserves) for the export picker.
export const listSaves = (db: Database): SaveSummary[] => {
    const hasUid = columns(db, 'saves').includes('SaveUID');
    const uidSel = hasUid ? 'SaveUID' : "'' AS SaveUID";
    return rows(
        db,
        `SELECT SaveId, PlayerName, CurrentSlot, ${uidSel} FROM saves
         ORDER BY (CurrentSlot IS NULL), CurrentSlot, SaveId`
    ).map((r) => ({
        saveId: Number(r.SaveId),
        name: String(r.PlayerName ?? ''),
        slot: slotOf(r.CurrentSlot),
        saveUid: String(r.SaveUID ?? '')
    }));
};

export const exportSave = (db: Database, saveId: number): PortableSave => {
    const saveCols = columns(db, 'saves');
    const present = SAVE_COLS.filter((c) => saveCols.includes(c));
    const saveRow = one(db, `SELECT ${present.join(',')} FROM saves WHERE SaveId=?`, [saveId]);
    if (!saveRow) throw new Error(`Save ${saveId} not found`);

    const bpCols = BEST_PLAY_COLS.filter((c) => columns(db, 'best_plays').includes(c));
    const bestPlays = rows(db, `SELECT ${bpCols.join(',')} FROM best_plays WHERE SaveId=?`, [saveId]);
    const activeTriggers = rows(db, 'SELECT Trigger FROM active_triggers WHERE SaveId=?', [saveId]).map((r) => r.Trigger);
    const globalCounters = tableExists(db, 'global_counters')
        ? rows(db, 'SELECT CounterName, CounterValue FROM global_counters WHERE SaveId=?', [saveId])
        : [];
    const danTitles = rows(db, 'SELECT DanTitleText, DanClearStatus, DanIsGold FROM dan_titles WHERE SaveId=?', [saveId]);
    const nameplateTitles = rows(db, 'SELECT NameplateId FROM nameplate_titles WHERE SaveId=?', [saveId]).map((r) => r.NameplateId);
    const unlockedCharacters = rows(db, 'SELECT Asset FROM unlocked_characters WHERE SaveId=?', [saveId]).map((r) => r.Asset);
    const unlockedPuchicharas = rows(db, 'SELECT Asset FROM unlocked_puchicharas WHERE SaveId=?', [saveId]).map((r) => r.Asset);
    const unlockedSongs = rows(db, 'SELECT Asset FROM unlocked_songs WHERE SaveId=?', [saveId]).map((r) => r.Asset);

    return {
        format: 'optk-save',
        formatVersion: 1,
        dbVersion: readDbVersion(db),
        save: saveRow,
        bestPlays,
        activeTriggers,
        globalCounters,
        danTitles,
        nameplateTitles,
        unlockedCharacters,
        unlockedPuchicharas,
        unlockedSongs
    };
};

const mergeSaveRow = (db: Database, targetId: number, save: SaveRow): void => {
    const saveCols = columns(db, 'saves');
    const sets: string[] = [];
    const params: Params = [];
    for (const col of SAVE_COLS) {
        if (col === 'SaveUID') continue;          // identity key — keep the target's
        if (!saveCols.includes(col)) continue;    // column absent on this (older) DB
        if (!(col in save)) continue;
        if (COUNTER_COLS.has(col)) {
            sets.push(`${col} = MAX(${col}, ?)`);
        } else {
            sets.push(`${col} = ?`);
        }
        params.push(save[col]);
    }
    if (sets.length > 0) {
        db.run(`UPDATE saves SET ${sets.join(', ')} WHERE SaveId=?`, [...params, targetId]);
    }
};

const mergeBestPlays = (db: Database, targetId: number, plays: SaveRow[]): void => {
    const bpCols = BEST_PLAY_COLS.filter((c) => columns(db, 'best_plays').includes(c));
    for (const play of plays) {
        const existing = one(
            db,
            'SELECT * FROM best_plays WHERE ChartUniqueId=? AND ChartDifficulty=? AND PlayMods=? AND SaveId=?',
            [play.ChartUniqueId, play.ChartDifficulty ?? 3, play.PlayMods ?? 0, targetId]
        );
        if (!existing) {
            const cols = bpCols.filter((c) => c in play);
            db.run(
                `INSERT INTO best_plays (${cols.join(',')}, SaveId) VALUES (${cols.map(() => '?').join(',')}, ?)`,
                [...cols.map((c) => play[c]), targetId]
            );
            continue;
        }
        // Merge into the existing record without regressing any achievement
        const merged: SaveRow = {};
        const importedWins = Number(play.HighScore ?? 0) > Number(existing.HighScore ?? 0);
        for (const field of HIGHSCORE_FIELDS) {
            if (bpCols.includes(field)) merged[field] = importedWins ? play[field] : existing[field];
        }
        if (bpCols.includes('ClearStatus')) merged.ClearStatus = Math.max(Number(existing.ClearStatus ?? -1), Number(play.ClearStatus ?? -1));
        if (bpCols.includes('TowerBestFloor')) merged.TowerBestFloor = Math.max(Number(existing.TowerBestFloor ?? 0), Number(play.TowerBestFloor ?? 0));
        // Highest wins rather than a sum: importing the same archive twice must not
        // keep inflating the count. Every other merged field is idempotent the same way.
        if (bpCols.includes('PlayCount')) merged.PlayCount = Math.max(Number(existing.PlayCount ?? 0), Number(play.PlayCount ?? 0));
        const setCols = Object.keys(merged);
        if (setCols.length > 0) {
            db.run(
                `UPDATE best_plays SET ${setCols.map((c) => `${c}=?`).join(', ')} WHERE PlayId=?`,
                [...setCols.map((c) => merged[c]), existing.PlayId]
            );
        }
    }
};

const insertBestPlays = (db: Database, saveId: number, plays: SaveRow[]): void => {
    const bpCols = BEST_PLAY_COLS.filter((c) => columns(db, 'best_plays').includes(c));
    for (const play of plays) {
        const cols = bpCols.filter((c) => c in play);
        db.run(
            `INSERT INTO best_plays (${cols.join(',')}, SaveId) VALUES (${cols.map(() => '?').join(',')}, ?)`,
            [...cols.map((c) => play[c]), saveId]
        );
    }
};

const unionInsert = (db: Database, table: string, col: string, values: SqlValue[], saveId: number): void => {
    for (const value of values) {
        const exists = one(db, `SELECT 1 FROM ${table} WHERE ${col}=? AND SaveId=?`, [value, saveId]);
        if (!exists) db.run(`INSERT INTO ${table} (${col}, SaveId) VALUES (?, ?)`, [value, saveId]);
    }
};

const applyDanTitles = (db: Database, saveId: number, danTitles: SaveRow[]): void => {
    // DanTitleText is globally unique, so upsert like the game's RegisterDanTitle
    for (const t of danTitles) {
        db.run(
            `INSERT INTO dan_titles (DanTitleText, DanClearStatus, DanIsGold, SaveId) VALUES (?, ?, ?, ?)
             ON CONFLICT(DanTitleText) DO UPDATE SET
               DanClearStatus = MAX(DanClearStatus, excluded.DanClearStatus),
               DanIsGold = MAX(DanIsGold, excluded.DanIsGold)`,
            [t.DanTitleText, t.DanClearStatus ?? 0, t.DanIsGold ?? 0, saveId]
        );
    }
};

const applyGlobalCounters = (db: Database, saveId: number, counters: SaveRow[], merge: boolean): void => {
    if (!tableExists(db, 'global_counters')) return;
    for (const c of counters) {
        const existing = merge
            ? one(db, 'SELECT EntryId, CounterValue FROM global_counters WHERE CounterName=? AND SaveId=?', [c.CounterName, saveId])
            : null;
        if (existing) {
            db.run('UPDATE global_counters SET CounterValue = MAX(CounterValue, ?) WHERE EntryId=?', [c.CounterValue, existing.EntryId]);
        } else {
            db.run('INSERT INTO global_counters (CounterName, CounterValue, SaveId) VALUES (?, ?, ?)', [c.CounterName, c.CounterValue, saveId]);
        }
    }
};

const applyChildren = (db: Database, saveId: number, save: Partial<PortableSave>, merge: boolean): void => {
    if (merge) {
        mergeBestPlays(db, saveId, save.bestPlays ?? []);
    } else {
        insertBestPlays(db, saveId, save.bestPlays ?? []);
    }
    unionInsert(db, 'active_triggers', 'Trigger', save.activeTriggers ?? [], saveId);
    applyGlobalCounters(db, saveId, save.globalCounters ?? [], merge);
    applyDanTitles(db, saveId, save.danTitles ?? []);
    unionInsert(db, 'nameplate_titles', 'NameplateId', save.nameplateTitles ?? [], saveId);
    unionInsert(db, 'unlocked_characters', 'Asset', save.unlockedCharacters ?? [], saveId);
    unionInsert(db, 'unlocked_puchicharas', 'Asset', save.unlockedPuchicharas ?? [], saveId);
    unionInsert(db, 'unlocked_songs', 'Asset', save.unlockedSongs ?? [], saveId);
};

// Creates an empty save as a reserve entry (no slot), mirroring what the game's own
// template rows look like. It stays out of the 5 playable slots until the user binds
// it to one, which swaps it with that slot's occupant.
export const createSave = (db: Database, playerName: string, newUid: string): { saveId: number; name: string } => {
    const saveCols = columns(db, 'saves');
    const cols = ['PlayerName'];
    const values: Params = [playerName];
    if (saveCols.includes('SaveUID')) {
        cols.push('SaveUID');
        values.push(newUid);
    }
    // CurrentSlot stays NULL: a new save is always a reserve save
    cols.push('CurrentSlot');
    values.push(null);
    db.run(
        `INSERT INTO saves (${cols.join(',')}) VALUES (${cols.map(() => '?').join(',')})`,
        values
    );
    const saveId = Number(one(db, 'SELECT last_insert_rowid() AS id')?.id);
    return { saveId, name: playerName };
};

// Deletes a reserve save and everything attached to it. Saves bound to one of the 5
// playable slots are refused: the game needs those slots to exist, so a save must be
// moved out of its slot (by a swap) before it can be removed.
export const deleteSave = (db: Database, saveId: number): { deleted: boolean; name: string } => {
    const saveRow = one(db, 'SELECT SaveId, PlayerName, CurrentSlot FROM saves WHERE SaveId=?', [saveId]);
    if (!saveRow) throw new Error(`Save ${saveId} not found`);
    const name = String(saveRow.PlayerName ?? '');
    if (saveRow.CurrentSlot !== null && saveRow.CurrentSlot !== undefined) {
        return { deleted: false, name };
    }
    for (const table of [
        'best_plays', 'active_triggers', 'dan_titles', 'nameplate_titles',
        'unlocked_characters', 'unlocked_puchicharas', 'unlocked_songs', 'global_counters'
    ]) {
        if (tableExists(db, table)) db.run(`DELETE FROM ${table} WHERE SaveId=?`, [saveId]);
    }
    db.run('DELETE FROM saves WHERE SaveId=?', [saveId]);
    return { deleted: true, name };
};

export interface RebindResult {
    changed: boolean;
    swapped: boolean;
    name: string;
    swappedWith: string | null;
    swappedWithId: number | null;
}

// Moves a save to a target active slot (0-4). When another save already holds the
// target slot, the two swap places. A slotted save can never be parked to reserve:
// the game requires all 5 slots to exist and stay unique, so the only way out of a
// slot is being displaced by the swap. A reserve save entering a slot displaces the
// occupant to reserve, keeping the 5-slot layout complete either way.
export const rebindSlot = (db: Database, saveId: number, targetSlot: number | null | undefined): RebindResult => {
    const saveRow = one(db, 'SELECT SaveId, PlayerName, CurrentSlot FROM saves WHERE SaveId=?', [saveId]);
    if (!saveRow) throw new Error(`Save ${saveId} not found`);
    const name = String(saveRow.PlayerName ?? '');
    const fromSlot = slotOf(saveRow.CurrentSlot);
    const toSlot = targetSlot === null || targetSlot === undefined ? null : Number(targetSlot);
    const unchanged: RebindResult = { changed: false, swapped: false, name, swappedWith: null, swappedWithId: null };
    if (fromSlot === toSlot) return unchanged;
    // Refuse to empty a slot: only slot-to-slot and reserve-to-slot moves are valid
    if (toSlot === null) return unchanged;

    const occupant = one(db, 'SELECT SaveId, PlayerName FROM saves WHERE CurrentSlot=?', [toSlot]);

    // CurrentSlot is UNIQUE (NULLs excepted), so park the moving save first
    db.run('UPDATE saves SET CurrentSlot=NULL WHERE SaveId=?', [saveId]);
    if (occupant) db.run('UPDATE saves SET CurrentSlot=? WHERE SaveId=?', [fromSlot, occupant.SaveId]);
    db.run('UPDATE saves SET CurrentSlot=? WHERE SaveId=?', [toSlot, saveId]);

    return {
        changed: true,
        swapped: !!occupant,
        name,
        swappedWith: occupant ? String(occupant.PlayerName ?? '') : null,
        swappedWithId: occupant ? Number(occupant.SaveId) : null
    };
};

// Merges a portable save document into ONE chosen save. The archive's SaveUID is not
// used to find the destination: a save exported on another machine never matches a
// local id, so the user picks the target save instead (one import button per slot).
//
// The merge rules are unchanged and idempotent: highest score wins per chart, counters
// take the highest value, unlocks and triggers are unioned, so progress is never
// regressed and re-importing the same file changes nothing.
//
// The destination keeps its own identity (its slot and its SaveUID), so importing the
// same archive into two slots cannot produce two saves sharing a uuid, which the game
// uses to key per-save data. `fallbackUid` is only used when the destination has no uid
// yet (pass crypto.randomUUID() so this stays environment-agnostic).
export const importSaveInto = (
    db: Database,
    portable: Partial<PortableSave>,
    targetSaveId: number,
    fallbackUid: string
): { name: string; slot: number | null; targetName: string } => {
    const save = portable.save ?? {};
    const saveCols = columns(db, 'saves');
    const target = one(db, 'SELECT SaveId, PlayerName, CurrentSlot FROM saves WHERE SaveId=?', [targetSaveId]);
    if (!target) throw new Error(`Save ${targetSaveId} not found`);

    mergeSaveRow(db, targetSaveId, save);
    if (saveCols.includes('SaveUID')) {
        const current = one(db, 'SELECT SaveUID FROM saves WHERE SaveId=?', [targetSaveId]);
        if (!String(current?.SaveUID ?? '').trim()) {
            db.run('UPDATE saves SET SaveUID=? WHERE SaveId=?', [fallbackUid, targetSaveId]);
        }
    }
    applyChildren(db, targetSaveId, portable, true);

    const targetName = String(target.PlayerName ?? '');
    return {
        name: String(save.PlayerName ?? targetName),
        slot: slotOf(target.CurrentSlot),
        targetName
    };
};
