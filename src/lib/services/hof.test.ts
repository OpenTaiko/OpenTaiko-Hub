import { describe, expect, it } from 'vitest';
import { ComputeMaxListPoints, ScoreToListPointsRatio, RankScores } from './hof';

describe('ComputeMaxListPoints', () => {
    it('starts at 1000 and decays by tier, each tier continuing from the last', () => {
        expect(ComputeMaxListPoints(1)).toBe(1000);
        expect(ComputeMaxListPoints(2)).toBe(950);
        // 0.95 up to rank 20
        expect(ComputeMaxListPoints(20)).toBe(377);
        // then 0.96 up to rank 50
        expect(ComputeMaxListPoints(21)).toBe(362);
        expect(ComputeMaxListPoints(50)).toBe(110);
        // then 0.98
        expect(ComputeMaxListPoints(51)).toBe(108);
    });

    it('treats ranks below 1 as rank 1', () => {
        expect(ComputeMaxListPoints(0)).toBe(1000);
        expect(ComputeMaxListPoints(-5)).toBe(1000);
    });
});

describe('ScoreToListPointsRatio', () => {
    const score = (status: string, goodCount: number, okCount = 0, badCount = 0) => ({ status, goodCount, okCount, badCount });

    it('gives a perfect play the full points', () => {
        expect(ScoreToListPointsRatio(score('Perfect', 500))).toBe(1);
    });

    it('scales down a full combo and a clear', () => {
        expect(ScoreToListPointsRatio(score('Full Combo', 500))).toBeCloseTo(0.9);
        expect(ScoreToListPointsRatio(score('Clear', 500))).toBeCloseTo(0.7);
    });

    it('gives nothing for a failed play or an empty one', () => {
        expect(ScoreToListPointsRatio(score('Failed', 500))).toBe(0);
        expect(ScoreToListPointsRatio(score('Perfect', 0))).toBe(0);
    });

    it('costs more for a bad than for an ok', () => {
        const withOk = ScoreToListPointsRatio(score('Clear', 499, 1, 0));
        const withBad = ScoreToListPointsRatio(score('Clear', 499, 0, 1));
        expect(withBad).toBeLessThan(withOk);
    });
});

describe('RankScores', () => {
    const row = (player: string, status: string, score: number, goodCount: number, okCount: number) => ({
        player, status, score, grade: '', goodCount, okCount, badCount: 0, videoLink: null, imageLink: null
    });

    it('orders by list points, then by score, and numbers the ranks', () => {
        const ranked = RankScores([
            row('clear', 'Clear', 900000, 500, 0),
            row('perfect', 'Perfect', 800000, 500, 0),
            row('perfect-high', 'Perfect', 990000, 500, 0)
        ], 1000);
        expect(ranked.map((s) => s.player)).toEqual(['perfect-high', 'perfect', 'clear']);
        expect(ranked.map((s) => s.rank)).toEqual([1, 2, 3]);
        expect(ranked[0].listPoints).toBe(1000);
        expect(ranked[2].listPoints).toBe(700);
    });
});
