// Hall of Fame: the community score database (hof.db3) and the list-point rules of the
// website, kept apart from the Songs page so the math can be tested on its own.
import type { Database } from 'sql.js';
import { getSQL } from '$lib/utils/sqljs';
import { fetchCached } from '$lib/utils/download';
import type { CourseName, HoFScore } from '$lib/types';

const HOF_DB_URL = 'https://opentaiko.github.io/hof.db3';

/** Course of each difficulty index used by the database and the website. */
export const HOF_DIFFICULTY: Record<number, CourseName> = { 0: 'Easy', 1: 'Normal', 2: 'Hard', 3: 'Oni', 4: 'Edit' };
export const HOF_DIFFICULTY_INDEX: Partial<Record<CourseName, number>> = { Easy: 0, Normal: 1, Hard: 2, Oni: 3, Edit: 4 };

/** uniqueId → course → global rank */
export type HoFRanks = Record<string, Partial<Record<CourseName, number>>>;

// Max list points decay per rank, matching the website: 0.95 up to rank 20, then
// 0.96 up to rank 50, then 0.98, so the drop flattens out for later ranks. The
// tiers chain from one another (no reset), which keeps the curve continuous.
export const ComputeMaxListPoints = (rank: number): number => {
    const r = Math.max(1, rank);
    const steps95 = Math.min(r, 20) - 1;
    const steps96 = Math.min(Math.max(r - 20, 0), 30);
    const steps98 = Math.max(r - 50, 0);
    return Math.trunc(1000 * Math.pow(0.95, steps95) * Math.pow(0.96, steps96) * Math.pow(0.98, steps98));
};

export const ScoreToListPointsRatio = (score: Pick<HoFScore, 'goodCount' | 'okCount' | 'badCount' | 'status'>): number => {
    const total = score.goodCount + score.okCount + score.badCount;
    if (total === 0) return 0;
    const accuracy = (score.goodCount + score.okCount * 0.5) / total;
    const badRatio = score.badCount / total;
    let ratio = Math.pow(accuracy, 6) * Math.pow(1 - badRatio, 18);
    switch (score.status) {
        case 'Perfect':    break;
        case 'Full Combo': ratio *= 0.9; break;
        case 'Clear':      ratio *= 0.7; break;
        default:           ratio = 0;    break;
    }
    return ratio;
};

/**
 * Downloads the database and ranks every chart. The global rank orders all entries by
 * internalDifficultyIndex, whatever their difficulty. The caller owns the database and
 * closes it.
 */
export const LoadHallOfFame = async (): Promise<{ db: Database; ranks: HoFRanks }> => {
    const SQL = await getSQL();
    const db = new SQL.Database(new Uint8Array(await fetchCached(HOF_DB_URL)));
    const ranks: HoFRanks = {};
    const result = db.exec('SELECT uniqueId, difficulty, internalDifficultyIndex FROM entries ORDER BY internalDifficultyIndex DESC');
    if (result.length > 0) {
        let globalRank = 0;
        for (const [uniqueId, difficulty] of result[0].values) {
            globalRank++;
            const course = HOF_DIFFICULTY[Number(difficulty)];
            if (course && typeof uniqueId === 'string') {
                (ranks[uniqueId] ??= {})[course] = globalRank;
            }
        }
    }
    return { db, ranks };
};

type ScoreRow = Omit<HoFScore, 'rank' | 'listPoints'>;

/** Ranks scores by list points (then score), as the website does. */
export const RankScores = (rows: ScoreRow[], maxListPoints: number): HoFScore[] =>
    rows
        .map((s) => ({ ...s, listPoints: Math.round(maxListPoints * ScoreToListPointsRatio(s)) }))
        .sort((a, b) => b.listPoints - a.listPoints || b.score - a.score)
        .map((s, i): HoFScore => ({ ...s, rank: i + 1 }));

/** Scores of one chart, or null when it is not ranked. */
export const ChartScores = (
    db: Database,
    ranks: HoFRanks,
    uniqueId: string,
    course: CourseName
): { maxListPoints: number; scores: HoFScore[] } | null => {
    const rank = ranks[uniqueId]?.[course];
    const difficulty = HOF_DIFFICULTY_INDEX[course];
    if (rank === undefined || difficulty === undefined) return null;
    const result = db.exec(
        `SELECT player, status, score, grade, goodCount, okCount, badCount, videoLink, imageLink
         FROM scores WHERE entryId = ? AND difficulty = ?`,
        [uniqueId, difficulty]
    );
    const rows: ScoreRow[] = result.length > 0
        ? result[0].values.map((row) => ({
            player: String(row[0] ?? ''), status: String(row[1] ?? ''), score: Number(row[2] ?? 0), grade: String(row[3] ?? ''),
            goodCount: Number(row[4] ?? 0), okCount: Number(row[5] ?? 0), badCount: Number(row[6] ?? 0),
            videoLink: row[7] == null ? null : String(row[7]), imageLink: row[8] == null ? null : String(row[8])
        }))
        : [];
    const maxListPoints = ComputeMaxListPoints(rank);
    return { maxListPoints, scores: RankScores(rows, maxListPoints) };
};
