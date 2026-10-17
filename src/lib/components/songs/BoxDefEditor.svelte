<script lang="ts">
    // A form over box.def rather than a text box.
    //
    // A real box.def carries more than this form shows — per-language titles, scene
    // presets, character ids, directives the Hub has never heard of. Editing goes through
    // setField, which rewrites only the line it owns, so nothing is lost by saving from
    // here. Whatever the form does not cover is listed read-only at the bottom, and the
    // raw file is one click away.
    import { _ } from 'svelte-i18n';
    import { onDestroy } from 'svelte';
    import { unsavedBoxDef } from '$lib/stores/editing';
    import { parseBoxDef, serializeBoxDef, getField, setField, asColor, directivesOf, BOX_LANGS } from '$lib/utils/boxdef';
    import type { BoxDefDoc } from '$lib/utils/boxdef';

    interface Props {
        /** Raw file text; null while it loads. */
        Text: string | null;
        FileExists?: boolean;
        Saving?: boolean;
        ReadOnly?: boolean;
        OnSave?: (text: string) => void;
    }

    let { Text, FileExists = false, Saving = false, ReadOnly = false, OnSave }: Props = $props();

    let doc = $state<BoxDefDoc | null>(null);
    let loadedFrom = $state<string | null>(null);
    let showLangs = $state(false);
    let showRaw = $state(false);
    let genreHelpOpen = $state(false);
    let lang = $state<string>('JA');

    // Reload when the file changes underneath, but never while the user is mid-edit.
    $effect(() => {
        if (Text !== null && loadedFrom !== Text) {
            loadedFrom = Text;
            doc = parseBoxDef(Text);
        }
    });

    let dirty = $derived(doc !== null && Text !== null && serializeBoxDef(doc) !== Text);

    // Told to the page, which asks before another selection would throw the changes away
    $effect(() => {
        unsavedBoxDef.set(dirty && !ReadOnly);
    });
    onDestroy(() => unsavedBoxDef.set(false));

    const set = (key: string, value: string) => {
        if (!doc || ReadOnly) return;
        doc = setField(doc, key, value);
    };

    const COLORS = [
        { key: '#BOXCOLOR', label: 'songs.box.box_color' },
        { key: '#BACKCOLOR', label: 'songs.box.back_color' },
        { key: '#BGCOLOR', label: 'songs.box.bg_color' },
        { key: '#FORECOLOR', label: 'songs.box.fore_color' }
    ];

    const NUMBERS = [
        { key: '#BOXTYPE', label: 'songs.box.box_type' },
        { key: '#BGTYPE', label: 'songs.box.bg_type' },
        { key: '#BOXCHARA', label: 'songs.box.box_chara' }
    ];

    /** Directives the form already covers, so "everything else" excludes them. */
    let covered = $derived(new Set([
        '#TITLE', '#GENRE', '#BOXEXPLANATION1', '#BOXEXPLANATION2', '#BOXEXPLANATION3',
        ...COLORS.map((c) => c.key),
        ...NUMBERS.map((n) => n.key),
        ...BOX_LANGS.flatMap((l) => [
            `#TITLE${l.code}`,
            `#BOXEXPLANATION${l.code}1`,
            `#BOXEXPLANATION${l.code}2`,
            `#BOXEXPLANATION${l.code}3`
        ])
    ]));

    let others = $derived(doc ? directivesOf(doc).filter((d) => !covered.has(d.key)) : []);
</script>

