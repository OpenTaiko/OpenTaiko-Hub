<script>
    import { onMount } from 'svelte';
    import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';

    import { getContext } from 'svelte';
    const { TriggerError, TriggerWarning, TriggerSuccess } = getContext('toast');

    import { GetRootPath } from "$lib/utils/path.js";

    import { path } from '@tauri-apps/api';

    import { RadioGroup, RadioItem } from '@skeletonlabs/skeleton';
    import { setModeCurrent } from '@skeletonlabs/skeleton';

    // Debug mode code
    let debugMode = false;

    const DisableDebugMode = async () => {
        TriggerSuccess('<b>Debug mode has been disabled.</b><br><p class="smalltext">(Note: You need to change the "debugMode" value to fully disable debug mode!)</p>');
        debugMode = false;
    }

    let currentThemeModeDebug = 'dark';

    const ThemeChangerDebug = async () => {
        const themeselect = document.getElementById("themeselect");
        const themevalue = themeselect.value;
        const themetarget = document.getElementById('themetarget');
        const themedata = themetarget.getAttribute('data-theme');
        
        if (themedata == themevalue) {
            TriggerWarning("DEBUG: Selected theme is the same as current theme.");
        } 
        else {
            currentTheme = themevalue;
            document.body.dataset.theme = themevalue;
            console.log("DEBUG: theme has been changed")
        }
    }
    
    const ThemeModeChangerDebug = async () => { 
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
    let themeDetails = 'dark';
    let themeSettingsNotFound = false;
    let themeModeSettingsNotFound = false;

    const TryFetchingCurrentTheme = async () => {
        try {
            themeSettingsNotFound = false;
            const filePathTheme = './settings/theme.json';
            const res = await GetRootPath();
            const themeFilePath = await path.join(res,filePathTheme)
            const fileContentTheme = await readTextFile(themeFilePath);
            const jsonDataTheme = JSON.parse(fileContentTheme);
            currentTheme = jsonDataTheme.theme;
            themeDetails = `${currentTheme}`;
        } catch (err) {
            themeSettingsNotFound = true;
            currentTheme = 'skeleton';
            themeDetails = "theme.json doesn't exist";
            TriggerError("The theme.json file (or the settings folder) doesn't exist or is corrupted!<br>Please reinstall the OpenTaiko Hub to fix this.");
        }

        document.body.dataset.theme = currentTheme;
    }

    const ThemeChanger = async () => {
        const themeselect = document.getElementById("themeselect");
        const themevalue = themeselect.value;
        const themetarget = document.getElementById('themetarget');
        const themedata = themetarget.getAttribute('data-theme');
        
        if (themedata == themevalue) {
            TriggerWarning("Selected theme is the same as current theme.");
        } 
        else {
            const theme_settings = ("./settings/theme.json");
            const json_theme = {"theme":themevalue}
            await writeTextFile(theme_settings, JSON.stringify(json_theme));
            
            document.body.dataset.theme = themevalue;
            console.log("theme has been changed")
        }
    }

    // Theme mode
    const thememode_settings = ("./settings/thememode.json");
    const json_modelight = {"thememode":"light"};
    const json_modedark = {"thememode":"dark"};
    
    let currentThemeMode = 'Loading...';
    let themeModeDetails = 'Loading...';

    const TryFetchingCurrentThemeMode = async () => {
        try {
            themeModeSettingsNotFound = false;
            const filePathMode = './settings/thememode.json';
            const res = await GetRootPath();
            const modeFilePath = await path.join(res,filePathMode)
            const fileContentMode = await readTextFile(modeFilePath);
            const jsonDataMode = JSON.parse(fileContentMode);
            currentThemeMode = jsonDataMode.thememode;
            themeModeDetails = `${currentThemeMode}`;
        } catch (err) {
            themeModeSettingsNotFound = true;
            currentThemeMode = 'dark';
            themeModeDetails = "thememode.json doesn't exist";
            TriggerError("The thememode.json file (or the settings folder) doesn't exist or is corrupted!<br>Please reinstall the OpenTaiko Hub to fix this.");
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

    const ThemeModeChanger = async () => {
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

    // Tooltips
    

    onMount(async () => {
        TryFetchingCurrentTheme();
        TryFetchingCurrentThemeMode();
    });
</script>

{#if currentTile === 7}
    {#if debugMode === false}
        <div id="themetab">
            <section class="card w-full">
                <div class="p-4 space-y-4">
                    <h1>Select a theme.</h1>
                    <div class="flex gap-2">
                        <!-- value={currentTheme} on:change={themeChanger} on:click={TryFetchingCurrentTheme} -->
                        <select id="themeselect" size="11" class="select w-full max-w-[265px]" value={currentTheme} on:change={ThemeChanger} on:click={TryFetchingCurrentTheme}>
                            <optgroup label="OpenTaiko Hub themes:">
                                <option value="gleamingsky">Gleaming Sky</option>
                                <option value="dashy">888</option>
                                <option value="deceiver">Deceiver</option>
                                <option value="onyx">Onyx</option>
                                <option value="pearl">Pearl</option>
                                <option value="optkkun">OpenTaiko-Kun</option>
                            </optgroup>

                            <optgroup label="Skeleton preset themes:">
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
                            <h1>Welcome to the OpenTaiko Hub Themes tab!</h1>

                            <hr class="m-4">
                                    
                            <h2>Color preview</h2>
                            <div class="grid grid-cols-1 grid-rows-3 gap-2">
                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>Theme accents:</span>                        
                                        <span class="badge p-2 variant-filled-primary">Primary accent</span>
                                        <span class="badge p-2 variant-filled-secondary">Secondary accent</span>
                                        <span class="badge p-2 variant-filled-tertiary">Tertiary accent</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>Button colors:</span>
                                        <span class="badge p-2 button-blue">Action</span>
                                        <span class="badge p-2 button-green">Download</span>
                                        <span class="badge p-2 button-gray">Repeat</span>
                                        <span class="badge p-2 button-red ">Destructive/Error</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>Notifications:</span>
                                        <span class="badge p-2 variant-filled-success">Success</span>
                                        <span class="badge p-2 variant-filled-warning">Warning</span>
                                        <span class="badge p-2 variant-filled-error">Error</span>
                                    </p>
                                </div>
                            </div>
                                    
                            <hr class="m-4">

                            <h2>Theme credits</h2>
                            <p>All theme credits can be found here:</p>
                            <a href="https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md" target="_blank">https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md</a>
                            
                        </div>
                    </div>
                            
                    <RadioGroup>
                        <RadioItem bind:group={currentThemeMode} on:change={ThemeModeChanger} name="justify" value={"dark"}><i class="fa-solid fa-moon"></i></RadioItem>
                        <RadioItem bind:group={currentThemeMode} on:change={ThemeModeChanger} name="justify" value={"light"}><i class="fa-solid fa-sun"></i></RadioItem>
                        
                        {#if currentThemeMode === "dark"}
                            <p class="flex items-center px-2">Current Mode: Dark</p>
                        {:else if currentThemeMode === "light"}
                            <p class="flex items-center px-2">Current Mode: Light</p>
                        {/if}
                    </RadioGroup>
                </div>
            </section>
        </div>
    {:else if debugMode === true}
        <div id="themetab">
            <section class="card w-full">
                <div class="p-4 space-y-4">
                    <h1>Select a theme. <b>(DEBUG MODE ON)</b></h1>
                    <div class="flex gap-2">
                        <!-- value={currentTheme} on:change={themeChanger} on:click={TryFetchingCurrentTheme} -->
                        <select id="themeselect" size="11" class="select w-full max-w-[265px]" value={currentTheme} on:change={ThemeChangerDebug}>
                            <optgroup label="OpenTaiko Hub themes:">
                                <option value="gleamingsky">Gleaming Sky</option>
                                <option value="dashy">888</option>
                                <option value="deceiver">Deceiver</option>
                                <option value="onyx">Onyx</option>
                                <option value="pearl">Pearl</option>
                                <option value="optkkun">OpenTaiko-Kun</option>
                            </optgroup>

                            <optgroup label="Skeleton preset themes:">
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
                            <h1>Welcome to the OpenTaiko Hub Themes tab!</h1>

                            <hr class="m-4">
                                    
                            <h2>Color preview</h2>
                            <div class="grid grid-cols-1 grid-rows-3 gap-2">
                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>Theme accents:</span>                        
                                        <span class="badge p-2 variant-filled-primary">Primary accent</span>
                                        <span class="badge p-2 variant-filled-secondary">Secondary accent</span>
                                        <span class="badge p-2 variant-filled-tertiary">Tertiary accent</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>Button colors:</span>
                                        <span class="badge p-2 button-blue">Action</span>
                                        <span class="badge p-2 button-green">Download</span>
                                        <span class="badge p-2 button-gray">Repeat</span>
                                        <span class="badge p-2 button-red ">Destructive/Error</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>Notifications:</span>
                                        <span class="badge p-2 variant-filled-success">Success</span>
                                        <span class="badge p-2 variant-filled-warning">Warning</span>
                                        <span class="badge p-2 variant-filled-error">Error</span>
                                    </p>
                                </div>
                            </div>
                                    
                            <hr class="m-4">

                            <h2>Theme credits</h2>
                            <p>All theme credits can be found here:</p>
                            <a href="https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md" target="_blank">https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md</a>
                        </div>
                    </div>
                            
                    <div class="flex gap-2">
                        <RadioGroup>
                            <RadioItem bind:group={currentThemeModeDebug} on:change={ThemeModeChangerDebug} name="justify" value={"dark"}><i class="fa-solid fa-moon"></i></RadioItem>
                            <RadioItem bind:group={currentThemeModeDebug} on:change={ThemeModeChangerDebug} name="justify" value={"light"}><i class="fa-solid fa-sun"></i></RadioItem>
                            
                            {#if currentThemeModeDebug === "dark"}
                                <p class="flex items-center px-2">Current Mode: Dark</p>
                            {:else if currentThemeModeDebug === "light"}
                                <p class="flex items-center px-2">Current Mode: Light</p>
                            {/if}
                        </RadioGroup>

                        <button type="button" on:click={DisableDebugMode} class="button-red button-main"><i class="fa-solid fa-code"></i> Disable debug mode</button>
                    </div>
                </div>
            </section>
        </div>
    {/if}
{:else}
    <!-- Hide content -->
{/if}

<style>
    /* Theme page CSS */
    option {text-align: center;}
</style>