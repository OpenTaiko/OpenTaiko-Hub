// Localized song titles from the soundtrack catalog.
//
// Catalog entries carry a default title (usually romanized) plus `chartTitles` /
// `chartSubtitles` objects holding translations keyed by uppercase language code
// ("JA", "ZH", "EN", ...). Most songs are only translated into a few languages, and
// untranslated slots are absent or empty, so every lookup falls back to the default.
import type { SoundtrackEntry } from '$lib/types';

/**
 * Catalog language code for a Hub locale: the primary subtag, uppercased. Both Chinese
 * variants (zh-Hans / zh-Hant) therefore share the catalog's single "ZH" translation.
 */
export const catalogLanguage = (locale: string | null | undefined): string =>
    (locale ?? '').split('-')[0].toUpperCase();

/** Title in the user's language, falling back to the catalog's default title. */
export const localizedTitle = (entry: SoundtrackEntry, locale: string | null | undefined): string =>
    entry.chartTitles?.[catalogLanguage(locale)] || entry.chartTitle;

/** Subtitle in the user's language, falling back to the catalog's default subtitle. */
export const localizedSubtitle = (entry: SoundtrackEntry, locale: string | null | undefined): string =>
    entry.chartSubtitles?.[catalogLanguage(locale)] || entry.chartSubtitle || '';

/**
 * Every spelling of a song's name, lowercased, for searching. Searching covers all
 * translations (not just the displayed one) so a song can be found by its Japanese
 * title as well as its romanized one, whatever language the Hub is set to.
 */
export const titleSearchTerms = (entry: SoundtrackEntry): string[] => [
    entry.chartTitle,
    entry.chartSubtitle ?? '',
    ...Object.values(entry.chartTitles ?? {}),
    ...Object.values(entry.chartSubtitles ?? {})
].filter(Boolean).map((term) => term.toLowerCase());
