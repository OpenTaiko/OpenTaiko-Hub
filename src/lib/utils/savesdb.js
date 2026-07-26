// Per-save export / import for OpenTaiko's Saves.db3, operating on a sql.js Database.
//
// A save spreads across the `saves` row plus child tables keyed by SaveId. Export pulls
// one save (by SaveId) into a portable JSON document. Import either MERGES into an
// existing save when the archive's SaveUID matches one already present, or ADDs the save
// as a new entry otherwise (or when the archive has no SaveUID). Everything runs against
// the passed-in sql.js Database so the exact same schema the game uses is honored.

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

const rows = (db, sql, params = []) => {
    const stmt = db.prepare(sql);
    try {
        stmt.bind(params);
        const out = [];
        while (stmt.step()) out.push(stmt.getAsObject());
        return out;
    } finally {
        stmt.free();
    }
};
const one = (db, sql, params = []) => rows(db, sql, params)[0] ?? null;

const tableExists = (db, name) =>
    !!one(db, "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", [name]);
const columns = (db, table) => rows(db, `PRAGMA table_info(${table})`).map((r) => r.name);

export const readDbVersion = (db) => {
    try {
        const r = one(db, 'SELECT SupportedVersion FROM opentaiko_version');
        if (r && r.SupportedVersion) return String(r.SupportedVersion);
    } catch {
        // table absent → the game assumes the oldest version
    }
    return DEFAULT_DB_VERSION;
};

// Lists every save (active slots first, then reserves) for the export picker.
export const listSaves = (db) => {
    const hasUid = columns(db, 'saves').includes('SaveUID');
    const uidSel = hasUid ? 'SaveUID' : "'' AS SaveUID";
    return rows(
        db,
        `SELECT SaveId, PlayerName, CurrentSlot, ${uidSel} FROM saves
         ORDER BY (CurrentSlot IS NULL), CurrentSlot, SaveId`
    ).map((r) => ({
        saveId: r.SaveId,
        name: r.PlayerName,
        slot: r.CurrentSlot === null || r.CurrentSlot === undefined ? null : Number(r.CurrentSlot),
        saveUid: r.SaveUID ?? ''
    }));
};

