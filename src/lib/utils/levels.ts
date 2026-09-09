// Difficulty level display. Charts may carry fractional levels (e.g. 8.5); the Taiko
// convention writes a half step as a "+" suffix, so 8.5 → "8+" and 10.7 → "10+", while
// 10.4 → "10". Applies to every course and level, not only the 10+ ones.
export const FormatLevel = (level: number): string => {
    const base = Math.floor(level);
    if (level - base >= 0.5) return `${base}+`;
    return `${base}`;
};
