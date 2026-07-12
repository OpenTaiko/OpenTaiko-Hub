<script>
    import { onMount } from 'svelte';
    import { readTextFile, writeTextFile, mkdir } from '@tauri-apps/plugin-fs';

    import { getContext } from 'svelte';
    const { TriggerWarning, TriggerSuccess } = getContext('toast');

    import { GetPreferencesPath } from "$lib/utils/path.js";

    import { path } from '@tauri-apps/api';

    import { RadioGroup, RadioItem } from '@skeletonlabs/skeleton';
    import { setModeCurrent } from '@skeletonlabs/skeleton';

    import { _ , locale } from 'svelte-i18n';
    import { get } from 'svelte/store';

    // Debug mode code
    let debugMode = false;

    const DisableDebugMode = async () => {
        TriggerSuccess(get(_)('themes.success.debug_disabled'));
        debugMode = false;
    }

    let currentThemeModeDebug = 'dark';

    const ThemeChangerDebug = async (e) => {
        const themevalue = e.target.value;
        const themetarget = document.getElementById('themetarget');
        const themedata = themetarget.getAttribute('data-theme');

        if (themedata == themevalue) {
            TriggerWarning(get(_)('themes.warn.same_theme_debug'));
        }
        else {
            currentTheme = themevalue;
            document.body.dataset.theme = themevalue;
            console.log("DEBUG: theme has been changed")
        }
    }

    const ThemeModeChangerDebug = async (e) => {
        currentThemeModeDebug = e.target.value;
        if (currentThemeModeDebug === "dark") {
            setModeCurrent(false);
            console.log("DEBUG: mode has been changed to dark")
        }
        else if (currentThemeModeDebug === "light") {
            setModeCurrent(true);
            console.log("DEBUG: mode has been changed to light")
        }
    }

    // Theme code
    export let currentTile = 0;

    let currentTheme = 'skeleton';

    const TryFetchingCurrentTheme = async () => {
        const prefsDir = await GetPreferencesPath();
        await mkdir(prefsDir, { recursive: true });
        const themeFilePath = await path.join(prefsDir, 'theme.json');
        try {
            const fileContentTheme = await readTextFile(themeFilePath);
            const jsonDataTheme = JSON.parse(fileContentTheme);
            currentTheme = jsonDataTheme.theme ?? 'skeleton';
        } catch {
            // File missing on first run — silently use default and write it
            currentTheme = 'skeleton';
            try { await writeTextFile(themeFilePath, JSON.stringify({ theme: currentTheme })); } catch {}
        }
        document.body.dataset.theme = currentTheme;
    }

    const ThemeChanger = async (e) => {
        const themevalue = e.target.value;
        const themetarget = document.getElementById('themetarget');
        const themedata = themetarget.getAttribute('data-theme');

        if (themedata == themevalue) {
            TriggerWarning(get(_)('themes.warn.same_theme'));
        }
        else {
            const prefsDir = await GetPreferencesPath();
            await mkdir(prefsDir, { recursive: true });
            const theme_settings = await path.join(prefsDir, 'theme.json');
            const json_theme = {"theme":themevalue}
            await writeTextFile(theme_settings, JSON.stringify(json_theme));

            currentTheme = themevalue;
            document.body.dataset.theme = themevalue;
            console.log("theme has been changed")
        }
    }

    // Theme mode
    const json_modelight = {"thememode":"light"};
    const json_modedark = {"thememode":"dark"};

    let currentThemeMode = 'Loading...';

    const TryFetchingCurrentThemeMode = async () => {
        const prefsDir = await GetPreferencesPath();
        await mkdir(prefsDir, { recursive: true });
        const modeFilePath = await path.join(prefsDir, 'thememode.json');
        try {
            const fileContentMode = await readTextFile(modeFilePath);
            const jsonDataMode = JSON.parse(fileContentMode);
            currentThemeMode = jsonDataMode.thememode ?? 'dark';
        } catch {
            // File missing on first run — silently use default and write it
            currentThemeMode = 'dark';
            try { await writeTextFile(modeFilePath, JSON.stringify({ thememode: currentThemeMode })); } catch {}
        }

        if (currentThemeMode === "dark") {
            setModeCurrent(false);
            console.log("mode has been changed to dark")
        }
        else if (currentThemeMode === "light") {
            setModeCurrent(true);
            console.log("mode has been changed to light")
        }
    }

    const ThemeModeChanger = async (e) => {
        currentThemeMode = e.target.value;
        const prefsDir = await GetPreferencesPath();
        await mkdir(prefsDir, { recursive: true });
        const thememode_settings = await path.join(prefsDir, 'thememode.json');
        if (currentThemeMode === "dark") {
            await writeTextFile(thememode_settings, JSON.stringify(json_modedark));

            setModeCurrent(false);
            console.log("mode has been changed to dark")
        }
        else if (currentThemeMode === "light") {
            await writeTextFile(thememode_settings, JSON.stringify(json_modelight));

            setModeCurrent(true);
            console.log("mode has been changed to light")
        }
    }

    // Language persistence
    const SaveLocale = async (selectedLocale) => {
        try {
            const prefsDir = await GetPreferencesPath();
            await mkdir(prefsDir, { recursive: true });
            const langFilePath = await path.join(prefsDir, 'language.json');
            await writeTextFile(langFilePath, JSON.stringify({ locale: selectedLocale }));
        } catch (err) {
            console.error('Failed to save locale:', err);
        }
    }

    onMount(async () => {
        TryFetchingCurrentTheme();
        TryFetchingCurrentThemeMode();
    });
</script>

