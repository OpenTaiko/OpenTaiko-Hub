import { register, init, getLocaleFromNavigator } from 'svelte-i18n';

register('en',      () => import('./en.json'));
register('ja',      () => import('./ja.json'));
register('zh-Hans', () => import('./zh-Hans.json'));
register('zh-Hant', () => import('./zh-Hant.json'));
register('fr',      () => import('./fr.json'));
register('es',      () => import('./es.json'));
register('de',      () => import('./de.json'));
register('nl',      () => import('./nl.json'));
register('ko',      () => import('./ko.json'));
register('ru',      () => import('./ru.json'));

function resolveLocale(): string {
    const nav = getLocaleFromNavigator() ?? '';

    if (/^zh(-Hans(-|$)|-(CN|SG|MY)(-|$))/i.test(nav) || /^zh$/i.test(nav)) {
        return 'zh-Hans';
    }
    if (/^zh(-Hant(-|$)|-(TW|HK|MO)(-|$))/i.test(nav)) {
        return 'zh-Hant';
    }
    if (/^ja(-|$)/i.test(nav)) return 'ja';
    if (/^fr(-|$)/i.test(nav)) return 'fr';
    if (/^es(-|$)/i.test(nav)) return 'es';
    if (/^de(-|$)/i.test(nav)) return 'de';
    if (/^nl(-|$)/i.test(nav)) return 'nl';
    if (/^ko(-|$)/i.test(nav)) return 'ko';
    if (/^ru(-|$)/i.test(nav)) return 'ru';

    return 'en';
}

export function setupI18n(): void {
    init({ fallbackLocale: 'en', initialLocale: resolveLocale() });
}