export const exportSave = (db, saveId) => {
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

const mergeSaveRow = (db, targetId, save) => {
    const saveCols = columns(db, 'saves');
    const sets = [];
    const params = [];
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

const mergeBestPlays = (db, targetId, plays) => {
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
        const merged = {};
        const importedWins = Number(play.HighScore ?? 0) > Number(existing.HighScore ?? 0);
        for (const field of HIGHSCORE_FIELDS) {
            if (bpCols.includes(field)) merged[field] = importedWins ? play[field] : existing[field];
        }
        if (bpCols.includes('ClearStatus')) merged.ClearStatus = Math.max(Number(existing.ClearStatus ?? -1), Number(play.ClearStatus ?? -1));
        if (bpCols.includes('TowerBestFloor')) merged.TowerBestFloor = Math.max(Number(existing.TowerBestFloor ?? 0), Number(play.TowerBestFloor ?? 0));
        if (bpCols.includes('PlayCount')) merged.PlayCount = Number(existing.PlayCount ?? 0) + Number(play.PlayCount ?? 0);
        const setCols = Object.keys(merged);
        if (setCols.length > 0) {
            db.run(
                `UPDATE best_plays SET ${setCols.map((c) => `${c}=?`).join(', ')} WHERE PlayId=?`,
                [...setCols.map((c) => merged[c]), existing.PlayId]
            );
        }
    }
};

const insertBestPlays = (db, saveId, plays) => {
    const bpCols = BEST_PLAY_COLS.filter((c) => columns(db, 'best_plays').includes(c));
    for (const play of plays) {
        const cols = bpCols.filter((c) => c in play);
        db.run(
            `INSERT INTO best_plays (${cols.join(',')}, SaveId) VALUES (${cols.map(() => '?').join(',')}, ?)`,
            [...cols.map((c) => play[c]), saveId]
        );
    }
};

const unionInsert = (db, table, col, values, saveId) => {
    for (const value of values) {
        const exists = one(db, `SELECT 1 FROM ${table} WHERE ${col}=? AND SaveId=?`, [value, saveId]);
        if (!exists) db.run(`INSERT INTO ${table} (${col}, SaveId) VALUES (?, ?)`, [value, saveId]);
    }
};

const applyDanTitles = (db, saveId, danTitles) => {
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

const applyGlobalCounters = (db, saveId, counters, merge) => {
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

const applyChildren = (db, saveId, save, merge) => {
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

const freeSlot = (db) => {
    const used = new Set(
        rows(db, 'SELECT CurrentSlot FROM saves WHERE CurrentSlot IS NOT NULL').map((r) => Number(r.CurrentSlot))
    );
    for (let s = 0; s < 5; s++) if (!used.has(s)) return s;
    return null; // all 5 active slots taken → import as a reserve save (NULL slot)
};

// Moves a save to a target active slot (0-4). When another save already holds the
// target slot, the two swap places. A slotted save can never be parked to reserve:
// the game requires all 5 slots to exist and stay unique, so the only way out of a
// slot is being displaced by the swap. A reserve save entering a slot displaces the
// occupant to reserve, keeping the 5-slot layout complete either way.
// Returns { changed, swapped, name, swappedWith, swappedWithId }.
export const rebindSlot = (db, saveId, targetSlot) => {
    const saveRow = one(db, 'SELECT SaveId, PlayerName, CurrentSlot FROM saves WHERE SaveId=?', [saveId]);
    if (!saveRow) throw new Error(`Save ${saveId} not found`);
    const fromSlot = saveRow.CurrentSlot === null || saveRow.CurrentSlot === undefined ? null : Number(saveRow.CurrentSlot);
    const toSlot = targetSlot === null || targetSlot === undefined ? null : Number(targetSlot);
    const unchanged = { changed: false, swapped: false, name: saveRow.PlayerName, swappedWith: null, swappedWithId: null };
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
        name: saveRow.PlayerName,
        swappedWith: occupant?.PlayerName ?? null,
        swappedWithId: occupant?.SaveId ?? null
    };
};

// Imports a portable save document into `db`. `newUid` is used when the archive has no
// SaveUID (pass crypto.randomUUID() from the caller so this stays environment-agnostic).
// Returns { mode: 'merge'|'add', name, slot }.
export const importSave = (db, portable, newUid) => {
    const save = portable.save ?? {};
    const saveCols = columns(db, 'saves');
    const hasUid = saveCols.includes('SaveUID');
    const importedUid = (save.SaveUID ?? '').trim();

    // Merge only when the archive UID matches an existing save
    let target = null;
    if (hasUid && importedUid) {
        target = one(db, 'SELECT SaveId, PlayerName FROM saves WHERE SaveUID=?', [importedUid]);
    }

    if (target) {
        mergeSaveRow(db, target.SaveId, save);
        applyChildren(db, target.SaveId, portable, true);
        const slotRow = one(db, 'SELECT CurrentSlot FROM saves WHERE SaveId=?', [target.SaveId]);
        return { mode: 'merge', name: save.PlayerName ?? target.PlayerName, slot: slotRow?.CurrentSlot ?? null };
    }

    // Add as a new save
    const slot = freeSlot(db);
    const insertCols = SAVE_COLS.filter((c) => saveCols.includes(c) && c !== 'SaveUID' && c in save);
    const colList = [...insertCols];
    const valParams = insertCols.map((c) => save[c]);
    if (hasUid) {
        colList.push('SaveUID');
        valParams.push(importedUid || newUid);
    }
    colList.push('CurrentSlot');
    valParams.push(slot);
    db.run(
        `INSERT INTO saves (${colList.join(',')}) VALUES (${colList.map(() => '?').join(',')})`,
        valParams
    );
    const newId = Number(one(db, 'SELECT last_insert_rowid() AS id').id);
    applyChildren(db, newId, portable, false);
    return { mode: 'add', name: save.PlayerName ?? 'Player', slot };
};