{#if currentTile === 7}
        <div id="themetab">
            <section class="card w-full">
                <div class="p-4 space-y-4">
                    <h1>{debugMode ? $_('themes.title_debug') : $_('themes.title')}</h1>
                    <div class="flex gap-2">
                        <select size="11" class="select w-full max-w-[265px]" value={currentTheme} on:change={debugMode ? ThemeChangerDebug : ThemeChanger} on:click={debugMode ? () => {} : TryFetchingCurrentTheme}>
                            <optgroup label={$_('themes.optgroup.optk')}>
                                <option value="gleamingsky">Gleaming Sky</option>
                                <option value="dashy">888</option>
                                <option value="deceiver">Deceiver</option>
                                <option value="onyx">Onyx</option>
                                <option value="pearl">Pearl</option>
                                <option value="optkkun">OpenTaiko-Kun</option>
                            </optgroup>

                            <optgroup label={$_('themes.optgroup.skeleton')}>
                                <option value="skeleton">Legacy</option>
                                <option value="wintry">Wintry</option>
                                <option value="modern">Modern</option>
                                <option value="rocket">Rocket</option>
                                <option value="seafoam">Seafoam</option>
                                <option value="vintage">Vintage</option>
                                <option value="sahara">Sahara</option>
                                <option value="hamlindigo">Hamlindigo</option>
                                <option value="gold-nouveau">Gold Nouveau</option>
                                <option value="crimson">Crimson</option>
                            </optgroup>
                        </select>

                        <div class="card w-full p-4 rounded-container-token">
                            <h1>{$_('themes.preview.title')}</h1>

                            <hr class="m-4">

                            <h2>{$_('themes.preview.color')}</h2>
                            <div class="grid grid-cols-1 grid-rows-3 gap-2">
                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>{$_('themes.preview.accents')}</span>
                                        <span class="badge p-2 variant-filled-primary">{$_('themes.preview.accent_primary')}</span>
                                        <span class="badge p-2 variant-filled-secondary">{$_('themes.preview.accent_secondary')}</span>
                                        <span class="badge p-2 variant-filled-tertiary">{$_('themes.preview.accent_tertiary')}</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>{$_('themes.preview.buttons')}</span>
                                        <span class="badge p-2 button-blue">{$_('themes.preview.btn_action')}</span>
                                        <span class="badge p-2 button-green">{$_('themes.preview.btn_download')}</span>
                                        <span class="badge p-2 button-gray">{$_('themes.preview.btn_repeat')}</span>
                                        <span class="badge p-2 button-red">{$_('themes.preview.btn_error')}</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>{$_('themes.preview.notifs')}</span>
                                        <span class="badge p-2 variant-filled-success">{$_('themes.preview.notif_success')}</span>
                                        <span class="badge p-2 variant-filled-warning">{$_('themes.preview.notif_warning')}</span>
                                        <span class="badge p-2 variant-filled-error">{$_('themes.preview.notif_error')}</span>
                                    </p>
                                </div>
                            </div>

                            <hr class="m-4">

                            <h2>{$_('themes.credits.title')}</h2>
                            <p>{$_('themes.credits.text')}</p>
                            <a href="https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md" target="_blank">https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md</a>

                            <hr class="m-4">

                            <h2>{$_('lang.label')}</h2>
                            <select class="select w-full max-w-[200px]" bind:value={$locale} on:change={() => SaveLocale($locale)}>
                                <option value="en">{$_('lang.en')}</option>
                                <option value="ja">{$_('lang.ja')}</option>
                                <option value="zh-Hans">{$_('lang.zh_Hans')}</option>
                                <option value="zh-Hant">{$_('lang.zh_Hant')}</option>
                                <option value="fr">{$_('lang.fr')}</option>
                                <option value="es">{$_('lang.es')}</option>
                                <option value="de">{$_('lang.de')}</option>
                                <option value="nl">{$_('lang.nl')}</option>
                                <option value="ko">{$_('lang.ko')}</option>
                                <option value="ru">{$_('lang.ru')}</option>
                            </select>
                        </div>
                    </div>

                    <div class={debugMode ? "flex gap-2" : ""}>
                        <RadioGroup>
                            <RadioItem group={debugMode ? currentThemeModeDebug : currentThemeMode} on:change={debugMode ? ThemeModeChangerDebug : ThemeModeChanger} name="justify" value={"dark"}><i class="fa-solid fa-moon"></i></RadioItem>
                            <RadioItem group={debugMode ? currentThemeModeDebug : currentThemeMode} on:change={debugMode ? ThemeModeChangerDebug : ThemeModeChanger} name="justify" value={"light"}><i class="fa-solid fa-sun"></i></RadioItem>

                            {#if (debugMode ? currentThemeModeDebug : currentThemeMode) === "dark"}
                                <p class="flex items-center px-2">{$_('themes.mode.current', { values: { mode: $_('themes.mode.dark') } })}</p>
                            {:else if (debugMode ? currentThemeModeDebug : currentThemeMode) === "light"}
                                <p class="flex items-center px-2">{$_('themes.mode.current', { values: { mode: $_('themes.mode.light') } })}</p>
                            {/if}
                        </RadioGroup>

                        {#if debugMode}
                            <button type="button" on:click={DisableDebugMode} class="button-red button-main"><i class="fa-solid fa-code"></i> {$_('themes.debug.disable_btn')}</button>
                        {/if}
                    </div>
                </div>
            </section>
        </div>
{:else}
    <!-- Hide content -->
{/if}

<style>
    /* Theme page CSS */
    option {text-align: center;}
</style>