{#if Text === null}
    <p class="dim">{$_('songs.folder.box_loading')}</p>
{:else if doc}
    <div class="editor">
        <div class="titlebar">
            <h4>{$_('songs.box.title')}</h4>
            {#if !FileExists}<span class="dim">{$_('songs.folder.box_missing')}</span>{/if}
        </div>

        {#if ReadOnly}
            <p class="locked">{$_('songs.box.official_locked')}</p>
        {/if}

        <label class="field">
            <span>{$_('songs.box.name')}</span>
            <input type="text" value={getField(doc, '#TITLE')} disabled={ReadOnly}
                oninput={(e) => set('#TITLE', e.currentTarget.value)}>
        </label>

        <label class="field">
            <span>
                {$_('songs.box.genre')}
                <button type="button" class="help" title={$_('songs.box.genre_help')} aria-label={$_('songs.box.genre_help')} aria-expanded={genreHelpOpen} onclick={() => genreHelpOpen = !genreHelpOpen}>?</button>
            </span>
            <input type="text" value={getField(doc, '#GENRE')} disabled={ReadOnly}
                oninput={(e) => set('#GENRE', e.currentTarget.value)}>
            {#if genreHelpOpen}<small class="hint">{$_('songs.box.genre_help')}</small>{/if}
        </label>

        <fieldset class="group">
            <legend>{$_('songs.box.description')}</legend>
            {#each [1, 2, 3] as n (n)}
                <input type="text" class="line" placeholder={$_('songs.box.line', { values: { n } })}
                    value={getField(doc, `#BOXEXPLANATION${n}`)} disabled={ReadOnly}
                    oninput={(e) => set(`#BOXEXPLANATION${n}`, e.currentTarget.value)}>
            {/each}
        </fieldset>

        <fieldset class="group">
            <legend>{$_('songs.box.colors')}</legend>
            <div class="swatches">
                {#each COLORS as c (c.key)}
                    {@const current = asColor(getField(doc, c.key))}
                    <label class="swatch">
                        <input type="color" value={current ?? '#000000'} disabled={ReadOnly}
                            oninput={(e) => set(c.key, e.currentTarget.value)}>
                        <span>{$_(c.label)}</span>
                        {#if current && !ReadOnly}
                            <button type="button" class="drop" title={$_('songs.box.clear')} onclick={() => set(c.key, '')}>✕</button>
                        {/if}
                    </label>
                {/each}
            </div>
        </fieldset>

        <fieldset class="group">
            <legend>{$_('songs.box.appearance')}</legend>
            <div class="numbers">
                {#each NUMBERS as n (n.key)}
                    <label class="num">
                        <span>{$_(n.label)}</span>
                        <input type="number" min="0" value={getField(doc, n.key)} disabled={ReadOnly}
                            oninput={(e) => set(n.key, e.currentTarget.value)}>
                    </label>
                {/each}
                <label class="num wide">
                    <span>{$_('songs.box.scene')}</span>
                    <input type="text" value={getField(doc, '#SCENEPRESET')} disabled={ReadOnly}
                        oninput={(e) => set('#SCENEPRESET', e.currentTarget.value)}>
                </label>
            </div>
        </fieldset>

        <button type="button" class="disclose" onclick={() => showLangs = !showLangs}>
            <i class="fa-solid fa-chevron-right" class:turned={showLangs}></i>
            {$_('songs.box.translations')}
        </button>
        {#if showLangs}
            <div class="langs">
                <label class="field">
                    <span>{$_('songs.box.language')}</span>
                    <select bind:value={lang} disabled={ReadOnly}>
                        {#each BOX_LANGS as l (l.code)}
                            <option value={l.code}>{l.code} ({l.name})</option>
                        {/each}
                    </select>
                </label>

                <label class="field">
                    <span>{$_('songs.box.name')}</span>
                    <input type="text" placeholder={getField(doc, '#TITLE')}
                        value={getField(doc, `#TITLE${lang}`)} disabled={ReadOnly}
                        oninput={(e) => set(`#TITLE${lang}`, e.currentTarget.value)}>
                </label>

                {#each [1, 2, 3] as n (n)}
                    <input type="text" class="line"
                        placeholder={getField(doc, `#BOXEXPLANATION${n}`) || $_('songs.box.line', { values: { n } })}
                        value={getField(doc, `#BOXEXPLANATION${lang}${n}`)} disabled={ReadOnly}
                        oninput={(e) => set(`#BOXEXPLANATION${lang}${n}`, e.currentTarget.value)}>
                {/each}
            </div>
        {/if}

        {#if others.length}
            <button type="button" class="disclose" onclick={() => showRaw = !showRaw}>
                <i class="fa-solid fa-chevron-right" class:turned={showRaw}></i>
                {$_('songs.box.others', { values: { count: others.length } })}
            </button>
            {#if showRaw}
                <ul class="others">
                    {#each others as d (d.key)}
                        <li><code>{d.key}</code><span title={d.value}>{d.value}</span></li>
                    {/each}
                </ul>
                <p class="dim">{$_('songs.box.others_note')}</p>
            {/if}
        {/if}

        {#if !ReadOnly}
            <div class="foot">
                <button type="button" class="button-green button-main" disabled={!dirty || Saving}
                    onclick={() => doc && OnSave?.(serializeBoxDef(doc))}>
                    <i class="fa-solid fa-floppy-disk"></i> {$_('songs.folder.save_box')}
                </button>
                {#if dirty}<span class="dim">{$_('songs.folder.unsaved')}</span>{/if}
            </div>
        {/if}
    </div>
{/if}

<style>
    .editor {display: flex; flex-direction: column; gap: 0.5rem;}
    .titlebar {display: flex; align-items: baseline; gap: 0.5rem;}
    .titlebar h4 {margin: 0; font-size: 0.85rem;}
    .dim {opacity: 0.7; font-size: 0.75rem;}

    .locked {
        margin: 0;
        font-size: 0.75rem;
        padding: 0.4rem 0.55rem;
        border-radius: 0.35rem;
        background-color: color-mix(in oklab, var(--color-surface-500) 16%, transparent);
    }

    .field {display: flex; flex-direction: column; gap: 0.15rem;}
    .field > span {font-size: 0.72rem; opacity: 0.8;}

    input[type='text'], input[type='number'] {
        width: 100%;
        font: inherit;
        font-size: 0.8rem;
        padding: 0.3rem 0.45rem;
        border-radius: 0.35rem;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 40%, transparent);
        background-color: light-dark(var(--color-surface-50), var(--color-surface-900));
        color: inherit;
    }
    input:disabled {opacity: 0.6; cursor: not-allowed;}

    .group {
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 25%, transparent);
        border-radius: 0.4rem;
        padding: 0.45rem 0.55rem 0.55rem;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
    }
    .group legend {font-size: 0.7rem; opacity: 0.75; padding: 0 0.25rem;}
    .line {width: 100%;}

    .swatches {display: grid; grid-template-columns: 1fr 1fr; gap: 0.35rem;}
    .swatch {display: flex; align-items: center; gap: 0.35rem; font-size: 0.72rem; min-width: 0;}
    .swatch span {overflow: hidden; text-overflow: ellipsis; white-space: nowrap;}
    input[type='color'] {
        flex: none;
        width: 1.6rem;
        height: 1.6rem;
        padding: 0;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 40%, transparent);
        border-radius: 0.3rem;
        background: none;
    }
    .drop {background: none; border: 0; color: inherit; opacity: 0.5; cursor: pointer; font-size: 0.7rem; padding: 0;}
    .drop:hover {opacity: 1;}

    .numbers {display: grid; grid-template-columns: 1fr 1fr; gap: 0.35rem;}
    .num {display: flex; flex-direction: column; gap: 0.1rem; font-size: 0.72rem; min-width: 0;}
    .num.wide {grid-column: 1 / -1;}

    .disclose {
        display: flex;
        align-items: center;
        gap: 0.4rem;
        background: none;
        border: 0;
        padding: 0.1rem 0;
        font: inherit;
        font-size: 0.76rem;
        font-weight: 700;
        color: inherit;
        cursor: pointer;
        text-align: left;
    }
    .disclose i {font-size: 0.65rem; opacity: 0.6; transition: transform 120ms;}
    .disclose i.turned {transform: rotate(90deg);}

    .langs {display: flex; flex-direction: column; gap: 0.3rem; padding-left: 0.5rem;}
    .langs select {
        width: 100%;
        font: inherit;
        font-size: 0.8rem;
        padding: 0.3rem 0.45rem;
        border-radius: 0.35rem;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 40%, transparent);
        background-color: light-dark(var(--color-surface-50), var(--color-surface-900));
        color: inherit;
    }
    .help {
        display: inline-grid;
        place-items: center;
        width: 1rem;
        height: 1rem;
        margin-left: 0.25rem;
        border-radius: 999px;
        border: 1px solid currentColor;
        background: none;
        color: inherit;
        font-size: 0.6rem;
        font-weight: 700;
        opacity: 0.65;
        cursor: help;
        vertical-align: middle;
    }
    .help:hover {opacity: 1;}
    .hint {font-size: 0.75rem; opacity: 0.75;}

    .others {list-style: none; margin: 0; padding: 0 0 0 0.5rem; display: flex; flex-direction: column; gap: 0.15rem;}
    .others li {display: flex; gap: 0.4rem; font-size: 0.72rem; margin: 0;}
    .others code {opacity: 0.75; flex: none;}
    .others span {overflow: hidden; text-overflow: ellipsis; white-space: nowrap;}

    .foot {display: flex; align-items: center; gap: 0.5rem;}
</style>
